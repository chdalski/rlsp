// SPDX-License-Identifier: MIT
//
// Property-preservation invariant for cursor-driven mutating code actions.
//
// For each non-delete_anchor cursor-driven mutating action, this test:
// 1. Feeds an input containing &anchor, !mytag, or &a !mytag (combined)
//    while satisfying the action's trigger precondition.
// 2. Dispatches code_actions(...) and applies the matching action's first TextEdit.
// 3. Asserts: for each property literal in the input, count(input) == count(output).
//
// delete_anchor is intentionally excluded — its purpose is to remove an anchor.

#![expect(missing_docs, reason = "test code")]
#![expect(clippy::panic, reason = "test harness reports failures via panic")]

mod common;
use common::*;

use rlsp_yaml::editing::code_actions::code_actions;
use rlsp_yaml::editing::formatter::YamlFormatOptions;
use rlsp_yaml::validation::validators::validate_flow_style;
use rstest::rstest;
use tower_lsp::lsp_types::{Diagnostic, NumberOrString, Position, Range};

fn line_range(line: u32) -> Range {
    Range::new(Position::new(line, 0), Position::new(line, 999))
}

fn make_diagnostic(line: u32, start_col: u32, end_col: u32, code: &str) -> Diagnostic {
    Diagnostic {
        range: Range::new(Position::new(line, start_col), Position::new(line, end_col)),
        code: Some(NumberOrString::String(code.to_string())),
        source: Some("rlsp-yaml".to_string()),
        ..Diagnostic::default()
    }
}

/// Non-overlapping substring count.
fn count(haystack: &str, needle: &str) -> usize {
    let mut n = 0;
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        n += 1;
        start += pos + needle.len();
    }
    n
}

// ---- Invariant assertion ----

/// For each property literal in the input, assert count(input) == count(output).
fn assert_properties_preserved(input: &str, output: &str, properties: &[&str]) {
    for prop in properties {
        let in_count = count(input, prop);
        let out_count = count(output, prop);
        assert_eq!(
            in_count, out_count,
            "property {prop:?} count changed: input has {in_count}, output has {out_count}\ninput:  {input:?}\noutput: {output:?}",
        );
    }
}

// ---- Test parameters ----
//
// Each case is: (input_yaml, cursor_range, diagnostics, title_substr, properties_to_check)
//
// Inputs are designed so the action's trigger precondition holds AND the input
// contains the named property literals.

// string_to_block_scalar: block mapping value, quoted scalar >= 40 chars, cursor on line 0.
// Edit range covers scalar token only (not &anchor/!tag prefix). Task 1 fix clears clone props.
const BLOCK_SCALAR_ANCHOR: &str =
    "description: &anchor \"this is a long string that exceeds forty characters in total\"\n";
const BLOCK_SCALAR_TAG: &str =
    "description: !mytag \"this is a long string that exceeds forty characters in total\"\n";
const BLOCK_SCALAR_ANCHOR_AND_TAG: &str =
    "description: &a !mytag \"this is a long string that exceeds forty characters in total\"\n";

// block_to_flow: anchor/tag on the block-collection value. Edit range starts at key_end_col+1
// (after the colon), which covers the &anchor/!tag prefix plus the entire collection body.
// format_subtree re-emits the property exactly once; the source occurrence is inside the
// replaced range. Net count: 1 -> 1.
const BLOCK_TO_FLOW_ANCHOR: &str = "key: &anchor\n  a: 1\n  b: 2\n";
const BLOCK_TO_FLOW_TAG: &str = "key: !mytag\n  a: 1\n  b: 2\n";
const BLOCK_TO_FLOW_ANCHOR_AND_TAG: &str = "key: &a !mytag\n  a: 1\n  b: 2\n";

// flow_to_block (flow mapping): anchor/tag before the flow-map node. edit_start_col = loc.start
// (the '{' position), so the &anchor/!tag prefix is outside the replaced range. The source
// occurrence is preserved by the source buffer; the clone originally re-emitted the property
// in new_text. Fix: clear anchor/tag from the clone before format_subtree.
const FLOW_TO_BLOCK_MAP_ANCHOR: &str = "config: &anchor {a: 1, b: 2}\n";
const FLOW_TO_BLOCK_MAP_TAG: &str = "config: !mytag {a: 1, b: 2}\n";
const FLOW_TO_BLOCK_MAP_ANCHOR_AND_TAG: &str = "config: &a !mytag {a: 1, b: 2}\n";

// flow_to_block (flow sequence): same shape as flow mapping.
const FLOW_TO_BLOCK_SEQ_ANCHOR: &str = "items: &anchor [a, b, c]\n";
const FLOW_TO_BLOCK_SEQ_TAG: &str = "items: !mytag [a, b, c]\n";
const FLOW_TO_BLOCK_SEQ_ANCHOR_AND_TAG: &str = "items: &a !mytag [a, b, c]\n";

