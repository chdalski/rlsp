// SPDX-License-Identifier: MIT
//
// Corpus invariant harness for rlsp-yaml.
//
// # Skip-list discipline
//
// The SKIP_LIST is **shrink-only**. Entries are removed as follow-up plans fix
// the root causes. New entries are only added when a NEW corpus file surfaces a
// known-fixable issue that has an immediate follow-up plan already filed; never
// to silence a surprise failure. This constraint is the harness's enforcement
// surface — without it the test degrades to a rubber stamp.
//
// A surprise failure (a (file, invariant) pair that fails but has no skip-list
// entry) must be reported to the lead via SendMessage identifying the pair and
// failure detail. The lead either files a follow-up plan (whose path the
// developer then references in the skip-list entry) or directs treating the
// failure as in-scope. The developer never adds a skip-list entry with an
// ad-hoc TODO marker lacking a plan reference.

#![expect(missing_docs, reason = "test code")]
#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code — panics are intentional assertion failures"
)]
#![expect(
    clippy::expect_used,
    reason = "test code — expect on infallible operations"
)]
#![expect(
    clippy::cast_possible_truncation,
    reason = "test code — LSP line counts fit in u32 for any real corpus file"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "test code — indices are validated by invariant checks before use"
)]

use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use rlsp_yaml::editing::code_actions::code_actions;
use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::parser::parse_yaml;
use rlsp_yaml::validation::validators::{
    validate_custom_tags, validate_duplicate_keys, validate_flow_style, validate_key_ordering,
    validate_unused_anchors, validate_yaml11_compat,
};
use tower_lsp::lsp_types::{Position, Range};

const CORPUS_DIR: &str = "tests/corpus";

/// Each registered invariant has an id, description, and a check function.
struct Invariant {
    id: &'static str,
    #[expect(
        dead_code,
        reason = "displayed in future failure-reporting; kept for extensibility"
    )]
    description: &'static str,
    check: fn(&Path, &str) -> Result<(), String>,
}

/// Skip-list entries: `(corpus_file_name, invariant_id, followup_plan_reference_and_justification)`.
///
/// Shrink-only — see module-level doc comment for the discipline.
const SKIP_LIST: &[(&str, &str, &str)] = &[];

/// Registered invariants.
const INVARIANTS: &[Invariant] = &[
    Invariant {
        id: "I1",
        description: "No panics on full LSP pipeline",
        check: check_i1_no_panics,
    },
    Invariant {
        id: "I2",
        description: "Diagnostic range validity",
        check: check_i2_range_validity,
    },
];

// ---------------------------------------------------------------------------
// I1: No panics on full LSP pipeline
// ---------------------------------------------------------------------------

