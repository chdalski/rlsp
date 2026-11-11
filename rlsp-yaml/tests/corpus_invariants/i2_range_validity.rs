use std::path::Path;

use rlsp_yaml::parser::parse_yaml;

use super::shared::collect_all_diagnostics;

pub fn check_i2_range_validity(_path: &Path, text: &str) -> Result<(), String> {
    let parse_result = parse_yaml(text);
    let docs = parse_result.documents;
    let diagnostics = collect_all_diagnostics(&docs);
    check_diagnostic_ranges(text, &diagnostics)
}

/// Check that every diagnostic range in `diagnostics` is valid with respect to `text`.
///
/// Extracted so unit tests can inject synthetic diagnostics.
pub fn check_diagnostic_ranges(
    text: &str,
    diagnostics: &[tower_lsp::lsp_types::Diagnostic],
) -> Result<(), String> {
    let lines: Vec<&str> = text.lines().collect();
    let line_count = lines.len() as u32;

    for diag in diagnostics {
        let r = diag.range;
        let code = diag
            .code
            .as_ref()
            .map_or_else(|| "<no-code>".to_string(), |c| format!("{c:?}"));

        // Check 1: start <= end (line ordering, then character on same line)
        if r.start.line > r.end.line {
            return Err(format!(
                "diagnostic {code} range start.line ({}) > end.line ({})",
                r.start.line, r.end.line
            ));
        }
        if r.start.line == r.end.line && r.start.character > r.end.character {
            // u32::MAX is used as a "to end-of-line" sentinel from parser.rs
            if r.end.character != u32::MAX {
                return Err(format!(
                    "diagnostic {code} range same-line start.character ({}) > end.character ({})",
                    r.start.character, r.end.character
                ));
            }
        }

        // Check 2: end.line < line_count (0-based, strict less-than)
        if line_count == 0 {
            return Err(format!(
                "diagnostic {code} range references line {} but file has 0 lines",
                r.end.line
            ));
        }
        if r.end.line >= line_count {
            return Err(format!(
                "diagnostic {code} range end.line ({}) >= line_count ({})",
                r.end.line, line_count
            ));
        }

        // Check 3: character values within the code-point length of their lines,
        // the unit the features count (skip sentinel u32::MAX — it means "to end
        // of line"). A column in code points never splits a character.
        if r.start.character != u32::MAX {
            let start_line_chars = char_count(lines[r.start.line as usize]);
            if r.start.character > start_line_chars as u32 {
                return Err(format!(
                    "diagnostic {code} start.character ({}) > code-point length of line {} ({})",
                    r.start.character, r.start.line, start_line_chars
                ));
            }
        }

        if r.end.character != u32::MAX {
            let end_line_chars = char_count(lines[r.end.line as usize]);
            if r.end.character > end_line_chars as u32 {
                return Err(format!(
                    "diagnostic {code} end.character ({}) > code-point length of line {} ({})",
                    r.end.character, r.end.line, end_line_chars
                ));
            }
        }
    }

    Ok(())
}

/// Count the code points of a string: the unit of the columns the features
/// compute.
pub fn char_count(s: &str) -> usize {
    s.chars().count()
}

#[cfg(test)]
mod tests {
    use super::super::shared::helpers::{collect_from, make_diag, with_temp_dir};
    use super::*;

    // UT-1: empty diagnostic list always passes
    #[test]
    fn i2_ut1_empty_diagnostics_passes() {
        assert!(check_diagnostic_ranges("key: value\n", &[]).is_ok());
    }

    // UT-2: valid ASCII range — synthetic in-bounds diagnostic passes
    #[test]
    fn i2_ut2_valid_ascii_range_passes() {
        // "abc\n" — line 0 has 3 code points; range (0,0)-(0,3) is valid
        let result = check_diagnostic_ranges("abc\n", &[make_diag(0, 0, 0, 3)]);
        assert!(
            result.is_ok(),
            "valid in-bounds range should pass: {result:?}"
        );
    }

    // UT-3: start.line > end.line is detected as invalid by check_diagnostic_ranges
    #[test]
    fn i2_ut3_start_line_after_end_line_fails() {
        let result = check_diagnostic_ranges("line0\nline1\n", &[make_diag(1, 0, 0, 0)]);
        assert!(result.is_err(), "inverted line range should fail");
    }

    // UT-4: same-line start.character > end.character (non-sentinel) detected as invalid
    #[test]
    fn i2_ut4_same_line_start_char_after_end_char_fails() {
        // "abcde\n" — range (0,5)-(0,3) is inverted on same line
        let result = check_diagnostic_ranges("abcde\n", &[make_diag(0, 5, 0, 3)]);
        assert!(
            result.is_err(),
            "inverted char range on same line should fail"
        );
    }

