use core::{cmp, ops::{Deref, Range}};

use alloc::{collections::btree_map::BTreeMap, sync::Arc, vec::Vec};

use hal::{addr::{PhysAddr, PhysAddrHal, PhysPageNum, PhysPageNumHal, RangePPNHal, VirtAddr, VirtAddrHal, VirtPageNum, VirtPageNumHal}, allocator::FrameAllocatorHal, common::FrameTracker, constant::{Constant, ConstantsHal}, instruction::{Instruction, InstructionHal}, pagetable::{MapPerm, PTEFlags, PageLevel, PageTableEntry, PageTableEntryHal, PageTableHal, VpnPageRangeIter}, println, util::smart_point::StrongArc};
use log::{info, Level};
use range_map::RangeMap;
use xmas_elf::reader::Reader;

use crate::{config::PAGE_SIZE, fs::{page, utils::FileReader, vfs::File}, mm::{allocator::{FrameAllocator, SlabAllocator}, vm::KernVmAreaType, PageTable}, sync::mutex::{spin_mutex::SpinMutex, MutexSupport}, syscall::SysError, task::utils::{generate_early_auxv, AuxHeader, AT_BASE, AT_PHDR, AT_RANDOM}, utils::round_down_to_page};

use crate::syscall::{mm::MmapFlags, SysResult};

use super::{KernVmArea, KernVmSpaceHal, PageFaultAccessType, UserVmArea, UserVmAreaType, UserVmSpaceHal};

#[allow(missing_docs, unused)]
pub struct KernVmSpace {
    page_table: PageTable,
    areas: RangeMap<VirtAddr, KernVmArea>,
}

#[allow(missing_docs, unused)]
pub struct UserVmSpace {
    page_table: PageTable,
    areas: RangeMap<VirtAddr, UserVmArea>,
    heap_bottom_va: VirtAddr,
}

impl KernVmSpace {
    /// The second-level page table in the kernel virtual mapping area is pre-allocated to avoid synchronization
    fn map_vm_area_huge_pages(&mut self) {
        let ptes = self.page_table.root_ppn
            .start_addr().get_mut::<[PageTableEntry; Constant::PTES_PER_PAGE]>();

        const HUGE_PAGES: usize = Constant::KERNEL_VM_SIZE / (Constant::PAGE_SIZE * 512 * 512);
        const VM_START: usize = (Constant::KERNEL_VM_BOTTOM & ((1 << Constant::VA_WIDTH)-1)) / (Constant::PAGE_SIZE * 512 * 512);
        let range_ppn = FrameAllocator.alloc(HUGE_PAGES).unwrap();
        range_ppn.get_slice_mut::<u8>().fill(0);
        let ppn = range_ppn.start;
        for (i, pte_i) in (VM_START..VM_START+HUGE_PAGES).enumerate() {
            ptes[pte_i] = PageTableEntry::new(ppn+i, MapPerm::empty(), true);
        }
    }
}

impl KernVmSpaceHal for KernVmSpace {

    fn enable(&self) {
        unsafe {
            self.page_table.enable_high();
        }
    }

    fn new() -> Self{

        unsafe extern "C" {
            fn stext();
            fn etext();
            fn srodata();
            fn erodata();
            fn sdata();
            fn edata();
            fn sbss_with_stack();
            fn ebss();
            fn ekernel();
        }

        let mut ret = Self {
            page_table: PageTable::new_in(0, FrameAllocator),
            areas: RangeMap::new(),
        };

        ret.map_vm_area_huge_pages();

        ret.push_area(KernVmArea::new(
                (stext as usize).into()..(etext as usize).into(), 
                KernVmAreaType::Data, 
                MapPerm::R | MapPerm::X,
            ),
            None
        );

        ret.push_area(KernVmArea::new(
                (srodata as usize).into()..(erodata as usize).into(), 
                KernVmAreaType::Data, 
                MapPerm::R,
            ),
            None
        );

        ret.push_area(KernVmArea::new(
                (sdata as usize).into()..(edata as usize).into(), 
                KernVmAreaType::Data, 
                MapPerm::R | MapPerm::W,
            ),
            None
        );

        ret.push_area(KernVmArea::new(
                (sdata as usize).into()..(edata as usize).into(), 
                KernVmAreaType::Data, 
                MapPerm::R | MapPerm::W,
            ),
            None
        );

        ret.push_area(KernVmArea::new(
                (sbss_with_stack as usize).into()..(ebss as usize).into(), 
                KernVmAreaType::Data, 
                MapPerm::R | MapPerm::W, 
            ),
            None
        );

        ret.push_area(KernVmArea::new(
                Constant::KERNEL_STACK_BOTTOM.into()..Constant::KERNEL_STACK_TOP.into(), 
                KernVmAreaType::KernelStack, 
                MapPerm::R | MapPerm::W,
            ),
            None
        );

        ret.push_area(KernVmArea::new(
                Constant::SIGRET_TRAMPOLINE_BOTTOM.into()..Constant::SIGRET_TRAMPOLINE_TOP.into(), 
                KernVmAreaType::SigretTrampoline, 
                MapPerm::R | MapPerm::X | MapPerm::U,
            ),
            None
        );

        ret.push_area(KernVmArea::new(
                (ekernel as usize).into()..(Constant::MEMORY_END + Constant::KERNEL_ADDR_SPACE.start).into(), 
                KernVmAreaType::PhysMem, 
                MapPerm::R | MapPerm::W,
            ),
            None
        );
        
        for pair in hal::board::MMIO {
            ret.push_area(
                KernVmArea::new(
                    ((*pair).0 + Constant::KERNEL_ADDR_SPACE.start).into()..((*pair).0 + Constant::KERNEL_ADDR_SPACE.start + (*pair).1).into(),
                    KernVmAreaType::MemMappedReg, 
                    MapPerm::R | MapPerm::W,
                ),
                None
            );
        }
        ret
    }
    
