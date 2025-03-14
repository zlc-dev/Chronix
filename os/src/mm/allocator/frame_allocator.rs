//! Implementation of [`FrameAllocator`] which
//! controls all the frames in the operating system.
use crate::sync::mutex::spin_mutex::SpinMutex;
use crate::sync::mutex::Spin;
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use bitmap_allocator::{BitAlloc, BitAlloc16M, BitAlloc4K};
use hal::addr::{PhysAddr, PhysAddrHal, PhysPageNum, RangePPNHal};
use hal::allocator::FrameAllocatorHal;
use hal::constant::{Constant, ConstantsHal};
use hal::println;
use log::info;
use core::fmt::{self, Debug, Formatter};
use core::ops::Range;
use lazy_static::*;

struct BitMapFrameAllocator {
    range: Range<PhysPageNum>,
    inner: bitmap_allocator::BitAlloc16M,
}

impl BitMapFrameAllocator {
    const fn new() -> Self {
        BitMapFrameAllocator {
            range: PhysPageNum(0)..PhysPageNum(1),
            inner: bitmap_allocator::BitAlloc16M::DEFAULT
        }
    }

    fn init(&mut self, range_pa: Range<PhysAddr>) {
        self.range = range_pa.start.ceil()..range_pa.end.floor();
        info!("{:#x}, {:#x}", range_pa.end.0, range_pa.end.floor().0);
        self.inner.insert(0..(range_pa.end.floor().0 - range_pa.start.floor().0));
    }
}


/// frame allocator
static FRAME_ALLOCATOR: SpinMutex<BitMapFrameAllocator, Spin> = SpinMutex::new(BitMapFrameAllocator::new());

#[allow(missing_docs)]
#[derive(Clone)]
pub struct FrameAllocator;

pub type FrameTracker = hal::common::FrameTracker<FrameAllocator>;

impl FrameAllocatorHal for FrameAllocator {
    fn alloc(&self, cnt: usize) -> Option<Range<PhysPageNum>> {
        let mut start = FRAME_ALLOCATOR.lock().inner.alloc_contiguous(None, cnt, 0)?;
        start += FRAME_ALLOCATOR.lock().range.start.0;
        Some(PhysPageNum(start)..PhysPageNum(start + cnt))
    }

    fn dealloc(&self, range_ppn: Range<PhysPageNum>) {
        if range_ppn.end.0 - range_ppn.start.0 == 0 {
            return;
        }
        let start = range_ppn.start.0 - FRAME_ALLOCATOR.lock().range.start.0;
        FRAME_ALLOCATOR.lock().inner.dealloc_contiguous(start, range_ppn.count());
    }
}

/// initiate the frame allocator using `ekernel` and `MEMORY_END`
pub fn init_frame_allocator() {
    extern "C" {
        fn ekernel();
    }

    FRAME_ALLOCATOR.lock().init(
        PhysAddr::from(ekernel as usize - Constant::KERNEL_ADDR_SPACE.start)..PhysAddr::from(Constant::MEMORY_END),
    );
}

#[allow(unused)]
/// allocate frames
pub fn frames_alloc(size: usize) -> Option<FrameTracker> {
    FrameAllocator
        .alloc(size)
        .map(|ppn| {
            FrameTracker::new_in(ppn, FrameAllocator)
        })
}

#[allow(unused)]
/// allocate frames and clean
pub fn frames_alloc_clean(size: usize) -> Option<FrameTracker> {
    frames_alloc(size).map(|f| {
        f.range_ppn.get_slice_mut::<u8>().fill(0);
        f
    })
}

/// deallocate frames
#[allow(unused)]
pub fn frames_dealloc(range_ppn: Range<PhysPageNum>) {
    if range_ppn.clone().count() > 0 {
        FrameAllocator.dealloc(range_ppn);
    }
}

#[allow(unused)]
/// a simple test for frame allocator
pub fn frame_allocator_test() {
    let mut v: Vec<FrameTracker> = Vec::new();
    for i in 0..5 {
        let frame = frames_alloc(1).unwrap();
        // println!("{:?}", frame);
        v.push(frame);
    }
    v.clear();
    for i in 0..5 {
        let frame = frames_alloc(1).unwrap();
        // println!("{:?}", frame);
        v.push(frame);
    }
    drop(v);
    println!("frame_allocator_test passed!");
}