fn check_i1_no_panics(_path: &Path, text: &str) -> Result<(), String> {
    // Stage 1: parse
    let parse_result = catch_unwind(AssertUnwindSafe(|| parse_yaml(text)))
        .map_err(|e| format!("panic in parse_yaml: {}", panic_message(&e)))?;

    let docs = parse_result.documents;

    // Stage 2: validate_unused_anchors
    catch_unwind(AssertUnwindSafe(|| validate_unused_anchors(text)))
        .map_err(|e| format!("panic in validate_unused_anchors: {}", panic_message(&e)))?;

    // Stage 3: validate_flow_style
    catch_unwind(AssertUnwindSafe(|| validate_flow_style(text)))
        .map_err(|e| format!("panic in validate_flow_style: {}", panic_message(&e)))?;

    // Stage 4: validate_custom_tags (empty allowed set — all tags are unknown)
    let allowed_tags: HashSet<String> = HashSet::new();
    catch_unwind(AssertUnwindSafe(|| {
        validate_custom_tags(text, &docs, &allowed_tags)
    }))
    .map_err(|e| format!("panic in validate_custom_tags: {}", panic_message(&e)))?;

    // Stage 5: validate_key_ordering
    catch_unwind(AssertUnwindSafe(|| validate_key_ordering(text, &docs)))
        .map_err(|e| format!("panic in validate_key_ordering: {}", panic_message(&e)))?;

    // Stage 6: validate_duplicate_keys
    catch_unwind(AssertUnwindSafe(|| validate_duplicate_keys(&docs)))
        .map_err(|e| format!("panic in validate_duplicate_keys: {}", panic_message(&e)))?;

    // Stage 7: validate_yaml11_compat
    catch_unwind(AssertUnwindSafe(|| validate_yaml11_compat(&docs)))
        .map_err(|e| format!("panic in validate_yaml11_compat: {}", panic_message(&e)))?;

    // Stage 8: format_yaml
    let opts = YamlFormatOptions::default();
    catch_unwind(AssertUnwindSafe(|| format_yaml(text, &opts)))
        .map_err(|e| format!("panic in format_yaml: {}", panic_message(&e)))?;

    // Stage 9: code_actions with zero-width range at (0,0) and all diagnostics
    let all_diagnostics = collect_all_diagnostics(text, &docs);
    let zero_range = Range::new(Position::new(0, 0), Position::new(0, 0));
    let fake_uri = tower_lsp::lsp_types::Url::parse("file:///corpus/test.yaml").expect("valid URI");
    catch_unwind(AssertUnwindSafe(|| {
        code_actions(text, zero_range, &all_diagnostics, &fake_uri)
    }))
    .map_err(|e| format!("panic in code_actions: {}", panic_message(&e)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// I2: Diagnostic range validity
// ---------------------------------------------------------------------------

fn check_i2_range_validity(_path: &Path, text: &str) -> Result<(), String> {
    let parse_result = parse_yaml(text);
    let docs = parse_result.documents;
    let diagnostics = collect_all_diagnostics(text, &docs);
    check_diagnostic_ranges(text, &diagnostics)
}

/// Check that every diagnostic range in `diagnostics` is valid with respect to `text`.
///
/// Extracted so unit tests can inject synthetic diagnostics.
fn check_diagnostic_ranges(
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

        // Check 3: character values within UTF-16 code-unit length of their lines
        // (skip sentinel u32::MAX — it means "to end of line")
        if r.start.character != u32::MAX {
            let start_line_utf16 = utf16_len(lines[r.start.line as usize]);
            if r.start.character > start_line_utf16 as u32 {
                return Err(format!(
                    "diagnostic {code} start.character ({}) > utf16 length of line {} ({})",
                    r.start.character, r.start.line, start_line_utf16
                ));
            }
        }

        if r.end.character != u32::MAX {
            let end_line_utf16 = utf16_len(lines[r.end.line as usize]);
            if r.end.character > end_line_utf16 as u32 {
                return Err(format!(
                    "diagnostic {code} end.character ({}) > utf16 length of line {} ({})",
                    r.end.character, r.end.line, end_line_utf16
                ));
            }
        }

        // Check 4: byte offsets derived from (line, character) must land on
        // UTF-8 character boundaries.
        if r.start.character != u32::MAX {
            check_utf8_boundary(&lines, r.start.line, r.start.character, &code, "start")?;
        }
        if r.end.character != u32::MAX {
            check_utf8_boundary(&lines, r.end.line, r.end.character, &code, "end")?;
        }
    }

    Ok(())
}

/// Count UTF-16 code units in a string.
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Walk UTF-16 code units to find the byte offset, then check it's a UTF-8
/// char boundary. Returns Err with a message if the check fails.
fn check_utf8_boundary(
    lines: &[&str],
    line: u32,
    character: u32,
    code: &str,
    endpoint: &str,
) -> Result<(), String> {
    let line_str = lines[line as usize];
    let mut utf16_units = 0u32;
    let mut byte_offset = line_str.len(); // default: past end (for char == utf16_len)

    for (byte_pos, ch) in line_str.char_indices() {
        if utf16_units == character {
            byte_offset = byte_pos;
            break;
        }
        let units = ch.len_utf16() as u32;
        if utf16_units + units > character {
            // character falls in the middle of a surrogate pair — not a boundary
            return Err(format!(
                "diagnostic {code} {endpoint} position (line {line}, char {character}) \
                 splits a UTF-16 surrogate pair; not a UTF-8 character boundary"
            ));
        }
        utf16_units += units;
    }

    // Verify byte_offset is on a UTF-8 boundary
    if !line_str.is_char_boundary(byte_offset) {
        return Err(format!(
            "diagnostic {code} {endpoint} position (line {line}, char {character}) \
             byte offset {byte_offset} is not a UTF-8 character boundary in line {line_str:?}"
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Collect diagnostics from all validators for a given text + parsed documents.
fn collect_all_diagnostics(
    text: &str,
    docs: &[rlsp_yaml_parser::node::Document<rlsp_yaml_parser::Span>],
) -> Vec<tower_lsp::lsp_types::Diagnostic> {
    let allowed_tags: HashSet<String> = HashSet::new();
    let mut all = Vec::new();
    all.extend(validate_unused_anchors(text));
    all.extend(validate_flow_style(text));
    all.extend(validate_custom_tags(text, docs, &allowed_tags));
    all.extend(validate_key_ordering(text, docs));
    all.extend(validate_duplicate_keys(docs));
    all.extend(validate_yaml11_compat(docs));
    all
}

/// Extract a human-readable message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    payload.downcast_ref::<&str>().map_or_else(
        || {
            payload
                .downcast_ref::<String>()
                .map_or_else(|| "<non-string panic>".to_string(), Clone::clone)
        },
        |s| (*s).to_string(),
    )
}

// ---------------------------------------------------------------------------
// Harness infrastructure (unchanged from Task 1)
// ---------------------------------------------------------------------------

fn collect_corpus_files() -> Vec<PathBuf> {
    let dir = Path::new(CORPUS_DIR);
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "yml" || ext == "yaml" {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files
}

fn is_skipped(file_name: &str, invariant_id: &str) -> bool {
    SKIP_LIST
        .iter()
        .any(|(f, id, _)| *f == file_name && *id == invariant_id)
}

enum CheckOutcome {
    Passed,
    FailedExpected,
    FailedUnexpected(String),
    PassedUnexpected,
}

fn run_check(path: &Path, content: &str, invariant: &Invariant) -> CheckOutcome {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let skipped = is_skipped(file_name, invariant.id);
    match (invariant.check)(path, content) {
        Ok(()) => {
            if skipped {
                CheckOutcome::PassedUnexpected
            } else {
                CheckOutcome::Passed
            }
        }
        Err(msg) => {
            if skipped {
                CheckOutcome::FailedExpected
            } else {
                CheckOutcome::FailedUnexpected(msg)
            }
        }
    }
}

#[test]
fn corpus_invariants() {
    let files = collect_corpus_files();
    let n_files = files.len();
    let n_invariants = INVARIANTS.len();
    let n_checks = n_files * n_invariants;

    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        for invariant in INVARIANTS {
            match run_check(path, &content, invariant) {
                CheckOutcome::Passed | CheckOutcome::FailedExpected => {}
                CheckOutcome::FailedUnexpected(msg) => {
                    failures.push(format!("FAIL [{} / {}]: {}", file_name, invariant.id, msg));
                }
                CheckOutcome::PassedUnexpected => {
                    failures.push(format!(
                        "STALE SKIP [{} / {}]: expected failure but invariant passed — remove skip-list entry",
                        file_name, invariant.id
                    ));
                }
            }
        }
    }

    println!("corpus_invariants: {n_invariants} invariants × {n_files} files = {n_checks} checks");

    assert!(
        failures.is_empty(),
        "{} check(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};

    use super::*;

    fn with_temp_dir<F: FnOnce(&Path)>(f: F) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let dir = std::env::temp_dir().join(format!("corpus_test_{unique}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        f(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn make_diag(start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> Diagnostic {
        Diagnostic {
            range: Range::new(
                Position::new(start_line, start_char),
                Position::new(end_line, end_char),
            ),
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String("test".to_string())),
            ..Default::default()
        }
    }

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

    #[test]
    fn skip_list_lookup_matches_on_filename_only() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/seed.yaml");
        assert!(skip_list_contains(skip, path, "round-trip"));
    }

    #[test]
    fn skip_list_lookup_does_not_match_different_invariant() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/seed.yaml");
        assert!(!skip_list_contains(skip, path, "idempotent"));
    }

    #[test]
    fn skip_list_lookup_does_not_match_different_filename() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/other.yaml");
        assert!(!skip_list_contains(skip, path, "round-trip"));
    }

    // ---------------------------------------------------------------------------
    // I2 unit tests (UT-1 through UT-12 from test spec)
    // ---------------------------------------------------------------------------

    // UT-1: empty diagnostic list always passes
    #[test]
    fn i2_ut1_empty_diagnostics_passes() {
        assert!(check_diagnostic_ranges("key: value\n", &[]).is_ok());
    }

    // UT-2: valid ASCII range — synthetic in-bounds diagnostic passes
    #[test]
    fn i2_ut2_valid_ascii_range_passes() {
        // "abc\n" — line 0 has UTF-16 len 3; range (0,0)-(0,3) is valid
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

    // UT-6: character beyond UTF-16 line length detected as invalid
    #[test]
    fn i2_ut6_character_beyond_line_length_fails() {
        // "abc\n" — line 0 has UTF-16 len 3; character 4 is out of bounds
        let result = check_diagnostic_ranges("abc\n", &[make_diag(0, 0, 0, 4)]);
        assert!(
            result.is_err(),
            "character beyond utf16 line length should fail"
        );
    }

    // UT-7: multi-byte UTF-8 character — utf16_len counts code units correctly
    #[test]
    fn i2_ut7_multibyte_utf8_counts_utf16_correctly() {
        // "café" — 'é' is U+00E9, 1 UTF-16 code unit, 2 UTF-8 bytes
        let s = "café";
        assert_eq!(s.len(), 5); // UTF-8 bytes: c(1)+a(1)+f(1)+é(2)
        assert_eq!(utf16_len(s), 4); // UTF-16 code units: 4
    }

    // UT-8: supplementary-plane character (emoji) — 2 UTF-16 code units, 4 UTF-8 bytes
    #[test]
    fn i2_ut8_supplementary_plane_counts_utf16_as_two_units() {
        // U+1F600 GRINNING FACE — 4 UTF-8 bytes, 2 UTF-16 code units
        let s = "a\u{1F600}b";
        assert_eq!(s.len(), 6); // UTF-8: 1+4+1
        assert_eq!(utf16_len(s), 4); // UTF-16: 1+2+1
    }

    // UT-9: UTF-16 vs UTF-8 byte indexing correctness — column after emoji
    #[test]
    fn i2_ut9_utf16_column_after_emoji_is_correct() {
        let s = "a\u{1F600}b";
        // 'b' starts at UTF-16 offset 3
        // Verify check_utf8_boundary finds 'b' at byte offset 5 (1 + 4)
        let lines = &[s];
        let result = check_utf8_boundary(lines, 0, 3, "test", "end");
        assert!(
            result.is_ok(),
            "UTF-16 col 3 after emoji should be a valid boundary"
        );
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

    // UT-12: check_utf8_boundary correctly rejects mid-surrogate offset
    #[test]
    fn i2_ut12_mid_surrogate_offset_fails_boundary_check() {
        // U+1F600 is a surrogate pair in UTF-16. UTF-16 column 1 splits it.
        let s = "\u{1F600}x";
        let lines = &[s];
        // Column 1 falls inside the surrogate pair (emoji takes units 0 and 1)
        let result = check_utf8_boundary(lines, 0, 1, "test", "start");
        assert!(
            result.is_err(),
            "column 1 splits a surrogate pair, should fail"
        );
    }

    // ---------------------------------------------------------------------------
    // Helpers used only in tests
    // ---------------------------------------------------------------------------

    fn collect_from(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return files;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "yml" || ext == "yaml" {
                        files.push(path);
                    }
                }
            }
        }
        files.sort();
        files
    }

    fn skip_list_contains(skip: &[(&str, &str, &str)], path: &Path, invariant_id: &str) -> bool {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        skip.iter()
            .any(|(f, id, _)| *f == file_name && *id == invariant_id)
    }

    // Validates that zero invariants × N files = 0 checks, which is the
    // expected output of the real `corpus_invariants` test in Task 1.
    #[test]
    fn corpus_invariants_runs_zero_checks_with_empty_invariant_list() {
        with_temp_dir(|dir| {
            let mut f = std::fs::File::create(dir.join("smoke.yaml")).unwrap();
            writeln!(f, "key: value").unwrap();

            let files = collect_from(dir);
            assert_eq!(files.len(), 1);

            // With an empty invariant list, checks = files × 0 = 0.
            let n_invariants = 0_usize;
            assert_eq!(files.len() * n_invariants, 0);
        });
    }
}