    fn push_area(&mut self, mut area: KernVmArea, data: Option<&[u8]>) {
        area.map(&mut self.page_table);
        if let Some(data) = data{
            area.copy_data(&mut self.page_table, data);
        }
        let _ = self.areas.try_insert(area.range_va.clone(), area);
    }
    
    fn translate_vpn(&self, vpn: VirtPageNum) -> Option<PhysPageNum>{
        self.page_table.translate_vpn(vpn)
    }
    
    fn translate_va(&self, va: VirtAddr) -> Option<PhysAddr> {
        self.page_table.translate_va(va)
    }
    
    fn map_vm_area(&mut self, frames: Vec<StrongArc<crate::mm::FrameTracker, SlabAllocator>>, map_perm: MapPerm) -> Option<Range<VirtPageNum>> {
        let range_va = self.areas.find_free_range(
            Constant::KERNEL_VM_BOTTOM.into()..Constant::KERNEL_VM_TOP.into(), 
            frames.len() << Constant::PAGE_SIZE_BITS
        )?;
        assert!(range_va.start.0 % Constant::PAGE_SIZE == 0);
        let range_vpn = range_va.start.floor()..range_va.end.ceil();

        let mut vma = KernVmArea::new(range_va, KernVmAreaType::VirtMemory, map_perm);

        range_vpn.clone()
            .enumerate()
            .map(|(i, vpn)| (vpn, &frames[i]) )
            .for_each(|(vpn, frame)| {
                vma.frames.insert(vpn, frame.clone());
            });

        self.push_area(vma, None);
            
        Some(range_vpn)
    }
    
    fn unmap_vm_area(&mut self, range_vpn: Range<VirtPageNum>) {
        let mut left: KernVmArea;
        let right: KernVmArea;
        if let Some(area) = self.areas.get_mut(range_vpn.start.start_addr()) {
            let range_va = area.range_va.clone();
            left = self.areas.force_remove_one(range_va);
            let mut mid = left.split_off(range_vpn.start);
            mid.unmap(&mut self.page_table);
            right = mid.split_off(range_vpn.end);
        } else {
            return;
        }
        if !left.range_va.is_empty() {
            let _ = self.areas.try_insert(left.range_va.clone(), left);
        }
        if !right.range_va.is_empty() {
            let _ = self.areas.try_insert(right.range_va.clone(), right);
        }
    }

}

impl UserVmSpace {
    fn find_heap(&mut self) -> Option<&mut UserVmArea> {
        let area = self.areas.get_mut(self.heap_bottom_va)?;
        if area.vma_type == UserVmAreaType::Heap {
            Some(area)
        } else {
            None
        }
    }
}

impl UserVmSpaceHal for UserVmSpace {

    fn new() -> Self {
        Self {
            page_table: PageTable::new_in(0, FrameAllocator),
            areas: RangeMap::new(),
            heap_bottom_va: VirtAddr(0)
        }
    }

    fn get_page_table(&self) -> &PageTable {
        &self.page_table
    }

    fn from_kernel(kvm_space: &KernVmSpace) -> Self {
        let ret = Self {
            page_table: PageTable::new_in(0, FrameAllocator),
            areas: RangeMap::new(),
            heap_bottom_va: VirtAddr(0)
        };

        ret.page_table.root_ppn
            .start_addr()
            .get_mut::<[PageTableEntry; 512]>()[256..]
            .copy_from_slice(
                &kvm_space.page_table.root_ppn
                    .start_addr()
                    .get_mut::<[PageTableEntry; 512]>()[256..]
            );

        ret
    }

