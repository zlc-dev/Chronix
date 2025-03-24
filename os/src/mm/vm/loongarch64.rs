use core::ops::Range;

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use hal::{addr::{PhysAddr, PhysAddrHal, PhysPageNum, PhysPageNumHal, VirtAddr, VirtAddrHal, VirtPageNum, VirtPageNumHal}, allocator::FrameAllocatorHal, constant::{Constant, ConstantsHal}, instruction::{Instruction, InstructionHal}, pagetable::{MapPerm, PTEFlags, PageLevel, PageTableHal, VpnPageRangeIter}, util::smart_point::StrongArc};

use crate::mm::{allocator::FrameAllocator, PageTable};

use super::{KernVmArea, KernVmSpaceHal, PageFaultAccessType, UserVmArea, UserVmAreaType, UserVmSpaceHal};

/// Kernel's VmSpace
pub struct KernVmSpace;

/// User's VmSpace
pub struct UserVmSpace {
    page_table: PageTable,
    areas: Vec<UserVmArea>,
    heap: usize
}

impl KernVmSpaceHal for KernVmSpace {

    fn enable(&self) {
        // do nothing
    }

    fn new() -> Self {
        Self
    }
    
    fn push_area(&mut self, mut _area: KernVmArea, _data: Option<&[u8]>) {
        // do nothing
    }

    fn translate_vpn(&self, vpn: VirtPageNum) -> Option<PhysPageNum>{
        Some(PhysPageNum(vpn.0 & !(0x8_0000_0000_0000)))
    }
    
    fn translate_va(&self, va: VirtAddr) -> Option<PhysAddr> {
        Some(PhysAddr(va.0 & !(0x8000_0000_0000_0000)))
    }

}

#[allow(missing_docs, unused)]
impl UserVmSpace {
    fn find_heap(&mut self) -> Option<&mut UserVmArea> {
        if self.areas[self.heap].vma_type == UserVmAreaType::Heap {
            return Some(&mut self.areas[self.heap]);
        } else {
            self.areas.iter_mut().enumerate().find(|(i, vm)| {
                if vm.vma_type == UserVmAreaType::Heap {
                    self.heap = *i;
                    true
                } else {
                    false
                }
            }).map(|(_, vm)| vm)
        }
    }
}

impl UserVmSpaceHal for UserVmSpace {

    fn new() -> Self {
        Self {
            page_table: PageTable::new_in(0, FrameAllocator),
            areas: Vec::new(),
            heap: 0,
        }
    }

    fn get_page_table(&self) -> &PageTable {
        &self.page_table
    }

    fn from_kernel(_kvm_space: &KernVmSpace) -> Self {
        let ret = Self {
            page_table: PageTable::new_in(0, FrameAllocator),
            areas: Vec::new(),
            heap: 0,
        };
        ret
    }

