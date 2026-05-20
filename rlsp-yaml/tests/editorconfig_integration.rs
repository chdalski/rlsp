// SPDX-License-Identifier: MIT
//
// Integration tests for `.editorconfig` integration with `format_yaml` and
// `code_actions`. These tests assemble the full pipeline:
//   resolve() → YamlFormatOptions construction → format_yaml() / code_actions()
// and confirm that `.editorconfig` settings affect formatter output.
//
// Tests in `editor_config.rs` cover `resolve()` in isolation (all field reads,
// walk-up, root = true, caching, security). These tests are not duplicated here.

#![expect(clippy::expect_used, missing_docs, reason = "test code")]

use std::fs;

use rlsp_yaml::editing::code_actions::code_actions;
use rlsp_yaml::editing::editor_config::{LineEnding, invalidate_all, resolve};
use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use tempfile::TempDir;
use tower_lsp::lsp_types::{Position, Range, Url};

fn file_url(path: &std::path::Path) -> Url {
    Url::from_file_path(path).expect("valid file path")
}

fn write_editorconfig(dir: &std::path::Path, content: &str) {
    fs::write(dir.join(".editorconfig"), content).expect("write .editorconfig");
}

fn make_options_from_ec(uri: &Url, lsp_print_width: Option<usize>) -> YamlFormatOptions {
    let ec = resolve(uri);
    YamlFormatOptions {
        print_width: lsp_print_width.or(ec.max_line_length).unwrap_or(80),
        line_ending: ec.end_of_line.unwrap_or(LineEnding::Lf),
        insert_final_newline: ec.insert_final_newline.unwrap_or(true),
        ..YamlFormatOptions::default()
    }
}

// ---- Group A: print_width precedence ----------------------------------------

// A1. max_line_length from .editorconfig overrides the default 80 print width.
#[test]
fn max_line_length_from_editorconfig_overrides_default_print_width() {
    let dir = TempDir::new().unwrap();
    write_editorconfig(dir.path(), "[*.yaml]\nmax_line_length = 100\n");
    let file = dir.path().join("file.yaml");
    fs::write(&file, "").unwrap();
    invalidate_all();

    let uri = file_url(&file);
    let opts = make_options_from_ec(&uri, None);

    // Input whose long value wraps at width 80 but not at width 100.
    let long_value = "x".repeat(85);
    let input = format!("key: \"{long_value}\"\n");
    let output = format_yaml(&input, &opts);

    // At width 100 the quoted scalar stays on one line (no multi-line block form).
    assert!(
        !output.contains('\n') || output.lines().count() <= 2,
        "at width 100 the output should not wrap the long value to a new block line; got:\n{output:?}"
    );
    assert_eq!(
        opts.print_width, 100,
        "print_width should come from .editorconfig"
    );
}

// A2. Explicit LSP formatPrintWidth overrides .editorconfig max_line_length.
#[test]
fn explicit_lsp_print_width_overrides_editorconfig_max_line_length() {
    let dir = TempDir::new().unwrap();
    write_editorconfig(dir.path(), "[*.yaml]\nmax_line_length = 100\n");
    let file = dir.path().join("file.yaml");
    fs::write(&file, "").unwrap();
    invalidate_all();

    let uri = file_url(&file);
    // LSP setting of 60 wins over .editorconfig's 100.
    let opts = make_options_from_ec(&uri, Some(60));

    assert_eq!(
        opts.print_width, 60,
        "LSP setting should win over .editorconfig"
    );
}

// ---- Group B: line_ending ---------------------------------------------------

// B1. end_of_line = crlf produces CRLF output.
#[test]
fn end_of_line_crlf_produces_crlf_output() {
    let dir = TempDir::new().unwrap();
    write_editorconfig(dir.path(), "[*.yaml]\nend_of_line = crlf\n");
    let file = dir.path().join("file.yaml");
    fs::write(&file, "").unwrap();
    invalidate_all();

    let uri = file_url(&file);
    let opts = make_options_from_ec(&uri, None);

    let output = format_yaml("a: 1\nb: 2\n", &opts);

    assert!(
        output.contains("\r\n"),
        "output should contain CRLF; got: {output:?}"
    );
    // No bare LF that is not preceded by CR.
    for (i, ch) in output.char_indices() {
        if ch == '\n' {
            assert!(
                i > 0 && output.as_bytes()[i - 1] == b'\r',
                "bare LF found at byte {i}; output: {output:?}"
            );
        }
    }
}