    // UT-5: end.line == line_count (off by one) detected as out of bounds
    #[test]
    fn i2_ut5_end_line_equals_line_count_fails() {
        // "line0\nline1\n" has 2 lines (indices 0 and 1); line 2 is out of bounds
        let result = check_diagnostic_ranges("line0\nline1\n", &[make_diag(0, 0, 2, 0)]);
        assert!(result.is_err(), "end.line == line_count should fail");
    }

    // UT-6: character beyond the code-point line length detected as invalid
    #[test]
    fn i2_ut6_character_beyond_line_length_fails() {
        // "abc\n" — line 0 has 3 code points; character 4 is out of bounds
        let result = check_diagnostic_ranges("abc\n", &[make_diag(0, 0, 0, 4)]);
        assert!(
            result.is_err(),
            "character beyond the code-point line length should fail"
        );
    }

    // UT-7: multi-byte UTF-8 character — char_count counts code points
    #[test]
    fn i2_ut7_multibyte_utf8_counts_code_points() {
        // "café" — 'é' is U+00E9, 1 code point, 2 UTF-8 bytes
        let s = "café";
        assert_eq!(s.len(), 5); // UTF-8 bytes: c(1)+a(1)+f(1)+é(2)
        assert_eq!(char_count(s), 4);
    }

    // UT-8: supplementary-plane character (emoji) — 1 code point, 4 UTF-8 bytes
    #[test]
    fn i2_ut8_supplementary_plane_counts_as_one_code_point() {
        // U+1F600 GRINNING FACE — 4 UTF-8 bytes
        let s = "a\u{1F600}b";
        assert_eq!(s.len(), 6); // UTF-8: 1+4+1
        assert_eq!(char_count(s), 3);
    }

    // UT-9: the columns of a line with an emoji count it once
    #[test]
    fn i2_ut9_columns_after_emoji_count_code_points() {
        let text = "a\u{1F600}b\n";
        assert!(check_diagnostic_ranges(text, &[make_diag(0, 0, 0, 3)]).is_ok());
        assert!(check_diagnostic_ranges(text, &[make_diag(0, 0, 0, 4)]).is_err());
    }

    // UT-10: sentinel u32::MAX end.character passes check_diagnostic_ranges
    #[test]
    fn i2_ut10_sentinel_u32_max_skips_character_bound_check() {
        // u32::MAX as end.character is the "to end of line" sentinel from parser.rs:59.
        // A diagnostic with that sentinel on a valid line must not trigger failures.
        let result = check_diagnostic_ranges("line0\n", &[make_diag(0, 0, 0, u32::MAX)]);
        assert!(
            result.is_ok(),
            "u32::MAX sentinel should pass without triggering char-bound check: {result:?}"
        );
    }

    // UT-11: multi-line range with valid endpoints passes check_diagnostic_ranges
    #[test]
    fn i2_ut11_multiline_range_start_before_end_passes() {
        // "line0\nline1\nline2\n" — range (0,0)-(2,5) spans 3 lines; line2="line2" len=5
        let result = check_diagnostic_ranges("line0\nline1\nline2\n", &[make_diag(0, 0, 2, 5)]);
        assert!(
            result.is_ok(),
            "valid multiline range should pass: {result:?}"
        );
    }

    // filesystem-bounded tests that exercise collect_from (shared helper)
    #[test]
    fn collect_corpus_files_finds_yml_and_yaml() {
        with_temp_dir(|dir| {
            std::fs::File::create(dir.join("a.yml")).unwrap();
            std::fs::File::create(dir.join("b.yaml")).unwrap();
            std::fs::File::create(dir.join("c.txt")).unwrap();
            std::fs::File::create(dir.join("d.json")).unwrap();

            let files = collect_from(dir);
            let names: Vec<_> = files
                .iter()
                .map(|p| p.file_name().unwrap().to_str().unwrap())
                .collect();
            assert!(names.contains(&"a.yml"), "expected a.yml, got {names:?}");
            assert!(names.contains(&"b.yaml"), "expected b.yaml, got {names:?}");
            assert!(!names.contains(&"c.txt"), "unexpected c.txt in {names:?}");
            assert!(!names.contains(&"d.json"), "unexpected d.json in {names:?}");
            assert_eq!(names.len(), 2);
        });
    }

    #[test]
    fn collect_corpus_files_returns_empty_for_empty_dir() {
        with_temp_dir(|dir| {
            assert!(collect_from(dir).is_empty());
        });
    }

    #[test]
    fn collect_corpus_files_excludes_subdirectories() {
        with_temp_dir(|dir| {
            std::fs::File::create(dir.join("file.yaml")).unwrap();
            std::fs::create_dir(dir.join("sub")).unwrap();

            let files = collect_from(dir);
            let names: Vec<_> = files
                .iter()
                .map(|p| p.file_name().unwrap().to_str().unwrap())
                .collect();
            assert_eq!(names, vec!["file.yaml"]);
        });
    }
}