    fn from_elf(elf_data: &[u8], kvm_space: &KernVmSpace) -> (Self, super::VmSpaceUserStackTop, super::VmSpaceEntryPoint, Vec<AuxHeader>) {
        let mut ret = Self::from_kernel(kvm_space);
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let entry = elf_header.pt2.entry_point() as usize;
        let ph_count = elf_header.pt2.ph_count();
        let ph_entry_size = elf_header.pt2.ph_entry_size() as usize;
        let mut max_end_vpn = VirtPageNum(0);
        let mut header_va = 0;
        let mut has_found_header_va = false;

        // extract the aux
        let mut auxv = generate_early_auxv(ph_entry_size, ph_count as usize, entry);
        auxv.push(AuxHeader::new(AT_BASE, 0));
        
        // map the elf data to user space
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                log::debug!("i: {}, start_va: {:#x}, end_va: {:#x}", i, start_va.0, end_va.0);
                if !has_found_header_va {
                    header_va = start_va.0;
                    has_found_header_va = true;
                }

                let mut map_perm = MapPerm::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapPerm::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPerm::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPerm::X;
                }
                let map_area = UserVmArea::new(
                    start_va..end_va, 
                    UserVmAreaType::Data,
                    map_perm,
                );
                max_end_vpn = map_area.range_vpn().end;
                log::debug!("{:?}", &elf.input.read(ph.offset() as usize, 4));
                let elf_offset_start = PhysAddr::from(ph.offset() as usize).floor().start_addr().0;
                let elf_offset_end = (ph.offset() + ph.file_size()) as usize;
                log::debug!("{:x} aligned to {:x}, now pushing ({:x}, {:x})", ph.offset() as usize, elf_offset_start, elf_offset_start, elf_offset_end);
                // warning: now only aligned the load data to page.
                // will same page have different usage?
                ret.push_area(
                    map_area,
                    Some(&elf.input[elf_offset_start..elf_offset_end]),
                );
            }
        };

        let ph_head_addr = header_va + elf.header.pt2.ph_offset() as usize;
        auxv.push(AuxHeader::new(AT_RANDOM, ph_head_addr));
        auxv.push(AuxHeader::new(AT_PHDR, ph_head_addr));
        
        // todo: should check if a elf file is dynamic link
        auxv.push(AuxHeader::new(AT_BASE, 0));

        
        let max_end_va: VirtAddr = max_end_vpn.start_addr();
        ret.heap_bottom_va = max_end_va;

        // map user stack with U flags
        let user_stack_bottom = Constant::USER_STACK_BOTTOM;
        let user_stack_top = Constant::USER_STACK_TOP;
        log::debug!("user_stack_bottom: {:#x}, user_stack_top: {:#x}", user_stack_bottom, user_stack_top);
        ret.push_area(
            UserVmArea::new(
                user_stack_bottom.into()..user_stack_top.into(),
                UserVmAreaType::Stack,
                MapPerm::R | MapPerm::W | MapPerm::U,
            ),
            None,
        );
        
        log::debug!("trap_context: {:#x}", Constant::USER_TRAP_CONTEXT_BOTTOM);
        // map TrapContext
        let mut trap_cx_area = UserVmArea::new(
            Constant::USER_TRAP_CONTEXT_BOTTOM.into()..(Constant::USER_TRAP_CONTEXT_TOP).into(),
            UserVmAreaType::TrapContext,
            MapPerm::R | MapPerm::W,
        );
        trap_cx_area.alloc_frames();
        ret.push_area(
            trap_cx_area,
            None,
        );
        (
            ret,
            user_stack_top,
            entry,
            auxv,
        )
    }

    fn from_elf_file(elf_file: Arc<dyn File>, kvm_space: &SpinMutex<KernVmSpace, impl MutexSupport>) -> (Self, super::VmSpaceUserStackTop, super::VmSpaceEntryPoint, Vec<AuxHeader>) {
        let mut ret = Self::from_kernel(kvm_space.lock().deref());
        let reader = FileReader::new(elf_file.inode().unwrap());
        let elf = xmas_elf::ElfFile::new(&reader).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let entry = elf_header.pt2.entry_point() as usize;
        let ph_count = elf_header.pt2.ph_count();
        let ph_entry_size = elf_header.pt2.ph_entry_size() as usize;
        let mut max_end_vpn = VirtPageNum(0);
        let mut header_va = 0;
        let mut has_found_header_va = false;

        // extract the aux
        let mut auxv = generate_early_auxv(ph_entry_size, ph_count as usize, entry);
        auxv.push(AuxHeader::new(AT_BASE, 0));
        
        // map the elf data to user space
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                log::debug!("i: {}, start_va: {:#x}, end_va: {:#x}", i, start_va.0, end_va.0);
                if !has_found_header_va {
                    header_va = start_va.0;
                    has_found_header_va = true;
                }

                let mut map_perm = MapPerm::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapPerm::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPerm::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPerm::X;
                }
               
                log::debug!("{:?}", &elf.input.read(ph.offset() as usize, 4));                
                let elf_offset_start = PhysAddr::from(ph.offset() as usize).floor().start_addr().0;
                let elf_offset_end = (ph.offset() + ph.file_size()) as usize;
                log::debug!("{:x} aligned to {:x}, now pushing ({:x}, {:x})", ph.offset() as usize, elf_offset_start, elf_offset_start, elf_offset_end);
                
                let mut map_area = UserVmArea::new(
                    start_va..end_va, 
                    UserVmAreaType::Data,
                    map_perm,
                );
                map_area.file = Some(elf_file.clone());
                map_area.offset = elf_offset_start;
                map_area.len = elf_offset_end - elf_offset_start;

                max_end_vpn = map_area.range_vpn().end;
                ret.push_area(
                    map_area,
                    None
                    // Some(elf.input.read(elf_offset_start, elf_offset_end-elf_offset_start))
                );
            }
        };

        let ph_head_addr = header_va + elf.header.pt2.ph_offset() as usize;
        auxv.push(AuxHeader::new(AT_RANDOM, ph_head_addr));
        auxv.push(AuxHeader::new(AT_PHDR, ph_head_addr));
        
        // todo: should check if a elf file is dynamic link
        auxv.push(AuxHeader::new(AT_BASE, 0));

        ret.heap_bottom_va = max_end_vpn.start_addr();

        // map user stack with U flags
        let user_stack_bottom = Constant::USER_STACK_BOTTOM;
        let user_stack_top = Constant::USER_STACK_TOP;
        log::debug!("user_stack_bottom: {:#x}, user_stack_top: {:#x}", user_stack_bottom, user_stack_top);
        ret.push_area(
            UserVmArea::new(
                user_stack_bottom.into()..user_stack_top.into(),
                UserVmAreaType::Stack,
                MapPerm::R | MapPerm::W | MapPerm::U,
            ),
            None,
        );
        
        log::debug!("trap_context: {:#x}", Constant::USER_TRAP_CONTEXT_BOTTOM);
        
        let mut trap_cx_area = UserVmArea::new(
            Constant::USER_TRAP_CONTEXT_BOTTOM.into()..(Constant::USER_TRAP_CONTEXT_TOP).into(),
            UserVmAreaType::TrapContext,
            MapPerm::R | MapPerm::W,
        );
        trap_cx_area.alloc_frames();
        // map TrapContext
        ret.push_area(
            trap_cx_area,
            None,
        );
        
        (
            ret,
            user_stack_top,
            entry,
            auxv,
        )
    }

    fn push_area(&mut self, area: UserVmArea, data: Option<&[u8]>) ->&mut UserVmArea {
        match self.areas.try_insert(area.range_va.clone(), area) {
            Ok(area) => {
                if let Some(data) = data{
                    area.copy_data(&mut self.page_table, data);
                } 
                area.map(&mut self.page_table);
                area
            },
            Err(_) => panic!("[push_area] fail")
        }
    }

    fn reset_heap_break(&mut self, new_brk: VirtAddr) -> VirtAddr {
        let heap = match self.find_heap() {
            Some(heap) => heap,
            None => {
                if new_brk > self.heap_bottom_va {
                    self.push_area(
                        UserVmArea::new(
                            self.heap_bottom_va..new_brk,
                            UserVmAreaType::Heap,
                            MapPerm::R | MapPerm::W | MapPerm::U,
                        ), 
                        None
                    )
                } else {
                    return self.heap_bottom_va;
                }
            }
        };
        let range = heap.range_va.clone();
        if new_brk >= range.end {
            match self.areas.extend_back(range.start..new_brk) {
                Ok(_) => {}
                Err(_) => return range.end
            }
        } else if new_brk > range.start {
            match self.areas.reduce_back(range.start..new_brk) {
                Ok(_) => {}
                Err(_) => return range.end
            }
        } else {
            return range.end;
        }

        let heap = self.find_heap().unwrap();
        if new_brk >= range.end {
            heap.range_va = range.start..new_brk;
            new_brk
        } else if new_brk > range.start {
            let mut right = heap.split_off(new_brk.ceil());
            right.unmap(&mut self.page_table);
            new_brk
        } else {
            range.end
        }
    }

    fn handle_page_fault(&mut self, va: VirtAddr, access_type: super::PageFaultAccessType) -> Result<(), ()> {
        let area = self.areas.get_mut(va).ok_or(())?;
        area.handle_page_fault(&mut self.page_table, va.floor(), access_type)
    }
    
    fn from_existed(uvm_space: &mut Self, kvm_space: &KernVmSpace) -> Self {
        let mut ret = Self::from_kernel(kvm_space);
        ret.heap_bottom_va = uvm_space.heap_bottom_va;
        for (_, area) in uvm_space.areas.iter_mut() {
            if let Ok(new_area) = area.clone_cow(&mut uvm_space.page_table) {
                ret.push_area(new_area, None);
            } else {
                ret.push_area(area.clone(), None);
            }
        }
        ret
    }
    
    fn alloc_mmap_area(&mut self, va: VirtAddr, len: usize, perm: MapPerm, flags: MmapFlags, file: Arc<dyn File>, offset: usize) -> SysResult {
        assert!(va.0 % PAGE_SIZE == 0);
        let range = if flags.contains(MmapFlags::MAP_FIXED) && 
        self.areas.is_range_free(va..va+len).is_ok() {
            va..va + len
        } else {
            self.areas
            .find_free_range(VirtAddr::from(Constant::USER_FILE_BEG)..Constant::USER_FILE_END.into(), len)
            .ok_or(SysError::ENOMEM)?
        };
        let start = range.start;
        let page_table = &mut self.page_table;
        let inode = file.inode().unwrap();
        let mut vma = UserVmArea::new_mmap(range, perm, flags, Some(file.clone()), offset, len);
        let mut range_vpn = vma.range_vpn();
        let length = cmp::min(len, Constant::USER_FILE_PER_PAGES * PAGE_SIZE);
        // the offset is already page aligned
        for page_offset in (offset..offset + length).step_by(PAGE_SIZE) {
            // get the cached page
            if let Some(page) = inode.clone().read_page_at(page_offset) {
                // page already in cache
                let vpn = range_vpn.next().unwrap();
                if flags.contains(MmapFlags::MAP_PRIVATE) {
                    // private mode: map in COW
                    let mut new_perm = perm;
                    new_perm.remove(MapPerm::W);
                    new_perm.insert(MapPerm::C);
                    // map a single page
                    page_table.map(vpn, page.ppn(), new_perm, PageLevel::Small);
                    vma.frames.insert(vpn, StrongArc::clone(&page.frame()));
                    vma.map_perm.insert(MapPerm::C);
                    // update tlb                     
                    unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
                } else {
                    // share mode
                    info!("[alloc_mmap_area]: mapping vpn:{:x} to ppn:{:x}", vpn.0, page.ppn().0);
                    page_table.map(vpn, page.ppn(), perm, PageLevel::Small);
                    vma.frames.insert(vpn, StrongArc::clone(&page.frame()));
                    unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
                }
            } else {
                // reach EOF
                break;
            }
        }
        self.push_area(vma, None);
        Ok(start.0 as isize)
    }

    fn alloc_anon_area(&mut self, va: VirtAddr, len: usize, perm: MapPerm, flags: MmapFlags, is_share: bool) -> SysResult {
        assert!(va.0 % PAGE_SIZE == 0);
        let range = if flags.contains(MmapFlags::MAP_FIXED) {
            va..va + len
        } else {
            self.areas
            .find_free_range(VirtAddr::from(Constant::USER_SHARE_BEG)..Constant::USER_SHARE_END.into(), len)
            .ok_or(SysError::ENOMEM)?
        };
        let start = range.start;
        if is_share {
            let vma = UserVmArea::new(range, UserVmAreaType::Shm, perm);
            self.push_area(vma, None);
        } else {
            let vma = UserVmArea::new_mmap(range, perm, flags, None, 0, len);
            self.push_area(vma, None);

        }
        Ok(start.0 as isize)
    }

    fn unmap(&mut self, va: VirtAddr, len: usize) -> SysResult {
        let mut left: UserVmArea;
        let right: UserVmArea;
        if let Some(area) = self.areas.get_mut(va) {
            let range_va = area.range_va.clone();
            left = self.areas.force_remove_one(range_va);
            let mut mid = left.split_off(va.floor());
            mid.unmap(&mut self.page_table);
            right = mid.split_off((va + len).ceil());
        } else {
            return Ok(0);
        }
        if !left.range_va.is_empty() {
            self.areas.try_insert(left.range_va.clone(), left).map_err(|_| SysError::EFAULT)?;
        }
        if !right.range_va.is_empty() {
            self.areas.try_insert(right.range_va.clone(), right).map_err(|_| SysError::EFAULT)?;
        }
        Ok(0)
    }

}