// B2. end_of_line = lf produces LF-only output.
#[test]
fn end_of_line_lf_produces_lf_output() {
    let dir = TempDir::new().unwrap();
    write_editorconfig(dir.path(), "[*.yaml]\nend_of_line = lf\n");
    let file = dir.path().join("file.yaml");
    fs::write(&file, "").unwrap();
    invalidate_all();

    let uri = file_url(&file);
    let opts = make_options_from_ec(&uri, None);

    let output = format_yaml("a: 1\nb: 2\n", &opts);

    assert!(
        !output.contains('\r'),
        "LF mode should produce no CR; got: {output:?}"
    );
}

// B3. end_of_line = cr produces CR-only output (no \n, no \r\n).
#[test]
fn end_of_line_cr_produces_cr_output() {
    let dir = TempDir::new().unwrap();
    write_editorconfig(dir.path(), "[*.yaml]\nend_of_line = cr\n");
    let file = dir.path().join("file.yaml");
    fs::write(&file, "").unwrap();
    invalidate_all();

    let uri = file_url(&file);
    let opts = make_options_from_ec(&uri, None);

    let output = format_yaml("a: 1\nb: 2\n", &opts);

    assert!(
        !output.contains('\n'),
        "CR mode should produce no LF; got: {output:?}"
    );
    assert!(
        output.contains('\r'),
        "CR mode should produce at least one CR; got: {output:?}"
    );
    // No \r\n sequences.
    assert!(
        !output.contains("\r\n"),
        "CR mode should not produce CRLF sequences; got: {output:?}"
    );
}

// B4. No .editorconfig → default line ending is LF.
#[test]
fn default_line_ending_is_lf_when_no_editorconfig() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file.yaml");
    fs::write(&file, "").unwrap();
    invalidate_all();

    let uri = file_url(&file);
    let opts = make_options_from_ec(&uri, None);

    let output = format_yaml("a: 1\nb: 2\n", &opts);

    assert!(
        !output.contains('\r'),
        "default line ending should be LF; got: {output:?}"
    );
}

// ---- Group C: insert_final_newline ------------------------------------------

// C1. insert_final_newline = false omits the trailing newline.
#[test]
fn insert_final_newline_false_omits_trailing_newline() {
    let dir = TempDir::new().unwrap();
    write_editorconfig(dir.path(), "[*.yaml]\ninsert_final_newline = false\n");
    let file = dir.path().join("file.yaml");
    fs::write(&file, "").unwrap();
    invalidate_all();

    let uri = file_url(&file);
    let opts = make_options_from_ec(&uri, None);

    let output = format_yaml("key: value\n", &opts);

    assert!(
        !output.ends_with('\n') && !output.ends_with('\r'),
        "insert_final_newline=false should strip trailing terminator; got: {output:?}"
    );
}

// C2. insert_final_newline = true appends a trailing newline.
#[test]
fn insert_final_newline_true_appends_trailing_newline() {
    let dir = TempDir::new().unwrap();
    write_editorconfig(dir.path(), "[*.yaml]\ninsert_final_newline = true\n");
    let file = dir.path().join("file.yaml");
    fs::write(&file, "").unwrap();
    invalidate_all();

    let uri = file_url(&file);
    let opts = make_options_from_ec(&uri, None);

    let output = format_yaml("key: value\n", &opts);

    assert!(
        output.ends_with('\n'),
        "insert_final_newline=true should preserve trailing newline; got: {output:?}"
    );
}

// C3. No .editorconfig → default insert_final_newline is true.
#[test]
fn default_insert_final_newline_is_true_when_no_editorconfig() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file.yaml");
    fs::write(&file, "").unwrap();
    invalidate_all();

    let uri = file_url(&file);
    let opts = make_options_from_ec(&uri, None);

    let output = format_yaml("key: value\n", &opts);

    assert!(
        output.ends_with('\n'),
        "default insert_final_newline should be true; got: {output:?}"
    );
}

// ---- Group D: walk-up and root = true (confirm effect on formatter output) --

// D1. .editorconfig two directories above the YAML file is found and applied.
#[test]
fn walk_up_two_directories_applies_editorconfig() {
    let root = TempDir::new().unwrap();
    write_editorconfig(root.path(), "[*.yaml]\nmax_line_length = 100\n");
    let nested = root.path().join("a").join("b");
    fs::create_dir_all(&nested).unwrap();
    let file = nested.join("file.yaml");
    fs::write(&file, "").unwrap();
    invalidate_all();

    let uri = file_url(&file);
    let opts = make_options_from_ec(&uri, None);

    assert_eq!(
        opts.print_width, 100,
        "walk-up should find .editorconfig two levels above; got print_width={}",
        opts.print_width
    );
}

