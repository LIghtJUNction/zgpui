use serde::Serialize;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[derive(Clone, Copy, Default, Serialize)]
pub struct Stats {
    allocations: u64,
    allocated_bytes: u64,
    deallocations: u64,
    deallocated_bytes: u64,
    reallocations: u64,
    net_requested_bytes: i64,
    peak_additional_requested_bytes: i64,
}

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static COUNTS: Cell<Stats> = Cell::new(Stats::default());
}

fn record(
    allocated: usize,
    freed: usize,
    allocation: bool,
    deallocation: bool,
    reallocation: bool,
) {
    if ACTIVE.try_with(Cell::get).unwrap_or(false) {
        COUNTS.with(|counts| {
            let mut value = counts.get();
            value.allocations += u64::from(allocation);
            value.deallocations += u64::from(deallocation);
            value.reallocations += u64::from(reallocation);
            value.allocated_bytes += allocated as u64;
            value.deallocated_bytes += freed as u64;
            value.net_requested_bytes += allocated as i64 - freed as i64;
            value.peak_additional_requested_bytes = value
                .peak_additional_requested_bytes
                .max(value.net_requested_bytes);
            counts.set(value);
        });
    }
}

pub struct Counter;

// Every allocation is delegated unchanged to System; the thread-local counters own no heap memory.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record(layout.size(), 0, true, false, false);
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record(layout.size(), 0, true, false, false);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record(0, layout.size(), false, true, false);
        unsafe {
            System.dealloc(pointer, layout);
        }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, layout, new_size) };
        if !result.is_null() {
            record(new_size, layout.size(), false, false, true);
        }
        result
    }
}

pub fn start() {
    COUNTS.with(|counts| counts.set(Stats::default()));
    ACTIVE.with(|active| active.set(true));
}
pub fn finish() -> Stats {
    ACTIVE.with(|active| active.set(false));
    COUNTS.with(Cell::get)
}