// quoted_bool: cursor positioned on the quoted boolean. Edit covers scalar only (not prefix).
// Clone retains anchor/tag; fix: clear clone props before format_subtree.
// "enabled: &anchor \"true\"" — "true" starts at col 17
// "enabled: !mytag \"true\""  — "true" starts at col 16
// "enabled: &a !mytag \"true\"" — "true" starts at col 19
const QUOTED_BOOL_ANCHOR: &str = "enabled: &anchor \"true\"\n";
const QUOTED_BOOL_TAG: &str = "enabled: !mytag \"true\"\n";
const QUOTED_BOOL_ANCHOR_AND_TAG: &str = "enabled: &a !mytag \"true\"\n";

// tab_to_spaces: tab in leading position, property in value region. Text-only replacement;
// properties are not touched. Always passes.
const TAB_TO_SPACES_ANCHOR: &str = "\tkey: &anchor value\n";
const TAB_TO_SPACES_TAG: &str = "\tkey: !mytag value\n";
const TAB_TO_SPACES_ANCHOR_AND_TAG: &str = "\tkey: &a !mytag value\n";

// yaml11_bool: diagnostic with exact column range matching the yaml11 bool scalar.
// Edit covers scalar only (not prefix). Clone retains anchor/tag; fix: clear before format_subtree.
// "enabled: &anchor yes" — "yes" at col 17
// "enabled: !mytag yes"  — "yes" at col 16
// "enabled: &a !mytag yes" — "yes" at col 19
const YAML11_BOOL_ANCHOR: &str = "enabled: &anchor yes\n";
const YAML11_BOOL_TAG: &str = "enabled: !mytag yes\n";
const YAML11_BOOL_ANCHOR_AND_TAG: &str = "enabled: &a !mytag yes\n";

// yaml11_octal: diagnostic with exact column range matching the octal scalar.
// "mode: &anchor 0755" — "0755" at col 14
// "mode: !mytag 0755"  — "0755" at col 13
// "mode: &a !mytag 0755" — "0755" at col 16
const YAML11_OCTAL_ANCHOR: &str = "mode: &anchor 0755\n";
const YAML11_OCTAL_TAG: &str = "mode: !mytag 0755\n";
const YAML11_OCTAL_ANCHOR_AND_TAG: &str = "mode: &a !mytag 0755\n";

// ---- Parameterized invariant test ----