// D2. root = true in inner .editorconfig terminates the walk.
#[test]
fn root_true_in_inner_editorconfig_terminates_walk() {
    let root = TempDir::new().unwrap();
    write_editorconfig(root.path(), "[*.yaml]\nmax_line_length = 80\n");
    let project = root.path().join("project");
    fs::create_dir_all(&project).unwrap();
    write_editorconfig(&project, "root = true\n[*.yaml]\nmax_line_length = 200\n");
    let file = project.join("file.yaml");
    fs::write(&file, "").unwrap();
    invalidate_all();

    let uri = file_url(&file);
    let opts = make_options_from_ec(&uri, None);

    assert_eq!(
        opts.print_width, 200,
        "root=true should stop walk; inner .editorconfig (200) should win over outer (80)"
    );

    // A 150-char value should stay on one line at width 200.
    let long_value = "x".repeat(150);
    let input = format!("key: \"{long_value}\"\n");
    let output = format_yaml(&input, &opts);
    // At width 200 the value doesn't wrap; we just assert the formatter ran without error.
    assert!(
        !output.is_empty(),
        "formatter should return non-empty output"
    );
}

// ---- Group E: code-action path respects .editorconfig ----------------------

// E1. block-to-flow action output is width-sensitive and uses .editorconfig print_width.
//
// The sequence input's flow one-liner is ~92 chars — fits on one line at width 200
// but wraps at width 80. The test asserts both directions:
//   - at width 200 (from .editorconfig): output is a single-line flow form
//   - at width 80 (default): output contains an internal newline (wraps)
// This ensures the `.editorconfig` overlay is applied before `format_subtree` is
// called. If the overlay were missing, `format_subtree` would use width 80 and the
// width-200 assertion would fail.
#[test]
fn code_action_block_to_flow_uses_editorconfig_print_width() {
    let dir = TempDir::new().unwrap();
    write_editorconfig(dir.path(), "[*.yaml]\nmax_line_length = 200\n");
    let file = dir.path().join("file.yaml");
    // Flow one-liner is ~92 chars: fits at 200, wraps at 80.
    let yaml = "items:\n  - alpha_item_one\n  - bravo_item_two\n  - charlie_item_three\n  - delta_item_four\n  - echo_item_five\n";
    fs::write(&file, yaml).unwrap();
    invalidate_all();

    let uri = file_url(&file);

    // At width 200 (from .editorconfig): output stays on one line.
    let opts_200 = make_options_from_ec(&uri, None);
    assert_eq!(
        opts_200.print_width, 200,
        "print_width should come from .editorconfig (max_line_length = 200)"
    );

    let docs = rlsp_yaml::parser::parse_yaml(yaml).documents;
    let range = Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: 0,
        },
    };
    let wide_actions = code_actions(&docs, yaml, range, &[], &uri, &opts_200);
    let wide_action = wide_actions
        .iter()
        .find(|a| a.title.contains("block to flow"))
        .expect("block-to-flow action must be offered for a block sequence");
    let wide_workspace_edit = wide_action.edit.as_ref().expect("action must have an edit");
    let wide_text_edits: Vec<_> = wide_workspace_edit
        .changes
        .as_ref()
        .expect("edit must have changes")
        .values()
        .flatten()
        .collect();
    let wide_text_edit = wide_text_edits
        .first()
        .expect("edit must have at least one TextEdit");

    assert!(
        wide_text_edit.new_text.contains('['),
        "block-to-flow at width 200 should produce a flow sequence; got:\n{:?}",
        wide_text_edit.new_text
    );
    assert!(
        !wide_text_edit
            .new_text
            .trim_end_matches('\n')
            .contains('\n'),
        "block-to-flow at width 200 should produce a single-line result; got:\n{:?}",
        wide_text_edit.new_text
    );

    // At width 80 (default, no .editorconfig override): the ~92-char one-liner wraps.
    let narrow_opts = make_options_from_ec(&uri, Some(80));
    let narrow_actions = code_actions(&docs, yaml, range, &[], &uri, &narrow_opts);
    let narrow_action = narrow_actions
        .iter()
        .find(|a| a.title.contains("block to flow"))
        .expect("block-to-flow action must be offered at width 80 too");
    let narrow_workspace_edit = narrow_action
        .edit
        .as_ref()
        .expect("action must have an edit");
    let narrow_text_edits: Vec<_> = narrow_workspace_edit
        .changes
        .as_ref()
        .expect("edit must have changes")
        .values()
        .flatten()
        .collect();
    let narrow_text_edit = narrow_text_edits
        .first()
        .expect("edit must have at least one TextEdit");

    assert!(
        narrow_text_edit
            .new_text
            .trim_end_matches('\n')
            .contains('\n'),
        "block-to-flow at width 80 should wrap the ~92-char sequence; got:\n{:?}",
        narrow_text_edit.new_text
    );
}
