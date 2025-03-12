
use core::ops::Range;

use crate::allocator::FrameAllocatorHal;
use crate::addr::{PhysPageNum, VirtPageNum};

use bitflags::bitflags;

bitflags! {
    pub struct MapPerm: u8 {
        /// Readable
        const R = 1 << 0;
        /// Writable
        const W = 1 << 1;
        /// Executable
        const X = 1 << 2;
        /// User-mode accessible
        const U = 1 << 3;
        /// Copy On Write
        const C = 1 << 4;
    }
}

pub trait PageTableEntryHal {
    fn new(ppn: PhysPageNum, map_perm: MapPerm, valid: bool) -> Self;
    fn map_perm(&self) -> MapPerm;
    fn set_valid(&mut self);
    fn is_valid(&self) -> bool;
}

pub trait PageTableHal<PTE: PageTableEntryHal, A: FrameAllocatorHal> {
    fn from_token(token: usize, alloc: A) -> Self;
    fn get_token(&self) -> usize;
    fn translate_va(&self, va: VirtAddr) -> Option<PhysAddr>;
    fn translate_vpn(&self, vpn: VirtPageNum) -> Option<PhysPageNum>;
    fn new_in(ppn: PhysPageNum, asid: usize, alloc: A) -> Self;
    fn find_pte(&self, vpn: VirtPageNum) -> Option<(&mut PTE, usize)>;
    fn map(&mut self, range_vpn: Range<VirtPageNum>, start_ppn: PhysPageNum, perm: MapPerm);
    fn unmap(&mut self, range_vpn: Range<VirtPageNum>);
    unsafe fn enable(&self);
}

#[cfg(target_arch = "riscv64")]
mod riscv64;

#[cfg(target_arch = "riscv64")]
pub use riscv64::*;

#[cfg(target_arch = "loongarch64")]
mod loongarch64;

#[cfg(target_arch = "loongarch64")]
pub use loongarch64::*;

use super::addr::{PhysAddr, VirtAddr};
