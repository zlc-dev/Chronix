//! SBI call wrappers
#![allow(unused)]

use hal::println;
use sbi_rt::HartMask;

/// use sbi call to putchar in console (qemu uart handler)
pub fn console_putchar(c: usize) {
    #[allow(deprecated)]
    sbi_rt::legacy::console_putchar(c);
}

/// use sbi call to getchar from console (qemu uart handler)
pub fn console_getchar() -> usize {
    #[allow(deprecated)]
    sbi_rt::legacy::console_getchar()
}

/// use sbi call to set timer
pub fn set_timer(timer: usize) {
    sbi_rt::set_timer(timer as _);
}

/// use sbi call to shutdown the kernel
pub fn shutdown(failure: bool) -> ! {
    use sbi_rt::{system_reset, NoReason, Shutdown, SystemFailure};
    if !failure {
        system_reset(Shutdown, NoReason);
    } else {
        system_reset(Shutdown, SystemFailure);
    }
    unreachable!()
}
/// use sbi call to send IPI to one hart
pub fn send_ipi(target_id: usize){
    #[allow(deprecated)]
    let hart_mask = 1 << target_id as usize;
    let hart_mask_base = 0;
    let result = sbi_rt::send_ipi(HartMask::from_mask_base(hart_mask, hart_mask_base));
    if result.is_err() {
        println!("Failed to send IPI to hart {}", target_id);
    }
}