#[rstest]
// string_to_block_scalar — Task 1 fix already applied
#[case::string_to_block_scalar_anchor(
    BLOCK_SCALAR_ANCHOR, cursor_range(0, 0), vec![], "block scalar", vec!["&anchor"]
)]
#[case::string_to_block_scalar_tag(
    BLOCK_SCALAR_TAG, cursor_range(0, 0), vec![], "block scalar", vec!["!mytag"]
)]
#[case::string_to_block_scalar_anchor_and_tag(
    BLOCK_SCALAR_ANCHOR_AND_TAG, cursor_range(0, 0), vec![], "block scalar", vec!["&a", "!mytag"]
)]
// block_to_flow — edit range covers the property prefix; format_subtree re-emits exactly once
#[case::block_to_flow_anchor(
    BLOCK_TO_FLOW_ANCHOR, cursor_range(0, 0), vec![], "block to flow", vec!["&anchor"]
)]
#[case::block_to_flow_tag(
    BLOCK_TO_FLOW_TAG, cursor_range(0, 0), vec![], "block to flow", vec!["!mytag"]
)]
#[case::block_to_flow_anchor_and_tag(
    BLOCK_TO_FLOW_ANCHOR_AND_TAG, cursor_range(0, 0), vec![], "block to flow", vec!["&a", "!mytag"]
)]
// flow_to_block (flow mapping) — diagnostic-driven; fix: clear clone props
#[case::flow_to_block_map_anchor(
    FLOW_TO_BLOCK_MAP_ANCHOR,
    Range::new(Position::new(0, 0), Position::new(999, 0)),
    {
        let docs = docs_for(FLOW_TO_BLOCK_MAP_ANCHOR);
        validate_flow_style(&docs)
    },
    "flow mapping",
    vec!["&anchor"]
)]
#[case::flow_to_block_map_tag(
    FLOW_TO_BLOCK_MAP_TAG,
    Range::new(Position::new(0, 0), Position::new(999, 0)),
    {
        let docs = docs_for(FLOW_TO_BLOCK_MAP_TAG);
        validate_flow_style(&docs)
    },
    "flow mapping",
    vec!["!mytag"]
)]
#[case::flow_to_block_map_anchor_and_tag(
    FLOW_TO_BLOCK_MAP_ANCHOR_AND_TAG,
    Range::new(Position::new(0, 0), Position::new(999, 0)),
    {
        let docs = docs_for(FLOW_TO_BLOCK_MAP_ANCHOR_AND_TAG);
        validate_flow_style(&docs)
    },
    "flow mapping",
    vec!["&a", "!mytag"]
)]
// flow_to_block (flow sequence) — diagnostic-driven; fix: clear clone props
#[case::flow_to_block_seq_anchor(
    FLOW_TO_BLOCK_SEQ_ANCHOR,
    Range::new(Position::new(0, 0), Position::new(999, 0)),
    {
        let docs = docs_for(FLOW_TO_BLOCK_SEQ_ANCHOR);
        validate_flow_style(&docs)
    },
    "flow sequence",
    vec!["&anchor"]
)]
#[case::flow_to_block_seq_tag(
    FLOW_TO_BLOCK_SEQ_TAG,
    Range::new(Position::new(0, 0), Position::new(999, 0)),
    {
        let docs = docs_for(FLOW_TO_BLOCK_SEQ_TAG);
        validate_flow_style(&docs)
    },
    "flow sequence",
    vec!["!mytag"]
)]
#[case::flow_to_block_seq_anchor_and_tag(
    FLOW_TO_BLOCK_SEQ_ANCHOR_AND_TAG,
    Range::new(Position::new(0, 0), Position::new(999, 0)),
    {
        let docs = docs_for(FLOW_TO_BLOCK_SEQ_ANCHOR_AND_TAG);
        validate_flow_style(&docs)
    },
    "flow sequence",
    vec!["&a", "!mytag"]
)]
// quoted_bool — cursor on the quoted boolean; fix: clear clone props
#[case::quoted_bool_anchor(
    QUOTED_BOOL_ANCHOR, cursor_range(0, 17), vec![], "Convert quoted", vec!["&anchor"]
)]
#[case::quoted_bool_tag(
    QUOTED_BOOL_TAG, cursor_range(0, 16), vec![], "Convert quoted", vec!["!mytag"]
)]
#[case::quoted_bool_anchor_and_tag(
    QUOTED_BOOL_ANCHOR_AND_TAG, cursor_range(0, 19), vec![], "Convert quoted", vec!["&a", "!mytag"]
)]
// tab_to_spaces — text-only replacement; always preserves properties
#[case::tab_to_spaces_anchor(
    TAB_TO_SPACES_ANCHOR, cursor_range(0, 0), vec![], "tabs to spaces", vec!["&anchor"]
)]
#[case::tab_to_spaces_tag(
    TAB_TO_SPACES_TAG, cursor_range(0, 0), vec![], "tabs to spaces", vec!["!mytag"]
)]
#[case::tab_to_spaces_anchor_and_tag(
    TAB_TO_SPACES_ANCHOR_AND_TAG, cursor_range(0, 0), vec![], "tabs to spaces", vec!["&a", "!mytag"]
)]
// yaml11_bool — diagnostic-driven with exact column range; fix: clear clone props
#[case::yaml11_bool_anchor(
    YAML11_BOOL_ANCHOR,
    line_range(0),
    vec![make_diagnostic(0, 17, 20, "yaml11Boolean")],
    "Quote value",
    vec!["&anchor"]
)]
#[case::yaml11_bool_tag(
    YAML11_BOOL_TAG,
    line_range(0),
    vec![make_diagnostic(0, 16, 19, "yaml11Boolean")],
    "Quote value",
    vec!["!mytag"]
)]
#[case::yaml11_bool_anchor_and_tag(
    YAML11_BOOL_ANCHOR_AND_TAG,
    line_range(0),
    vec![make_diagnostic(0, 19, 22, "yaml11Boolean")],
    "Quote value",
    vec!["&a", "!mytag"]
)]
// yaml11_octal — diagnostic-driven with exact column range; fix: clear clone props
#[case::yaml11_octal_anchor(
    YAML11_OCTAL_ANCHOR,
    line_range(0),
    vec![make_diagnostic(0, 14, 18, "yaml11Octal")],
    "Quote as string",
    vec!["&anchor"]
)]
#[case::yaml11_octal_tag(
    YAML11_OCTAL_TAG,
    line_range(0),
    vec![make_diagnostic(0, 13, 17, "yaml11Octal")],
    "Quote as string",
    vec!["!mytag"]
)]
#[case::yaml11_octal_anchor_and_tag(
    YAML11_OCTAL_ANCHOR_AND_TAG,
    line_range(0),
    vec![make_diagnostic(0, 16, 20, "yaml11Octal")],
    "Quote as string",
    vec!["&a", "!mytag"]
)]
fn property_preservation_invariant(
    #[case] input: &str,
    #[case] range: Range,
    #[case] diagnostics: Vec<Diagnostic>,
    #[case] title_substr: &str,
    #[case] properties: Vec<&str>,
) {
    let uri = test_uri();
    let docs = docs_for(input);
    let actions = code_actions(
        &docs,
        input,
        range,
        &diagnostics,
        &uri,
        &YamlFormatOptions::default(),
    );

    let action = actions
        .iter()
        .find(|a| a.title.contains(title_substr))
        .unwrap_or_else(|| {
            panic!(
                "no action with title containing {title_substr:?} for input {input:?}\navailable: {:?}",
                actions.iter().map(|a| &a.title).collect::<Vec<_>>()
            )
        });

    let edit = action
        .edit
        .as_ref()
        .and_then(|e| e.changes.as_ref())
        .and_then(|c| c.get(&uri))
        .and_then(|edits| edits.first())
        .unwrap_or_else(|| panic!("action {title_substr:?} has no TextEdit"));

    let output = apply_text_edit(input, edit);
    assert_properties_preserved(input, &output, &properties);
}