#[allow(missing_docs, unused)]
impl KernVmArea {

    fn range_vpn(&self) -> Range<VirtPageNum> {
        self.range_va.start.floor()..self.range_va.end.ceil()
    }

    fn copy_data(&mut self, page_table: &PageTable, data: &[u8]) {
        let mut start: usize = 0;
        let len = data.len();
        for vpn in self.range_vpn() {
            let src = &data[start..len.min(start + Constant::PAGE_SIZE)];
            if let Some(ppn)  = page_table.translate_vpn(vpn) {
                let dst = &mut ppn.start_addr()
                    .get_mut::<[u8; Constant::PAGE_SIZE]>()[..src.len()];
                dst.copy_from_slice(src);
                start += Constant::PAGE_SIZE;
                if start >= len {
                    break;
                }
            } else {
                panic!("copy data to unmap frame");
            }
        }
    }

    fn split_off(&mut self, p: VirtPageNum) -> Self {
        let ret = Self {
            range_va: p.start_addr()..self.range_va.end,
            frames: self.frames.split_off(&p),
            map_perm: self.map_perm,
            vma_type: self.vma_type,
        };
        self.range_va = self.range_va.start..p.start_addr();
        ret
    }

    fn map_range_to(&self, page_table: &mut PageTable, range_vpn: Range<VirtPageNum>, mut start_ppn: PhysPageNum) {
        VpnPageRangeIter::new(range_vpn)
        .for_each(|(vpn, level)| {
            let ppn = PhysPageNum(start_ppn.0);
            start_ppn += level.page_count();
            page_table.map(vpn, ppn, self.map_perm, level);
        });
    }

