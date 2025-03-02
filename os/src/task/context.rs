//! Implementation of [`TaskContext`]
use alloc::sync::Arc;
use riscv::register::sstatus;

pub struct EnvContext {
    /// Permit supervisor user memory access
    sum_flag: usize,
}

impl EnvContext{
    pub const fn new() -> Self {
        Self {
            sum_flag: 0,
        }
    }

    pub unsafe fn auto_sum(&self) {
        log::trace!("[EnvContext::auto_sum] sum_cnt: {}", self.sum_flag);
        if self.sum_flag == 0 {
            riscv::register::sstatus::clear_sum();
        } else {
            riscv::register::sstatus::set_sum();
        }
    }

    pub fn change_env(&self, new:&Self){
        unsafe{new.auto_sum();}
    }
    pub fn sum_inc(&mut self) {
        if self.sum_flag == 0 {
            unsafe {
                sstatus::set_sum();
            }
            self.sum_flag = 1;
        }
    }

    pub fn sum_dec(&mut self) {
        if self.sum_flag == 1 {
            unsafe {
                sstatus::clear_sum();
            }
            self.sum_flag = 0;
        }
    }
}
