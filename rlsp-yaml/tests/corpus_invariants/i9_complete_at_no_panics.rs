use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use rlsp_yaml::completion::complete_at;
use rlsp_yaml::parser::parse_yaml;
use tower_lsp::lsp_types::Position;

use super::i2_range_validity::utf16_len;
use super::shared::panic_message;

// Mirrors the private constant in completion.rs — must be kept in sync.
const MAX_COMPLETION_ITEMS: usize = 100;

pub fn check_i9_complete_at_no_panics(_path: &Path, text: &str) -> Result<(), String> {
    let docs = parse_yaml(text).documents;

    for (line, line_text) in text.lines().enumerate() {
        let line_utf16 = utf16_len(line_text) as u32;
        let col_0: u32 = 0;
        let col_mid: u32 = safe_utf16_midpoint(line_text);
        let col_end: u32 = line_utf16;

        // Deduplicate: avoid redundant calls on very short lines.
        let mut cols = vec![col_0];
        if col_mid != col_0 {
            cols.push(col_mid);
        }
        if col_end != col_mid {
            cols.push(col_end);
        }

        for col in cols {
            let pos = Position::new(line as u32, col);
            let result =
                catch_unwind(AssertUnwindSafe(|| complete_at(&docs, pos, None))).map_err(|e| {
                    format!(
                        "panic in complete_at at line {line} col {col}: {}",
                        panic_message(&e)
                    )
                })?;
            let n = result.len();
            if n > MAX_COMPLETION_ITEMS {
                return Err(format!(
                    "complete_at at line {line} col {col} returned {n} items (> MAX_COMPLETION_ITEMS {MAX_COMPLETION_ITEMS})"
                ));
            }
        }
    }

    Ok(())
}

/// Compute the UTF-16 midpoint of a line string, guarding against landing
/// inside a surrogate pair (supplementary-plane characters take 2 UTF-16
/// units; if `len / 2` falls on the second unit, advance by 1).
pub fn safe_utf16_midpoint(line: &str) -> u32 {
    let len = utf16_len(line) as u32;
    let mut mid = len / 2;
    // Walk UTF-16 units to verify `mid` lands on a code-point boundary.
    let mut units: u32 = 0;
    for ch in line.chars() {
        let ch_units = ch.len_utf16() as u32;
        if units == mid {
            return mid; // already on a boundary
        }
        if units + ch_units > mid {
            // `mid` falls inside a surrogate pair — advance past it.
            mid = units + ch_units;
            return mid;
        }
        units += ch_units;
    }
    mid
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn run_i9(text: &str) -> Result<(), String> {
        check_i9_complete_at_no_panics(Path::new("test.yaml"), text)
    }

    // UT-I9-1: empty file — zero lines, returns Ok immediately
    #[test]
    fn i9_ut1_empty_file_returns_ok() {
        assert!(run_i9("").is_ok());
    }

    // UT-I9-2: newline-only — one empty line, all cols collapse to 0, single call at (0,0)
    #[test]
    fn i9_ut2_newline_only_file_returns_ok() {
        assert!(run_i9("\n").is_ok());
    }

    // UT-I9-3: single-line YAML without trailing newline
    #[test]
    fn i9_ut3_single_line_no_newline_returns_ok() {
        assert!(run_i9("key: value").is_ok());
    }

    // UT-I9-4: multi-line YAML
    #[test]
    fn i9_ut4_multiline_yaml_returns_ok() {
        assert!(run_i9("a: 1\nb: 2\nc: 3\n").is_ok());
    }

    // UT-I9-5: BMP multi-byte UTF-8 ('é' = 2 UTF-8 bytes, 1 UTF-16 unit)
    #[test]
    fn i9_ut5_line_with_bmp_multibyte_char_returns_ok() {
        assert!(run_i9("café: value\n").is_ok());
    }

    // UT-I9-6: supplementary-plane emoji (😀 = 4 UTF-8 bytes, 2 UTF-16 units)
    #[test]
    fn i9_ut6_line_with_supplementary_plane_char_returns_ok() {
        assert!(run_i9("a\u{1F600}b: v\n").is_ok());
    }

    // UT-I9-7: 110-key mapping — exercises the len <= MAX_COMPLETION_ITEMS branch
    #[test]
    fn i9_ut7_large_mapping_respects_item_cap() {
        let mut yaml = String::new();
        for i in 1..=110_u32 {
            writeln!(yaml, "k{i}: v").expect("write to String is infallible");
        }
        assert!(run_i9(&yaml).is_ok());
    }
}