    fn map(&self, page_table: &mut PageTable) {
        unsafe extern "C" {
            fn kernel_stack_bottom();
            fn sigreturn_trampoline();
        }
        let range_vpn = self.range_va.start.floor()..self.range_va.end.ceil();
        match self.vma_type {
            KernVmAreaType::Data |
            KernVmAreaType::PhysMem |
            KernVmAreaType::MemMappedReg => {
                self.map_range_to(
                    page_table,
                    range_vpn.clone(), 
                    PhysPageNum(range_vpn.start.0 & !(Constant::KERNEL_ADDR_SPACE.start >> Constant::PAGE_SIZE_BITS))
                );
            },
            KernVmAreaType::SigretTrampoline => {
                self.map_range_to(
                    page_table, 
                    range_vpn.clone(),
                    PhysPageNum((sigreturn_trampoline as usize & !(Constant::KERNEL_ADDR_SPACE.start)) >> 12)
                );
            }
            KernVmAreaType::KernelStack => {
                self.map_range_to(
                    page_table, 
                    range_vpn.clone(),
                    PhysPageNum((kernel_stack_bottom as usize & !(Constant::KERNEL_ADDR_SPACE.start)) >> 12)
                );
            },
            KernVmAreaType::VirtMemory => {
                for (&vpn, frame) in self.frames.iter() {
                    page_table.map(vpn, frame.range_ppn.start, self.map_perm, PageLevel::Small);
                }
            },
        }
    }