    fn from_elf(elf_data: &[u8], kvm_space: &KernVmSpace) -> (Self, super::VmSpaceUserStackTop, super::VmSpaceEntryPoint) {
        let mut ret = Self::from_kernel(kvm_space);
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_vpn = VirtPageNum(0);
        
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                log::debug!("start_va: {:#x}, end_va: {:#x}", start_va.0, end_va.0);

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
                ret.push_area(
                    map_area,
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                );
            }
        };
        
        // map user stack with U flags
        let max_end_va: VirtAddr = max_end_vpn.start_addr();
        let user_heap_bottom: usize = max_end_va.0;
        // used in brk
        log::debug!("user_heap_bottom: {:#x}", user_heap_bottom);
        ret.heap = ret.areas.len();
        ret.push_area(
            UserVmArea::new(
                user_heap_bottom.into()..user_heap_bottom.into(),
                UserVmAreaType::Heap,
                MapPerm::R | MapPerm::W | MapPerm::U,
            ),
            None,
        );
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
        ret.push_area(
            UserVmArea::new(
                Constant::USER_TRAP_CONTEXT_BOTTOM.into()..Constant::USER_TRAP_CONTEXT_TOP.into(),
                UserVmAreaType::TrapContext,
                MapPerm::R | MapPerm::W,
            ),
            None,
        );
        (
            ret,
            user_stack_top,
            elf.header.pt2.entry_point() as usize,
        )
    }

    fn push_area(&mut self, mut area: UserVmArea, data: Option<&[u8]>) {
        area.map(&mut self.page_table);
        if let Some(data) = data{
            area.copy_data(&mut self.page_table, data);
        }
        self.areas.push(area);
    }

    fn reset_heap_break(&mut self, new_brk: VirtAddr) -> VirtAddr {
        let heap = &mut self.areas[self.heap];
        assert!(heap.vma_type == UserVmAreaType::Heap);
        let range = heap.range_va.clone();
        if new_brk >= range.end {
            heap.range_va = range.start..new_brk;
            for vpn in range.end.ceil()..new_brk.ceil() {
                let frame = FrameAllocator.alloc_tracker(1).unwrap();
                self.page_table.map(vpn, frame.range_ppn.start, heap.map_perm, PageLevel::Small);
                heap.frames.insert(vpn, StrongArc::new(frame));
            }
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
        let area = self.areas
            .iter_mut()
            .find(|a| a.range_va.contains(&va))
            .ok_or(())?;

        area.handle_page_fault(&mut self.page_table, va.floor(), access_type)
    }
    
    fn from_existed(uvm_space: &mut Self, kvm_space: &KernVmSpace) -> Self {
        let mut ret = Self::from_kernel(kvm_space);
        for area in uvm_space.areas.iter_mut() {
            match area.clone_cow(&mut uvm_space.page_table) {
                Ok(new_area) => {
                    ret.push_area(new_area, None);
                },
                Err(new_area) => {
                    ret.push_area(new_area, None);
                    for &vpn in area.frames.keys() {
                        let src_ppn = uvm_space.page_table.translate_vpn(vpn).unwrap();
                        let dst_ppn = ret.page_table.translate_vpn(vpn).unwrap();
                        dst_ppn
                            .start_addr()
                            .get_mut::<[u8; Constant::PAGE_SIZE]>()
                            .copy_from_slice(src_ppn.start_addr().get_mut::<[u8; Constant::PAGE_SIZE]>());
                    }
                }
            }
            
        }
        ret
    }
    

}

#[allow(missing_docs, unused)]
impl UserVmArea {

    fn range_vpn(&self) -> Range<VirtPageNum> {
        self.range_va.start.floor()..self.range_va.end.ceil()
    }

    fn copy_data(&mut self, page_table: &PageTable, data: &[u8]) {
        let mut start: usize = 0;
        let len = data.len();
        for vpn in self.range_vpn() {
            let src = &data[start..len.min(start + Constant::PAGE_SIZE)];
            let dst = &mut page_table
                .translate_vpn(vpn)
                .unwrap()
                .start_addr()
                .get_mut::<[u8; Constant::PAGE_SIZE]>()[..src.len()];
            dst.copy_from_slice(src);
            start += Constant::PAGE_SIZE;
            if start >= len {
                break;
            }
        }
    }

