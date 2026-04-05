// SPDX-License-Identifier: MIT

//! Throughput benchmarks: MB/s across document sizes and styles.
//!
//! Compares `rlsp-yaml-parser::load()` against libfyaml's event API.
//! Criterion groups pair the two parsers for direct comparison.

#![allow(unsafe_code)]
// Criterion's BenchmarkGroup has a significant Drop but the idiomatic usage
// is to keep the group alive for the whole bench function and call finish().
#![allow(clippy::significant_drop_tightening)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::os::raw::{c_char, c_int, c_uint};

#[path = "fixtures.rs"]
mod fixtures;

// ---------------------------------------------------------------------------
// libfyaml FFI — minimal event API
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

// ---------------------------------------------------------------------------
// Benchmark groups
// ---------------------------------------------------------------------------

fn bench_throughput_by_size(c: &mut Criterion) {
    let cases: &[(&str, String)] = &[
        ("tiny_100B", fixtures::tiny()),
        ("medium_10KB", fixtures::medium()),
        ("large_100KB", fixtures::large()),
        ("huge_1MB", fixtures::huge()),
    ];

    let mut group = c.benchmark_group("throughput/rlsp");
    for (name, yaml) in cases {
        group.throughput(Throughput::Bytes(yaml.len() as u64));
        group.bench_with_input(BenchmarkId::new("load", name), yaml, |b, yaml| {
            b.iter(|| {
                let result = rlsp_yaml_parser::load(black_box(yaml));
                black_box(result)
            });
        });
    }
    group.finish();

    let mut group = c.benchmark_group("throughput/libfyaml");
    for (name, yaml) in cases {
        group.throughput(Throughput::Bytes(yaml.len() as u64));
        group.bench_with_input(BenchmarkId::new("parse_events", name), yaml, |b, yaml| {
            b.iter(|| {
                let count = unsafe { libfyaml_parse_all(black_box(yaml)) };
                black_box(count)
            });
        });
    }
    group.finish();
}

fn bench_throughput_by_style(c: &mut Criterion) {
    let size = fixtures::LARGE_TARGET;
    let cases: &[(&str, String)] = &[
        ("block_heavy", fixtures::block_heavy(size)),
        ("block_sequence", fixtures::block_sequence(size)),
        ("flow_heavy", fixtures::flow_heavy(size)),
        ("scalar_heavy", fixtures::scalar_heavy(size)),
        ("mixed", fixtures::mixed(size)),
    ];

    let mut group = c.benchmark_group("throughput_style/rlsp");
    for (name, yaml) in cases {
        group.throughput(Throughput::Bytes(yaml.len() as u64));
        group.bench_with_input(BenchmarkId::new("load", name), yaml, |b, yaml| {
            b.iter(|| black_box(rlsp_yaml_parser::load(black_box(yaml))));
        });
    }
    group.finish();

    let mut group = c.benchmark_group("throughput_style/libfyaml");
    for (name, yaml) in cases {
        group.throughput(Throughput::Bytes(yaml.len() as u64));
        group.bench_with_input(BenchmarkId::new("parse_events", name), yaml, |b, yaml| {
            b.iter(|| {
                let count = unsafe { libfyaml_parse_all(black_box(yaml)) };
                black_box(count)
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_throughput_by_size, bench_throughput_by_style);
criterion_main!(benches);