    fn unmap(&mut self, page_table: &mut PageTable) {
        let range_vpn = self.range_vpn();
        for vpn in range_vpn {
            page_table.unmap(vpn);
            unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
        }
    }
}

#[allow(missing_docs, unused)]
impl UserVmArea {

    fn range_vpn(&self) -> Range<VirtPageNum> {
        self.range_va.start.floor()..self.range_va.end.ceil()
    }

    fn copy_data(&mut self, page_table: &mut PageTable, data: &[u8]) {
        for (vpn, src) in self.range_vpn().zip(data.chunks(Constant::PAGE_SIZE)) {
            let ppn;
            if let Some(_ppn) = page_table.translate_vpn(vpn) {
                ppn = _ppn;
            } else {
                let frame = FrameAllocator.alloc_tracker(1).unwrap();
                ppn = frame.range_ppn.start;
                self.frames.insert(vpn, StrongArc::new_in(frame, SlabAllocator));
            }
            let dst = &mut ppn
                    .start_addr()
                    .get_mut::<[u8; Constant::PAGE_SIZE]>();
            dst[..src.len()].copy_from_slice(src);
            dst[src.len()..].fill(0);
        }
    }

    fn split_off(&mut self, p: VirtPageNum) -> Self {
        let new_offset ;
        let new_len;
        if self.file.is_some() {
            new_offset = self.offset + (p.0 - self.range_vpn().start.0) * Constant::PAGE_SIZE;
            new_len = if new_offset - self.offset > self.len {
                0
            } else {
                self.len - (new_offset - self.offset)
            };
            self.len -= new_len;
        } else {
            new_offset = 0;
            new_len = 0;
        }

        let ret = Self {
            range_va: p.start_addr()..self.range_va.end,
            frames: self.frames.split_off(&p),
            map_perm: self.map_perm,
            vma_type: self.vma_type,
            file: self.file.clone(),
            offset: new_offset,
            mmap_flags: self.mmap_flags,
            len: new_len
        };
        self.range_va = self.range_va.start..p.start_addr();
        ret
    }
    
    fn map_range_to(&self, page_table: &mut PageTable, range_vpn: Range<VirtPageNum>, mut start_ppn: PhysPageNum) {
        VpnPageRangeIter::new(range_vpn)
        .for_each(|(vpn, level)| {
            let ppn = PhysPageNum(start_ppn.0);
            start_ppn += level.page_count();
            page_table.map(vpn, ppn, self.map_perm, level);
        });
    }

    fn alloc_frames(&mut self) {
        for vpn in self.range_vpn() {
            let frame = FrameAllocator.alloc_tracker(1).unwrap();
            self.frames.insert(vpn, StrongArc::new_in(frame, SlabAllocator));
        }
    }

    fn map(&mut self, page_table: &mut PageTable) {
        for (&vpn, frame) in self.frames.iter() {
            let level = PageLevel::from_count(frame.range_ppn.clone().count())
                                    .expect("unsupported frames count");
            page_table.map(vpn, frame.range_ppn.start, self.map_perm, level);
        }
    }

    fn unmap(&mut self, page_table: &mut PageTable) {
        for &vpn in self.frames.keys() {
            page_table.unmap(vpn);
            unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
        }
    }

