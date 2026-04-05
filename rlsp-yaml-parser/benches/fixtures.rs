// SPDX-License-Identifier: MIT

//! Synthetic YAML fixture generator for benchmarks.
//!
//! Produces deterministic YAML documents at controlled sizes and styles.
//! All generators are pure functions with no I/O.

#![allow(dead_code)]

use std::fmt::Write as _;

// ---------------------------------------------------------------------------
// Size constants
// ---------------------------------------------------------------------------

/// Target ~100 bytes.
pub const TINY_TARGET: usize = 100;
/// Target ~10 KB.
pub const MEDIUM_TARGET: usize = 10_000;
/// Target ~100 KB.
pub const LARGE_TARGET: usize = 100_000;
/// Target ~1 MB.
pub const HUGE_TARGET: usize = 1_000_000;

// ---------------------------------------------------------------------------
// Block-heavy: deeply nested mappings and sequences
// ---------------------------------------------------------------------------

/// Block-heavy YAML: nested mappings of the requested approximate byte count.
#[must_use]
pub fn block_heavy(target_bytes: usize) -> String {
    let mut out = String::with_capacity(target_bytes + 64);
    out.push_str("---\n");
    let mut count = 0usize;
    while out.len() < target_bytes {
        let key = format!("item_{count}");
        let _ = writeln!(
            out,
            "{key}:\n  name: value_{count}\n  enabled: true\n  count: {count}\n  ratio: 0.{count:04}"
        );
        count += 1;
    }
    out
}

/// Block sequence YAML: a sequence of scalar values.
#[must_use]
pub fn block_sequence(target_bytes: usize) -> String {
    let mut out = String::with_capacity(target_bytes + 64);
    out.push_str("---\nitems:\n");
    let mut count = 0usize;
    while out.len() < target_bytes {
        let _ = writeln!(out, "  - item_value_{count}");
        count += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Flow-heavy: flow mappings and sequences
// ---------------------------------------------------------------------------

/// Flow-heavy YAML: a sequence of flow mapping objects.
#[must_use]
pub fn flow_heavy(target_bytes: usize) -> String {
    let mut out = String::with_capacity(target_bytes + 64);
    out.push_str("---\nitems:\n");
    let mut count = 0usize;
    while out.len() < target_bytes {
        let _ = writeln!(
            out,
            "  - {{name: value_{count}, enabled: true, count: {count}}}"
        );
        count += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Scalar-heavy: many scalars with varied styles
// ---------------------------------------------------------------------------

/// Scalar-heavy YAML: many mapping entries with different scalar styles.
#[must_use]
pub fn scalar_heavy(target_bytes: usize) -> String {
    let mut out = String::with_capacity(target_bytes + 64);
    out.push_str("---\n");
    let mut count = 0usize;
    while out.len() < target_bytes {
        match count % 4 {
            0 => {
                let _ = writeln!(out, "plain_{count}: plain scalar value {count}");
            }
            1 => {
                let _ = writeln!(out, "quoted_{count}: \"double quoted value {count}\"");
            }
            2 => {
                let _ = writeln!(out, "single_{count}: 'single quoted value {count}'");
            }
            _ => {
                let _ = writeln!(
                    out,
                    "literal_{count}: |\n  first line of block scalar {count}\n  second line"
                );
            }
        }
        count += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Mixed: blend of block, flow, and scalar content
// ---------------------------------------------------------------------------

/// Mixed YAML: interleaved block and flow constructs.
#[must_use]
pub fn mixed(target_bytes: usize) -> String {
    let mut out = String::with_capacity(target_bytes + 64);
    out.push_str("---\n");
    let mut count = 0usize;
    while out.len() < target_bytes {
        match count % 3 {
            0 => {
                let _ = writeln!(
                    out,
                    "block_{count}:\n  key: value_{count}\n  nested:\n    deep: true"
                );
            }
            1 => {
                let _ = writeln!(out, "flow_{count}: [a_{count}, b_{count}, c_{count}]");
            }
            _ => {
                let _ = writeln!(out, "scalar_{count}: plain text value number {count}");
            }
        }
        count += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Named sizes using the style functions
// ---------------------------------------------------------------------------

/// Tiny fixture (~100 bytes).
#[must_use]
pub fn tiny() -> String {
    mixed(TINY_TARGET)
}

/// Medium fixture (~10 KB).
#[must_use]
pub fn medium() -> String {
    mixed(MEDIUM_TARGET)
}

/// Large fixture (~100 KB).
#[must_use]
pub fn large() -> String {
    mixed(LARGE_TARGET)
}

/// Huge fixture (~1 MB).
#[must_use]
pub fn huge() -> String {
    mixed(HUGE_TARGET)
}
