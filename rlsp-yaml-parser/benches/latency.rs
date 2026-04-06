// SPDX-License-Identifier: MIT

//! Latency benchmarks: time-to-first-event using the streaming `parse_events` API.
//!
//! Time-to-first-event is important for the LSP use case where the server needs
//! to start producing diagnostics before a large document is fully parsed.

#![allow(unsafe_code)]
// Criterion's BenchmarkGroup has a significant Drop but the idiomatic usage
// is to keep the group alive for the whole bench function and call finish().
#![allow(clippy::significant_drop_tightening)]

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::os::raw::{c_char, c_int, c_uint};

#[path = "fixtures.rs"]
mod fixtures;

// ---------------------------------------------------------------------------
// libfyaml FFI
// ---------------------------------------------------------------------------

#[repr(C)]
struct FyParser {
    _opaque: [u8; 0],
}

#[repr(C)]
struct FyEvent {
    _opaque: [u8; 0],
}

#[repr(C)]
struct FyParseCfg {
    search_path: *const c_char,
    flags: c_uint,
    userdata: *mut std::ffi::c_void,
    diag: *mut std::ffi::c_void,
}

const FYPCF_QUIET: c_uint = 1 << 0;

#[link(name = "fyaml")]
unsafe extern "C" {
    fn fy_parser_create(cfg: *const FyParseCfg) -> *mut FyParser;
    fn fy_parser_destroy(fyp: *mut FyParser);
    fn fy_parser_set_string(fyp: *mut FyParser, str: *const c_char, len: usize) -> c_int;
    fn fy_parser_parse(fyp: *mut FyParser) -> *mut FyEvent;
    fn fy_parser_event_free(fyp: *mut FyParser, fye: *mut FyEvent);
}

/// Drain all events from libfyaml for the given YAML string, returning event count.
///
/// # Safety
/// Calls into libfyaml via FFI. The input string must remain valid for the
/// parser's lifetime, which is scoped to this function.
unsafe fn libfyaml_parse_all(yaml: &str) -> usize {
    let cfg = FyParseCfg {
        search_path: std::ptr::null(),
        flags: FYPCF_QUIET,
        userdata: std::ptr::null_mut(),
        diag: std::ptr::null_mut(),
    };
    let parser = unsafe { fy_parser_create(&raw const cfg) };
    assert!(!parser.is_null(), "fy_parser_create failed");

    let ret = unsafe { fy_parser_set_string(parser, yaml.as_ptr().cast(), yaml.len()) };
    assert_eq!(ret, 0, "fy_parser_set_string failed");

    let mut count = 0usize;
    loop {
        let event = unsafe { fy_parser_parse(parser) };
        if event.is_null() {
            break;
        }
        count += 1;
        unsafe { fy_parser_event_free(parser, event) };
    }

    unsafe { fy_parser_destroy(parser) };
    count
}

/// Return the first event from libfyaml (then tear down the parser).
///
/// # Safety
/// Calls into libfyaml via FFI. The yaml string must remain valid for the
/// duration of this call.
unsafe fn libfyaml_first_event(yaml: &str) -> bool {
    let cfg = FyParseCfg {
        search_path: std::ptr::null(),
        flags: FYPCF_QUIET,
        userdata: std::ptr::null_mut(),
        diag: std::ptr::null_mut(),
    };
    let parser = unsafe { fy_parser_create(&raw const cfg) };
    assert!(!parser.is_null(), "fy_parser_create failed");

    let ret = unsafe { fy_parser_set_string(parser, yaml.as_ptr().cast(), yaml.len()) };
    assert_eq!(ret, 0, "fy_parser_set_string failed");

    let event = unsafe { fy_parser_parse(parser) };
    let got = !event.is_null();
    if got {
        unsafe { fy_parser_event_free(parser, event) };
    }
    unsafe { fy_parser_destroy(parser) };
    got
}