    fn clone_cow(&mut self, page_table: &mut PageTable) -> Result<Self, ()> {
        // note: trap context cannot supprt COW
        if self.vma_type == UserVmAreaType::TrapContext {
            return Err(());
        }
        // note: don't set C flag for readonly frames
        if self.map_perm.contains(MapPerm::W) {
            self.map_perm.insert(MapPerm::C);
            self.map_perm.remove(MapPerm::W);
            for &vpn in self.frames.keys() {
                let (pte, _) = page_table.find_pte(vpn).unwrap();
                pte.set_flags(PTEFlags::from(self.map_perm) | PTEFlags::V);
                unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
            }
        } else if self.map_perm.contains(MapPerm::C) {
            for &vpn in self.frames.keys() {
                let (pte, _) = page_table.find_pte(vpn).unwrap();
                pte.set_flags(PTEFlags::from(self.map_perm) | PTEFlags::V);
                unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
            }
        }
        Ok(Self {
            range_va: self.range_va.clone(), 
            frames: self.frames.clone(), 
            map_perm: self.map_perm.clone(), 
            vma_type: self.vma_type.clone(),
            file: self.file.clone(),
            mmap_flags: self.mmap_flags.clone(),
            offset: self.offset,
            len: self.len
        })
    }

    fn handle_page_fault(&mut self, 
        page_table: &mut PageTable, 
        vpn: VirtPageNum,
        access_type: PageFaultAccessType
    ) -> Result<(), ()> {
        if !access_type.can_access(self.map_perm) {
            log::warn!(
                "[VmArea::handle_page_fault] permission not allowed, perm:{:?}, access type: {:?} vaddr: {:#x}",
                self.map_perm,
                access_type,
                vpn.start_addr().0
            );
            return Err(());
        }
        match page_table.find_pte(vpn).map(|(pte, i)| (pte, PageLevel::from(i)) ) {
            Some((pte, _)) if pte.is_valid() => {
                // Cow
                if !access_type.contains(PageFaultAccessType::WRITE)
                    || !pte.map_perm().contains(MapPerm::C) {
                    return Err(());
                }
                let frame = self.frames.get_mut(&vpn).ok_or(())?;
                if frame.get_owners() == 1 {
                    let mut new_perm = pte.map_perm();
                    new_perm.remove(MapPerm::C);
                    new_perm.insert(MapPerm::W);
                    pte.set_flags(PTEFlags::from(new_perm) | PTEFlags::V);
                    unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0) };
                    Ok(())
                } else {
                    let new_frame = StrongArc::new_in(
                        FrameAllocator.alloc_tracker(1).ok_or(())?,
                        SlabAllocator
                    );
                    let new_range_ppn = new_frame.range_ppn.clone();

                    let old_data = frame.range_ppn.get_slice::<u8>();
                    new_range_ppn.get_slice_mut::<u8>().copy_from_slice(old_data);

                    *frame = new_frame;
                    
                    let mut new_perm = self.map_perm;
                    new_perm.remove(MapPerm::C);
                    new_perm.insert(MapPerm::W);
                    *pte = PageTableEntry::new(new_range_ppn.start, new_perm, true);
                    unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0) };
                    Ok(())
                }
            }
            _ => {
                match self.vma_type {
                    UserVmAreaType::TrapContext => {
                        return Err(())
                    },
                    UserVmAreaType::Data => {
                        if let Some(file) = self.file.clone() {
                            let inode = file.inode().unwrap().clone();
                            let area_offset = (vpn.0 - self.range_va.start.floor().0) * Constant::PAGE_SIZE;
                            let offset = self.offset + area_offset;
                            assert_eq!(offset % Constant::PAGE_SIZE, 0);
                            if area_offset < self.len {
                                if self.len - area_offset < Constant::PAGE_SIZE {
                                    let page_offset = self.len - area_offset;
                                    let new_frame = FrameAllocator.alloc_tracker(1).ok_or(())?;
                                    let data = new_frame.range_ppn.get_slice_mut::<u8>();
                                    let page = inode.read_page_at(offset).ok_or(())?;
                                    data[page_offset..].fill(0);
                                    data[..page_offset].copy_from_slice(&page.get_slice()[..page_offset]);
                                    page_table.map(vpn, new_frame.range_ppn.start, self.map_perm, PageLevel::Small);
                                    self.frames.insert(vpn, StrongArc::new_in(new_frame, SlabAllocator));
                                    unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
                                    Ok(())
                                } else {
                                    if access_type.contains(PageFaultAccessType::WRITE) {
                                        let new_frame = FrameAllocator.alloc_tracker(1).ok_or(())?;
                                        let page = inode.read_page_at(offset).ok_or(())?;
                                        let data = new_frame.range_ppn.get_slice_mut::<u8>();
                                        data.copy_from_slice(page.get_slice());
                                        page_table.map(vpn, new_frame.range_ppn.start, self.map_perm, PageLevel::Small);
                                        self.frames.insert(vpn, StrongArc::new_in(new_frame, SlabAllocator));
                                        unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
                                        Ok(())
                                    } else {
                                        let page = inode.read_page_at(offset).ok_or(())?;
                                        let mut new_perm = self.map_perm;
                                        if self.map_perm.contains(MapPerm::W) {
                                            new_perm.insert(MapPerm::C);
                                            new_perm.remove(MapPerm::W);
                                        }
                                        page_table.map(vpn, page.ppn(), new_perm, PageLevel::Small);
                                        self.frames.insert(vpn, page.frame());
                                        unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
                                        Ok(())
                                    }
                                }
                            } else {
                                let new_frame = FrameAllocator.alloc_tracker(1).ok_or(())?;
                                new_frame.range_ppn.get_slice_mut::<u8>().fill(0);
                                page_table.map(vpn, new_frame.range_ppn.start, self.map_perm, PageLevel::Small);
                                self.frames.insert(vpn, StrongArc::new_in(new_frame, SlabAllocator));
                                unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
                                Ok(())
                            }
                        } else {
                            let new_frame = FrameAllocator.alloc_tracker(1).ok_or(())?;
                            new_frame.range_ppn.get_slice_mut::<u8>().fill(0);
                            page_table.map(vpn, new_frame.range_ppn.start, self.map_perm, PageLevel::Small);
                            self.frames.insert(vpn, StrongArc::new_in(new_frame, SlabAllocator));
                            unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
                            Ok(())
                        }
                    },
                    UserVmAreaType::Stack
                    | UserVmAreaType::Heap => {
                        let new_frame = FrameAllocator.alloc_tracker(1).ok_or(())?;
                        new_frame.range_ppn.get_slice_mut::<u8>().fill(0);
                        page_table.map(vpn, new_frame.range_ppn.start, self.map_perm, PageLevel::Small);
                        self.frames.insert(vpn, StrongArc::new_in(new_frame, SlabAllocator));
                        unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0) };
                        return Ok(());
                    },
                    UserVmAreaType::Mmap => {
                        if !self.mmap_flags.contains(MmapFlags::MAP_ANONYMOUS) {
                            // file mapping
                            let file = self.file.as_ref().unwrap();
                            let inode = file.inode().unwrap().clone();
                            let offset = self.offset + (vpn.0 - self.range_va.start.floor().0) * PAGE_SIZE;
                            assert_eq!(offset % Constant::PAGE_SIZE, 0);
                        
                            if self.mmap_flags.contains(MmapFlags::MAP_SHARED) {
                                // share file mapping
                                let page = inode.read_page_at(offset).unwrap();
                                // map a single page
                                page_table.map(vpn, page.ppn(), self.map_perm, PageLevel::Small);
                                self.frames.insert(vpn, StrongArc::clone(&page.frame()));
                                unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
                            } else {
                                // private file mapping
                                if access_type.contains(PageFaultAccessType::WRITE) {
                                    let page = inode.read_page_at(offset).unwrap();
                                    let new_frame = FrameAllocator.alloc_tracker(1).unwrap();
                                    new_frame.range_ppn.get_slice_mut::<u8>().copy_from_slice(page.get_slice());
                                    page_table.map(vpn, new_frame.range_ppn.start, self.map_perm, PageLevel::Small);
                                    self.frames.insert(vpn, StrongArc::new_in(new_frame, SlabAllocator));
                                    unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
                                } else {
                                    let page = inode.read_page_at(offset).unwrap();
                                    let mut new_perm = self.map_perm;
                                    if self.map_perm.contains(MapPerm::W) {
                                        new_perm.insert(MapPerm::C);
                                        new_perm.remove(MapPerm::W);
                                    }
                                    page_table.map(vpn, page.ppn(), new_perm, PageLevel::Small);
                                    self.frames.insert(vpn, page.frame().clone());
                                    unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
                                }
                            }
                        } else if self.mmap_flags.contains(MmapFlags::MAP_PRIVATE) {
                            if self.mmap_flags.contains(MmapFlags::MAP_SHARED) {
                                panic!("should not reach here")
                            } else {
                                // private anonymous area
                                let new_frame = FrameAllocator.alloc_tracker(1).ok_or(())?;
                                new_frame.range_ppn.get_slice_mut::<u8>().fill(0);
                                page_table.map(vpn, new_frame.range_ppn.start, self.map_perm, PageLevel::Small);
                                self.frames.insert(vpn, StrongArc::new_in(new_frame, SlabAllocator));
                                unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
                            }
                        }
                        Ok(())
                    },
                    UserVmAreaType::Shm => {
                        panic!("do something");
                    }
                }
            }
        }
    }

}

impl Clone for UserVmArea {
    fn clone(&self) -> Self {
        let mut frames = BTreeMap::new();
        for (&vpn, frame) in self.frames.iter() {
            let new_frame = FrameAllocator.alloc_tracker(frame.range_ppn.clone().count()).unwrap();
            new_frame.range_ppn.get_slice_mut::<usize>().copy_from_slice(frame.range_ppn.get_slice());
            frames.insert(vpn, StrongArc::new_in(new_frame, SlabAllocator));
        }

        Self { 
            range_va: self.range_va.clone(), 
            vma_type: self.vma_type.clone(), 
            map_perm: self.map_perm.clone(), 
            frames,
            file: self.file.clone(),
            mmap_flags: self.mmap_flags.clone(),
            offset: self.offset,
            len: self.len
        }
    }
}