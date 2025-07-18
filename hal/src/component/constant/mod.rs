use core::ops::Range;

pub trait ConstantsHal {
    const MAX_PROCESSORS: usize = 4;
    const KERNEL_ENTRY_PA: usize;

    const KERNEL_ADDR_SPACE: Range<usize>;
    const USER_ADDR_SPACE: Range<usize>;

    const PA_WIDTH: usize;
    const VA_WIDTH: usize;

    const PAGE_SIZE: usize;
    const PAGE_SIZE_BITS: usize;

    const PTE_WIDTH: usize;
    const PTES_PER_PAGE: usize = Self::PAGE_SIZE / (Self::PTE_WIDTH >> 3);
    const PPN_WIDTH: usize = Self::PA_WIDTH - Self::PAGE_SIZE_BITS;
    const VPN_WIDTH: usize = Self::VA_WIDTH - Self::PAGE_SIZE_BITS;

    const PG_LEVEL: usize;

    const MEMORY_END: usize;

    const SIGRET_TRAMPOLINE_SIZE: usize; 
    const SIGRET_TRAMPOLINE_BOTTOM: usize = Self::SIGRET_TRAMPOLINE_TOP - Self::SIGRET_TRAMPOLINE_SIZE; 
    const SIGRET_TRAMPOLINE_TOP: usize; 

    const KERNEL_STACK_SIZE: usize;
    const KERNEL_STACK_BOTTOM: usize = Self::KERNEL_STACK_TOP - Self::KERNEL_STACK_SIZE * Self::MAX_PROCESSORS;
    const KERNEL_STACK_TOP: usize;

    const KERNEL_VM_SIZE: usize;
    const KERNEL_VM_BOTTOM: usize = Self::KERNEL_VM_TOP - Self::KERNEL_VM_SIZE;
    const KERNEL_VM_TOP: usize;

    const USER_STACK_SIZE: usize;
    const USER_STACK_BOTTOM: usize = Self::USER_STACK_TOP - Self::USER_STACK_SIZE;
    const USER_STACK_TOP: usize;

    const USER_FILE_BEG: usize = Self::USER_FILE_END - Self::USER_FILE_SIZE;
    const USER_FILE_SIZE: usize;
    const USER_FILE_END: usize;
    const USER_FILE_PER_PAGES: usize = 8; // how many pages can a mmap file own

    const USER_SHARE_BEG: usize = Self::USER_SHARE_END - Self::USER_SHARE_SIZE;
    const USER_SHARE_SIZE: usize;
    const USER_SHARE_END: usize;

    const DL_INTERP_OFFSET: usize;

}

pub struct Constant;

#[cfg(target_arch = "riscv64")]
mod riscv64;

#[cfg(target_arch = "riscv64")]
#[allow(unused)]
pub use riscv64::*;

#[cfg(target_arch = "loongarch64")]
mod loongarch64;

#[cfg(target_arch = "loongarch64")]
#[allow(unused)]
pub use loongarch64::*;