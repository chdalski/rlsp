// SPDX-License-Identifier: MIT
//
// The "Delete unused anchor" quick fix keeps the data: a document with the fix
// applied loads to the data of the original without the one anchor, and has one
// unused-anchor diagnostic less.  Where no fix is offered, deleting the token
// the naive way would have changed the document or broken it.  Run over a table
// of inputs and over every case of the yaml-test-suite that loads.

#![expect(missing_docs, reason = "test code")]

mod loaded_data;
mod suite_cases;

use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use rlsp_yaml::editing::code_actions::code_actions;
use rlsp_yaml::editing::formatter::YamlFormatOptions;
use rlsp_yaml::validation::validators::validate_unused_anchors;
use rlsp_yaml_parser::load;
use rstest::rstest;
use suite_cases::load_cases_from_file;
use tower_lsp::lsp_types::{Diagnostic, NumberOrString, Position, Range, TextEdit, Url};

// ---- helpers ----------------------------------------------------------------

/// `leaf` inside `depth` nested flow sequences.
fn nested_flow(depth: usize, leaf: &str) -> String {
    format!("{}{leaf}{}", "[".repeat(depth), "]".repeat(depth))
}

#[expect(clippy::expect_used, reason = "test helper; the URL is a literal")]
fn uri() -> Url {
    Url::parse("file:///test.yaml").expect("valid URI")
}

