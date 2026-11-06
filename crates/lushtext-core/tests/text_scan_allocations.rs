// SPDX-License-Identifier: GPL-3.0-or-later

//! The save-time trim allocates nothing on text it leaves unchanged
//! (`simd-line-scans`, design D7: allocation-free behaviour is shown by a test,
//! not a benchmark), and the file-health count, AVX2 kernel or `memchr3`
//! fallback, allocates nothing at all (`avx2-health-scan-kernel`).
//!
//! A separate test binary, because it installs a counting global allocator;
//! the count is per thread, so the test harness's own threads cannot disturb
//! it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::borrow::Cow;
use std::cell::Cell;

use lushtext_core::services::editor_io::text_scans;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

// SAFETY: every call forwards to `System` unchanged; the thread-local counter
// is a const-initialized `Cell`, so counting itself never allocates.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|count| count.set(count.get() + 1));
        // SAFETY: forwarded with the caller's layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: `pointer` came from `System` with this layout.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.with(|count| count.set(count.get() + 1));
        // SAFETY: forwarded with the caller's pointer, layout, and size.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations_during<T>(work: impl FnOnce() -> T) -> (T, usize) {
    let before = ALLOCATIONS.with(Cell::get);
    let value = work();
    (value, ALLOCATIONS.with(Cell::get) - before)
}

#[test]
fn trim_allocates_nothing_on_a_clean_mebibyte() {
    let line = "    let value = compute(\u{00E9}l\u{00E8}ve, \"\u{65E5}\u{672C}\");\r\n";
    let text = line.repeat((1024 * 1024) / line.len() + 1);

    let (trimmed, allocations) =
        allocations_during(|| text_scans::trim_trailing_space_and_tabs(&text));

    assert_eq!(allocations, 0);
    assert!(matches!(trimmed, Cow::Borrowed(_)));
    assert!(std::ptr::eq(trimmed.as_ref(), text.as_str()));
}

#[test]
fn trim_allocates_once_for_a_dirty_mebibyte() {
    let line = "    let value = compute();  \n";
    let text = line.repeat((1024 * 1024) / line.len() + 1);

    let (trimmed, allocations) =
        allocations_during(|| text_scans::trim_trailing_space_and_tabs(&text));

    assert_eq!(allocations, 1, "one String sized to the input");
    assert_eq!(trimmed.len(), text.len() - 2 * (text.len() / line.len()));
}

#[test]
fn health_count_allocates_nothing_on_a_typographic_mebibyte() {
    // Curly quotes, dashes and ellipses (`E2 80 xx`, never a hit) every few
    // bytes, beside one hit of each kind per line: the kernel's confirmation
    // path on every block, and the `memchr3` tail.
    let line = "\u{201C}It\u{2019}s\u{2026}\u{201D} \u{2014} a \u{2013} b\u{00A0}c\u{200B}d\u{2060}e\u{FEFF}\n";
    let text = line.repeat((1024 * 1024) / line.len() + 1);

    let (counts, allocations) = allocations_during(|| text_scans::count_health_characters(&text));

    assert_eq!(allocations, 0);
    let lines = text.len() / line.len();
    assert_eq!(counts, (lines, 3 * lines));
}
