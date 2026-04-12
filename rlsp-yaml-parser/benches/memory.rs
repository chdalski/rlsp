// SPDX-License-Identifier: MIT

//! Memory benchmarks: peak allocation during parse.
//!
//! Uses a counting allocator wrapper to track total bytes allocated during a
//! parse. Criterion's measurement is wall-clock time; the allocation count is
//! reported as a custom metric via `iter_custom`.

#![expect(
    unsafe_code,
    reason = "custom allocator wrapper requires unsafe GlobalAlloc impl"
)]

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[path = "fixtures.rs"]
#[expect(
    dead_code,
    reason = "bench fixture; each binary uses a subset of functions"
)]
mod fixtures;

// ---------------------------------------------------------------------------
// Counting allocator
// ---------------------------------------------------------------------------

/// Wraps the system allocator and counts total bytes allocated across all calls.
struct CountingAllocator;

static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size > layout.size() {
            ALLOC_BYTES.fetch_add((new_size - layout.size()) as u64, Ordering::Relaxed);
        }
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Reset the allocation counters and return `(bytes_before, count_before)`.
fn reset_counters() -> (u64, u64) {
    let b = ALLOC_BYTES.swap(0, Ordering::Relaxed);
    let c = ALLOC_COUNT.swap(0, Ordering::Relaxed);
    (b, c)
}

/// Read current counters without resetting.
fn read_counters() -> (u64, u64) {
    (
        ALLOC_BYTES.load(Ordering::Relaxed),
        ALLOC_COUNT.load(Ordering::Relaxed),
    )
}

// ---------------------------------------------------------------------------
// Benchmark groups
// ---------------------------------------------------------------------------

fn bench_memory_load(c: &mut Criterion) {
    let cases: &[(&str, String)] = &[
        ("tiny_100B", fixtures::tiny()),
        ("medium_10KB", fixtures::medium()),
        ("large_100KB", fixtures::large()),
    ];

    let mut group = c.benchmark_group("memory/rlsp_load");
    for (name, yaml) in cases {
        group.bench_with_input(BenchmarkId::new("load", name), yaml, |b, yaml| {
            b.iter_custom(|iters| {
                let start = Instant::now();
                for _ in 0..iters {
                    reset_counters();
                    let _ = black_box(rlsp_yaml_parser::load(black_box(yaml)));
                    let (bytes, _count) = read_counters();
                    black_box(bytes);
                }
                start.elapsed()
            });
        });
    }
    group.finish();
}

fn bench_memory_parse_events(c: &mut Criterion) {
    let cases: &[(&str, String)] = &[
        ("tiny_100B", fixtures::tiny()),
        ("medium_10KB", fixtures::medium()),
        ("large_100KB", fixtures::large()),
    ];

    let mut group = c.benchmark_group("memory/rlsp_parse_events");
    for (name, yaml) in cases {
        group.bench_with_input(BenchmarkId::new("parse_events", name), yaml, |b, yaml| {
            b.iter_custom(|iters| {
                let start = Instant::now();
                for _ in 0..iters {
                    reset_counters();
                    let count = rlsp_yaml_parser::parse_events(black_box(yaml)).count();
                    black_box(count);
                    let (bytes, _count) = read_counters();
                    black_box(bytes);
                }
                start.elapsed()
            });
        });
    }
    group.finish();
}

/// Benchmark allocation profile for a single large parse.
fn bench_alloc_stats(c: &mut Criterion) {
    let yaml = fixtures::large();

    let mut group = c.benchmark_group("memory/alloc_stats");
    group.bench_function("large_load", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                reset_counters();
                let _ = black_box(rlsp_yaml_parser::load(black_box(&yaml)));
                let (bytes, allocations) = read_counters();
                // Report via black_box so the optimizer cannot elide the measurement.
                black_box((bytes, allocations));
            }
            start.elapsed()
        });
    });
    group.finish();
}

fn bench_memory_real_world(c: &mut Criterion) {
    let yaml = fixtures::kubernetes_deployment();

    let mut group = c.benchmark_group("memory/real_world");
    group.bench_function("load", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                reset_counters();
                let _ = black_box(rlsp_yaml_parser::load(black_box(&yaml)));
                let (bytes, _count) = read_counters();
                black_box(bytes);
            }
            start.elapsed()
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_memory_load,
    bench_memory_parse_events,
    bench_alloc_stats,
    bench_memory_real_world
);
criterion_main!(benches);
