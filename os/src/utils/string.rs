//! useful utils for string handling

use crate::processor::context::SumGuard;
use alloc::string::String;

/// Convert C-style string(end with '\0') to rust string
pub fn c_str_to_string(ptr: *const u8) -> String {
    // dangerous: we dont do check but only open permission for kernel
    let _sum_guard = SumGuard::new();
    let mut ptr = ptr as usize;
    let mut ret = String::new();
    loop {
        let ch = unsafe { (ptr as *const u8).read() };
        //let ch: u8 = unsafe { *(ptr as *const u8) };
        if ch == 0 {
            break;
        }
        ret.push(ch as char);
        ptr += 1;
    }
    ret
}