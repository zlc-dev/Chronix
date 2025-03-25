use super::{Constant, ConstantsHal};

impl ConstantsHal for Constant {
    const KERNEL_ENTRY_PA: usize = 0x8020_0000;

    const KERNEL_ADDR_SPACE: core::ops::Range<usize> = 0xffff_ffc0_0000_0000..0xffff_ffff_ffff_ffff;

    const USER_ADDR_SPACE: core::ops::Range<usize> = 0x0000_0000_0000_0000..0x0000_003f_ffff_ffff;

    const VA_WIDTH: usize = 39;

    const PA_WIDTH: usize = 56;

    const PAGE_SIZE: usize = 4096;

    const PAGE_SIZE_BITS: usize = 12;

    const PG_LEVEL: usize = 3;
    
    const PTE_WIDTH: usize = 64;
    
    const MEMORY_END: usize = 0x8800_0000;
    
    const KERNEL_STACK_SIZE: usize = 16 * 4096;
    
    const KERNEL_STACK_TOP: usize = Self::KERNEL_ADDR_SPACE.end;
    
    const USER_STACK_SIZE: usize = 16 * 4096;
    
    const USER_STACK_TOP: usize = Self::USER_TRAP_CONTEXT_BOTTOM;

    // put the file mmap area under user stack
    const USER_FILE_END: usize = Self::USER_STACK_BOTTOM;
    const USER_FILE_SIZE: usize = 0x2_0000_0000;

    // put the share mmap area under file mmap area
    const USER_SHARE_END: usize = Self::USER_FILE_BEG;
    const USER_SHARE_SIZE: usize = 0x2_0000_0000;
    
    const USER_TRAP_CONTEXT_SIZE: usize = Self::PAGE_SIZE;
    
    const USER_TRAP_CONTEXT_TOP: usize = Self::USER_ADDR_SPACE.end;
}