/// The data of `yaml` without its anchors; `None` when `yaml` does not load.
fn anchor_blind_data(yaml: &str) -> Option<String> {
    static ANCHOR: LazyLock<Regex> = LazyLock::new(|| {
        #[expect(clippy::expect_used, reason = "the pattern is a literal")]
        Regex::new(r#"&(?:Some\("(?:[^"\\]|\\.)*"\)|None)"#).expect("valid regex")
    });
    loaded_data::data_signature(yaml).map(|data| ANCHOR.replace_all(&data, "&_").into_owned())
}

/// Whether `yaml` holds an alias whose name no anchor of the stream defines.
/// The anchor-blind data cannot tell: it keeps the alias and drops the anchor.
fn has_dangling_alias(yaml: &str) -> bool {
    static ALIAS: LazyLock<Regex> = LazyLock::new(|| {
        #[expect(clippy::expect_used, reason = "the pattern is a literal")]
        Regex::new(r"A([^;]*);").expect("valid regex")
    });
    static ANCHOR: LazyLock<Regex> = LazyLock::new(|| {
        #[expect(clippy::expect_used, reason = "the pattern is a literal")]
        Regex::new(r#"&Some\("((?:[^"\\]|\\.)*)"\)"#).expect("valid regex")
    });
    let Some(data) = loaded_data::data_signature(yaml) else {
        return false;
    };
    let anchors: Vec<&str> = ANCHOR
        .captures_iter(&data)
        .filter_map(|c| c.get(1).map(|m| m.as_str()))
        .collect();
    ALIAS
        .captures_iter(&data)
        .filter_map(|c| c.get(1).map(|m| m.as_str()))
        .any(|alias| !anchors.contains(&alias))
}

/// The byte offset of a position of `text` that the features computed: lines
/// end at `\n`, `\r\n` or a lone `\r`, columns count code points.
fn offset_of(text: &str, position: Position) -> usize {
    let bytes = text.as_bytes();
    let (mut at, mut line) = (0, 0);
    while line < position.line {
        match bytes.get(at) {
            Some(b'\r') if bytes.get(at + 1) == Some(&b'\n') => at += 2,
            Some(b'\r' | b'\n') => at += 1,
            Some(_) => {
                at += 1;
                continue;
            }
            None => break,
        }
        line += 1;
    }
    let skipped = position.character as usize;
    text.get(at..).map_or(text.len(), |rest| {
        rest.char_indices()
            .nth(skipped)
            .map_or(text.len(), |(offset, _)| at + offset)
    })
}

fn apply(text: &str, edit: &TextEdit) -> String {
    let (start, end) = (
        offset_of(text, edit.range.start),
        offset_of(text, edit.range.end),
    );
    format!("{}{}{}", &text[..start], edit.new_text, &text[end..])
}

/// The unused-anchor diagnostics of `text`; empty when `text` does not load.
fn unused_anchors(text: &str) -> Vec<Diagnostic> {
    let docs = load(text).unwrap_or_default();
    validate_unused_anchors(&docs)
        .into_iter()
        .filter(|d| {
            d.code
                == Some(tower_lsp::lsp_types::NumberOrString::String(
                    "unusedAnchor".into(),
                ))
        })
        .collect()
}

/// The offered fix for `diag` applied to `text`.
fn applied_fix(text: &str, diag: &Diagnostic) -> Option<String> {
    let docs = load(text).unwrap_or_default();
    let whole = Range::new(Position::new(0, 0), Position::new(u32::MAX, 0));
    let actions = code_actions(
        &docs,
        text,
        whole,
        std::slice::from_ref(diag),
        &uri(),
        &YamlFormatOptions::default(),
    );
    let action = actions.iter().find(|a| a.title == "Delete unused anchor")?;
    let edits = &action.edit.as_ref()?.changes.as_ref()?[&uri()];
    assert_eq!(edits.len(), 1, "one edit: {action:?}");
    Some(apply(text, edits.first()?))
}

/// `text` with the token of `diag` and the blanks after it deleted, whatever the
/// document looks like afterwards.
fn naive_deletion(text: &str, diag: &Diagnostic) -> String {
    let start = offset_of(text, diag.range.start);
    let end = offset_of(text, diag.range.end);
    let blanks = text[end..].len() - text[end..].trim_start_matches([' ', '\t']).len();
    format!("{}{}", &text[..start], &text[end + blanks..])
}

/// Check the fix of every unused anchor of `text` (which must load).
fn check_fixes(text: &str) -> Result<(), String> {
    let Some(data) = anchor_blind_data(text) else {
        return Ok(());
    };
    let diagnostics = unused_anchors(text);
    for diag in &diagnostics {
        let token = &text[offset_of(text, diag.range.start)..offset_of(text, diag.range.end)];
        if !token.starts_with('&') {
            return Err(format!("{text:?}: the range of {diag:?} is not a token"));
        }
        if let Some(applied) = applied_fix(text, diag) {
            if anchor_blind_data(&applied).as_deref() != Some(data.as_str()) {
                return Err(format!(
                    "{text:?}: the fix for {token} changes the data: {applied:?}"
                ));
            }
            if has_dangling_alias(&applied) && !has_dangling_alias(text) {
                return Err(format!(
                    "{text:?}: the fix for {token} leaves an alias without its anchor: {applied:?}"
                ));
            }
            if unused_anchors(&applied).len() + 1 != diagnostics.len() {
                return Err(format!(
                    "{text:?}: the fix for {token} does not remove one diagnostic: {applied:?}"
                ));
            }
        } else {
            let naive = naive_deletion(text, diag);
            if anchor_blind_data(&naive).as_deref() == Some(data.as_str())
                && !has_dangling_alias(&naive)
            {
                return Err(format!(
                    "{text:?}: no fix for {token}, but deleting it is safe: {naive:?}"
                ));
            }
        }
    }
    Ok(())
}

// ---- tests --------------------------------------------------------------------

#[rstest]
#[case::scalar("key: &a value  # keep me\n")]
#[case::mapping("base: &defaults\n  x: 1\n  y: 2\n")]
#[case::sequence("list: &s\n  - 1\n")]
#[case::empty_value("a: &x\nb: 1\n")]
#[case::empty_sequence_item("- &y !t\n- x\n")]
#[case::literal_block_scalar("a: &x |\n  line one\nb: 1\n")]
#[case::block_scalar_header_comment("a: &x | # c\n  t\nb: 1\n")]
#[case::alone_on_the_line("&x\nk: v")]
#[case::alone_before_a_comment("&x # note\nk: v")]
#[case::bare_empty_document("&x\n")]
#[case::bare_empty_document_with_comment("&x # c\n")]
#[case::bare_empty_document_before_end("&x\n...\n")]
#[case::bare_empty_document_before_another("&x\n---\nk: v\n")]
#[case::bare_empty_document_before_another_crlf("&x\r\n---\r\nk: v\r\n")]
#[case::explicit_empty_document("---\n&x\n---\nk: v\n")]
#[case::empty_sequence_item_on_its_own_line("-\n  &x\n- b\n")]
#[case::byte_order_mark("\u{FEFF}&x\nk: v\n")]
#[case::after_document_start("--- &x\nk: v\n")]
#[case::explicit_key("? &k\n: v\n")]
#[case::key_and_value("&k key: &v val\nl: &s\n  - &t 1\n")]
#[case::flow_mapping_value("{k: &a}")]
#[case::flow_mapping_value_then_entry("{k: &a, j: 1}")]
#[case::empty_flow_sequence_item("[&a, b]")]
#[case::empty_last_flow_sequence_item("[b, &a]")]
#[case::empty_flow_mapping_key("{&a, b}")]
#[case::empty_flow_key_with_value("{&a : v}")]
#[case::empty_flow_pair_key("[&a : v]")]
#[case::multi_line_flow("[\n  &a,\n  b\n]")]
#[case::crlf("k: &a\r\n  x: 1\r\n")]
#[case::lone_cr("k: &a\r  x: 1\r")]
#[case::documents("a: &x 1\n---\n名前: 2\n---\nc: &z 3\n")]
#[case::astral_key("😀: &a v # 😀\n")]
#[case::astral_keys("😀 😀: &a\n  x: 1\n")]
#[case::astral_line_before("😀: 1\nk: &a v\n")]
#[case::astral_alone_on_the_line("# 😀\nk:\n  &x\n  😀: 1\n")]
#[case::astral_flow("{😀: &a v, 😀😀: &b w}")]
#[case::astral_crlf("😀: 1\r\n😀: &a v\r\n")]
#[case::astral_lone_cr("😀: 1\r😀: &a v\r")]
#[case::astral_in_a_flow_sequence_before("[😀, &a b]")]
#[case::astral_after_a_byte_order_mark("\u{FEFF}😀: &a v\n")]
#[case::used_and_unused("a: &used 1\nb: &unused 2\nc: *used\n")]
#[case::same_name_twice("a: &x 1\nb: &x 2\n")]
fn the_fix_keeps_the_data(#[case] text: &str) {
    assert!(
        !unused_anchors(text).is_empty(),
        "{text:?} has an unused anchor"
    );
    if let Err(message) = check_fixes(text) {
        panic!("{message}");
    }
}

/// An anchor with an alias at any depth is not unused: no diagnostic, so no fix,
/// and the diagnostic of a stale client is refused. The refusal is right:
/// deleting the token the naive way leaves the alias without an anchor.
#[rstest]
fn an_anchor_with_a_deeply_nested_alias_is_not_reported_and_not_fixed(
    #[values(99, 100, 101, 255)] depth: usize,
) {
    let text = format!("a: &x 1\nb: {}\n", nested_flow(depth, "*x"));
    assert!(load(&text).is_ok());
    assert!(unused_anchors(&text).is_empty());

    let stale = Diagnostic {
        range: Range::new(Position::new(0, 3), Position::new(0, 5)),
        code: Some(NumberOrString::String("unusedAnchor".into())),
        ..Diagnostic::default()
    };
    assert_eq!(applied_fix(&text, &stale), None);
    assert!(has_dangling_alias(&naive_deletion(&text, &stale)));
}

/// The fix of an anchor deep in the document keeps the data.
#[rstest]
fn the_fix_keeps_the_data_of_a_deeply_nested_anchor(#[values(99, 100, 101, 255)] depth: usize) {
    let text = format!("a: {}\n", nested_flow(depth, "&x 1"));
    assert!(load(&text).is_ok());
    assert_eq!(unused_anchors(&text).len(), 1);
    if let Err(message) = check_fixes(&text) {
        panic!("{message}");
    }
}

/// A deep unused anchor is fixed, a flat anchor with a deep alias is kept.
#[test]
fn the_fix_tells_a_deep_unused_anchor_from_a_deep_alias() {
    let text = format!(
        "a: {}\nb: &y 2\nc: {}\n",
        nested_flow(101, "&x 1"),
        nested_flow(101, "*y")
    );
    assert!(load(&text).is_ok());
    assert_eq!(unused_anchors(&text).len(), 1);
    if let Err(message) = check_fixes(&text) {
        panic!("{message}");
    }
}

/// Fixing the first unused anchor again and again ends without an unused anchor
/// and keeps the data.
#[rstest]
#[case::mixed("&k key: &v val\nl: &s\n  - &t 1\n")]
#[case::documents("a: &x 1\n---\nb: &y\n  c: &z 3\n")]
fn applying_fixes_one_after_the_other_ends_without_unused_anchors(#[case] text: &str) {
    let data = anchor_blind_data(text).expect("the text loads");
    let mut current = text.to_owned();
    while let Some(diag) = unused_anchors(&current).first() {
        current = applied_fix(&current, diag).expect("a fix is offered");
        assert_eq!(
            anchor_blind_data(&current),
            Some(data.clone()),
            "{current:?}"
        );
    }
    assert!(!current.contains('&'), "{current:?}");
}

/// The fix of every unused anchor of every suite case that loads keeps the data;
/// where no fix is offered, deleting the token would not have.
#[rstest]
#[timeout(Duration::from_secs(5))]
fn the_fix_keeps_the_data_of_suite_case(
    #[files("../tests/yaml-test-suite/src/*.yaml")] path: PathBuf,
) {
    for case in load_cases_from_file(&path).iter().filter(|case| !case.fail) {
        if let Err(message) = check_fixes(&case.yaml) {
            panic!("{}[{}] {}: {message}", case.file, case.index, case.name);
        }
    }
}