// ---------------------------------------------------------------------------
// Benchmark groups
// ---------------------------------------------------------------------------

fn bench_time_to_first_event(c: &mut Criterion) {
    let cases: &[(&str, String)] = &[
        ("tiny_100B", fixtures::tiny()),
        ("medium_10KB", fixtures::medium()),
        ("large_100KB", fixtures::large()),
        ("huge_1MB", fixtures::huge()),
    ];

    let mut group = c.benchmark_group("latency/rlsp");
    for (name, yaml) in cases {
        group.bench_with_input(BenchmarkId::new("first_event", name), yaml, |b, yaml| {
            b.iter(|| {
                let mut events = rlsp_yaml_parser::parse_events(black_box(yaml));
                black_box(events.next())
            });
        });
    }
    group.finish();

    let mut group = c.benchmark_group("latency/libfyaml");
    for (name, yaml) in cases {
        group.bench_with_input(BenchmarkId::new("first_event", name), yaml, |b, yaml| {
            b.iter(|| {
                let got = unsafe { libfyaml_first_event(black_box(yaml)) };
                black_box(got)
            });
        });
    }
    group.finish();
}

/// Benchmark full parse via `parse_events` iterator (drain all events).
fn bench_parse_events_full(c: &mut Criterion) {
    let cases: &[(&str, String)] = &[
        ("tiny_100B", fixtures::tiny()),
        ("medium_10KB", fixtures::medium()),
        ("large_100KB", fixtures::large()),
    ];

    let mut group = c.benchmark_group("latency/rlsp_full");
    for (name, yaml) in cases {
        group.bench_with_input(BenchmarkId::new("parse_events", name), yaml, |b, yaml| {
            b.iter(|| {
                let count = rlsp_yaml_parser::parse_events(black_box(yaml)).count();
                black_box(count)
            });
        });
    }
    group.finish();
}

/// Benchmark full event drain via libfyaml by size.
fn bench_parse_events_full_libfyaml(c: &mut Criterion) {
    let cases: &[(&str, String)] = &[
        ("tiny_100B", fixtures::tiny()),
        ("medium_10KB", fixtures::medium()),
        ("large_100KB", fixtures::large()),
    ];

    let mut group = c.benchmark_group("latency/libfyaml_full");
    for (name, yaml) in cases {
        group.bench_with_input(BenchmarkId::new("parse_events", name), yaml, |b, yaml| {
            b.iter(|| {
                let count = unsafe { libfyaml_parse_all(black_box(yaml)) };
                black_box(count)
            });
        });
    }
    group.finish();
}

fn bench_real_world_latency(c: &mut Criterion) {
    let yaml = fixtures::kubernetes_deployment();

    let mut group = c.benchmark_group("latency_real/rlsp");
    group.bench_function("first_event", |b| {
        b.iter(|| {
            let mut events = rlsp_yaml_parser::parse_events(black_box(&yaml));
            black_box(events.next())
        });
    });
    group.finish();

    let mut group = c.benchmark_group("latency_real/libfyaml");
    group.bench_function("first_event", |b| {
        b.iter(|| {
            let got = unsafe { libfyaml_first_event(black_box(&yaml)) };
            black_box(got)
        });
    });
    group.finish();

    let mut group = c.benchmark_group("latency_real/rlsp_full");
    group.bench_function("parse_events", |b| {
        b.iter(|| {
            let count = rlsp_yaml_parser::parse_events(black_box(&yaml)).count();
            black_box(count)
        });
    });
    group.finish();

    let mut group = c.benchmark_group("latency_real/libfyaml_full");
    group.bench_function("parse_events", |b| {
        b.iter(|| {
            let count = unsafe { libfyaml_parse_all(black_box(&yaml)) };
            black_box(count)
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_time_to_first_event,
    bench_parse_events_full,
    bench_parse_events_full_libfyaml,
    bench_real_world_latency
);
criterion_main!(benches);