    fn split_off(&mut self, p: VirtPageNum) -> Self {
        debug_assert!(self.range_va.contains(&p.start_addr()));
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

    fn map(&mut self, page_table: &mut PageTable) {
        let range_vpn = self.range_va.start.floor()..self.range_va.end.ceil();
        
        for vpn in range_vpn {
            let frame = FrameAllocator.alloc_tracker(1).unwrap();
            page_table.map(vpn, frame.range_ppn.start, self.map_perm, PageLevel::Small);
            self.frames.insert(vpn, StrongArc::new(frame));
        }
        // if self.map_perm.contains(MapPerm::C) {
        //     for (&vpn, frame) in self.frames.iter() {
        //         self.map_range_to(page_table, vpn..vpn+1, frame.range_ppn.start);
        //     }
        // } else {
        //     match self.vma_type {
        //         UserVmAreaType::Data |
        //         UserVmAreaType::TrapContext => {
        //             let range_vpn = self.range_va.start.floor()..self.range_va.end.ceil();
        //             for vpn in range_vpn {
        //                 let frame = self.alloc.alloc_tracker(1).unwrap();
        //                 page_table.map(vpn, frame.range_ppn.start, self.map_perm, PageLevel::Small);
        //                 self.frames.insert(vpn, StrongArc::new(frame));
        //             }
        //         },
        //         UserVmAreaType::Heap |
        //         UserVmAreaType::Stack => {
        //         },
        //     }
        // }
    }

    fn unmap(&mut self, page_table: &mut PageTable) {
        let range_vpn = self.range_va.start.floor()..self.range_va.end.ceil();
        match self.vma_type {
            UserVmAreaType::Data |
            UserVmAreaType::TrapContext => {
                for vpn in range_vpn {
                    page_table.unmap(vpn);
                }
                self.frames.clear();
            },
            UserVmAreaType::Heap |
            UserVmAreaType::Stack => {
                for &vpn in self.frames.keys() {
                    page_table.unmap(vpn);
                }
                self.frames.clear();
            },
        }
    }

    fn clone_cow(&mut self, page_table: &mut PageTable) -> Result<Self, Self> {
        // note: trap context cannot supprt COW
        if true {
            return Err(self.clone());
        }
        if self.map_perm.contains(MapPerm::W) {
            self.map_perm.insert(MapPerm::C);
            self.map_perm.remove(MapPerm::W);
            for &vpn in self.frames.keys() {
                let (pte, _) = page_table.find_pte(vpn).unwrap();
                pte.set_flags(PTEFlags::from(self.map_perm) | PTEFlags::DEFAULT);
                unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0); }
            }
        } else {
            self.map_perm.insert(MapPerm::C);
        }
        Ok(Self {
            range_va: self.range_va.clone(), 
            frames: self.frames.clone(), 
            map_perm: self.map_perm.clone(), 
            vma_type: self.vma_type.clone(),
        })
    }

    fn handle_page_fault(&mut self, 
        _page_table: &mut PageTable, 
        _vpn: VirtPageNum,
        _access_type: PageFaultAccessType
    ) -> Result<(), ()> {
        Err(())
        // if !access_type.can_access(self.map_perm) {
        //     log::warn!(
        //         "[VmArea::handle_page_fault] permission not allowed, perm:{:?}",
        //         self.map_perm
        //     );
        //     return Err(());
        // }
        // match page_table.find_pte(vpn).map(|(pte, i)| (pte, PageLevel::from(i)) ) {
        //     Some((pte, level)) if pte.is_valid() => {
        //         // Cow
        //         let frame = self.frames.get(&vpn).ok_or(())?;
        //         if frame.get_owners() == 1 {
        //             self.map_perm.remove(MapPerm::C);
        //             self.map_perm.insert(MapPerm::W);
        //             pte.set_flags(PTEFlags::from(self.map_perm) | PTEFlags::DEFAULT);
        //             unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0) };
        //             Ok(())
        //         } else {
        //             let new_frame = StrongArc::new(self.alloc.alloc_tracker(level.page_count()).ok_or(())?);
        //             let new_range_ppn = new_frame.range_ppn.clone();

        //             let old_data = &frame.range_ppn.get_slice::<u8>();
        //             new_range_ppn.get_slice_mut::<u8>().copy_from_slice(old_data);
                    
        //             *self.frames.get_mut(&vpn).ok_or(())? = new_frame;

        //             self.map_perm.remove(MapPerm::C);
        //             self.map_perm.insert(MapPerm::W);
        //             *pte = PageTableEntry::new(new_range_ppn.start, self.map_perm, true);
                    
        //             unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0) };
        //             Ok(())
        //         }
        //     }
        //     _ => {
        //         match self.vma_type {
        //             UserVmAreaType::Data
        //             | UserVmAreaType::TrapContext => {
        //                 return Err(())
        //             },
        //             UserVmAreaType::Stack
        //             | UserVmAreaType::Heap => {
        //                 let new_frame = self.alloc.alloc_tracker(1).ok_or(())?;
        //                 self.map_range_to(page_table, vpn..vpn+1, new_frame.range_ppn.start);
        //                 self.frames.insert(vpn, StrongArc::new(new_frame));
        //                 unsafe { Instruction::tlb_flush_addr(vpn.start_addr().0) };
        //                 return Ok(());
        //             }
        //         }
        //     }
        // }
    }

}

impl Clone for UserVmArea {
    fn clone(&self) -> Self {
        Self { 
            range_va: self.range_va.clone(), 
            vma_type: self.vma_type.clone(), 
            map_perm: self.map_perm.clone(), 
            frames: BTreeMap::new(),
        }
    }
}
