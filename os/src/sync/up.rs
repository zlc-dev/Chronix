//! Uniprocessor interior mutability primitives

use core::cell::{UnsafeCell, RefMut,RefCell};
use core::sync::atomic::{AtomicBool, Ordering};
use core::ops::{Deref, DerefMut};
use log::info;

/// Wrap a static data structure inside it so that we are
/// able to access it without any `unsafe`.
///
/// We should only use it in uniprocessor.
///
/// In order to get mutable reference of inner data, call
/// `exclusive_access`.
pub struct UPSafeCell<T> {
    /// inner data
    inner: UnsafeCell<T>,
}

unsafe impl<T> Sync for UPSafeCell<T> {}

impl<T> UPSafeCell<T> {
    /// User is responsible to guarantee that inner struct is only used in
    /// uniprocessor.
    pub unsafe fn new(value: T) -> Self {
        Self {
            inner: UnsafeCell::new(value),
        }
    }
    /// Panic if the data has been borrowed.
    pub unsafe fn exclusive_access(&self) -> &mut T {
        &mut *self.inner.get()
    }
    /// get the inner data
    pub unsafe fn get(&self) -> *mut T{
        self.inner.get()
    }
}
