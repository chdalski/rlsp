// SPDX-License-Identifier: MIT
//! Heap use of the position converter, measured with a counting allocator (the
//! way `schema_memory.rs` measures: counters per thread, so the harness does
//! not disturb a measurement).
//!
//! A text without an astral character builds a converter that allocates
//! nothing, and converts positions without allocating. The memory of a
//! converter grows with the astral characters of its text, not with its lines.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use rlsp_yaml::position_encoding::PositionConverter;
use tower_lsp::lsp_types::Position;

struct CountingAllocator;

thread_local! {
    static CALLS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

fn count(bytes: usize) {
    CALLS.set(CALLS.get() + 1);
    BYTES.set(BYTES.get() + bytes);
}

// SAFETY: every method forwards to `System`, which upholds the contract of
// `GlobalAlloc`; the counters are plain thread-local cells.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Run `body` and return its result with the allocation calls and the bytes
/// they asked for on this thread.
fn measure<T>(body: impl FnOnce() -> T) -> (T, usize, usize) {
    let (calls, bytes) = (CALLS.get(), BYTES.get());
    let result = body();
    (result, CALLS.get() - calls, BYTES.get() - bytes)
}

const ASTRAL: &str = "\u{1f600}";

#[test]
fn building_for_a_text_without_an_astral_character_allocates_nothing() {
    let text = "key: value \u{e9}\u{540d}\r\nother: 1\r- item\n".repeat(30_000);

    let (_, calls, _) = measure(|| PositionConverter::new(&text));

    assert_eq!(calls, 0);
}

#[test]
fn converting_for_a_text_without_an_astral_character_allocates_nothing_and_returns_the_input() {
    let converter = PositionConverter::new(&"key: value\n".repeat(1000));
    let positions: Vec<Position> = (0..10_000)
        .map(|i| Position::new(i % 1100, (i * 7) % 40))
        .collect();

    let (answers, calls, _) = measure(|| {
        positions
            .iter()
            .map(|&p| (converter.to_client(p), converter.to_internal(p)))
            .collect::<Vec<_>>()
    });

    // The one allocation is the result vector.
    assert_eq!(calls, 1);
    assert!(
        positions
            .iter()
            .zip(&answers)
            .all(|(&p, &(client, internal))| client == p && internal == p)
    );
}

#[test]
fn the_memory_of_a_converter_does_not_grow_with_the_lines_of_its_text() {
    let mut text = "\n".repeat(1_000_000);
    text.push_str(ASTRAL);

    let (converter, calls, bytes) = measure(|| PositionConverter::new(&text));

    assert!(calls <= 2, "{calls} allocations");
    assert!(bytes < 1024, "{bytes} bytes");
    assert_eq!(
        converter.to_client(Position::new(1_000_000, 1)),
        Position::new(1_000_000, 2)
    );
}

#[test]
fn the_memory_of_a_converter_is_bounded_by_the_astral_characters_of_its_text() {
    let text = ASTRAL.repeat(100_000);

    let (_, _, bytes) = measure(|| PositionConverter::new(&text));

    // A vector grows by doubling and holds three counters per character; the
    // text takes four bytes per character.
    let text_bytes = text.len();
    assert!(
        bytes < text_bytes * 8,
        "{bytes} bytes for {text_bytes} bytes of text"
    );
}
