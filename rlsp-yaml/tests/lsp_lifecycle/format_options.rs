// SPDX-License-Identifier: MIT
//
// Pins how the server turns the settings, the `.editorconfig`, and the
// request's tab size into formatter options, through the three handlers that
// build them: `formatting`, `range_formatting`, and `code_action`. Every case
// is paired with the output of the default options, so a case fails when the
// option it names stops reaching the formatter.
use std::fs;

use futures::StreamExt;
use rlsp_yaml::server::Backend;
use rstest::rstest;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower_lsp::LspService;
use tower_lsp::lsp_types::Url;

use super::code_actions::code_action_request;
use super::configuration::did_change_configuration_notification;
use super::formatting_integration::{
    formatting_request_with_tab_size, range_formatting_request_with_tab_size,
};
use super::helpers::*;

#[derive(Clone, Copy, Debug)]
enum Handler {
    Formatting,
    RangeFormatting,
}

const BOTH_HANDLERS: [Handler; 2] = [Handler::Formatting, Handler::RangeFormatting];

const RANGE_END_LINE: u32 = 100;

/// A running server that has `settings` applied and `text` open at `uri`.
async fn open_document(settings: &Value, uri: &str, text: &str) -> LspService<Backend> {
    let (mut service, socket) = LspService::new(Backend::new);
    tokio::spawn(socket.for_each(|_| async {}));

    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;
    send(
        &mut service,
        did_change_configuration_notification(settings),
    )
    .await;
    send(&mut service, did_open_notification(uri, text)).await;
    service
}

/// The whole document as `handler` formats it: `text` with the handler's edit
/// applied.
async fn format_with(
    handler: Handler,
    settings: &Value,
    uri: &str,
    text: &str,
    tab_size: u32,
) -> String {
    let mut service = open_document(settings, uri, text).await;
    let request = match handler {
        Handler::Formatting => formatting_request_with_tab_size(2, uri, tab_size),
        // The range ends far below the document, so it touches whatever
        // formatting changes.
        Handler::RangeFormatting => {
            range_formatting_request_with_tab_size(2, uri, 0, RANGE_END_LINE, tab_size)
        }
    };
    let resp = send(&mut service, request)
        .await
        .expect("handler should return a response");
    apply_edit(text, resp.result().expect("handler should have a result"))
}

/// A file in a fresh directory that holds `editorconfig`.
struct EditorConfigFile {
    _dir: TempDir,
    uri: String,
}

fn file_with_editorconfig(editorconfig: &str) -> EditorConfigFile {
    let dir = TempDir::new().expect("temp dir");
    fs::write(
        dir.path().join(".editorconfig"),
        format!("root = true\n\n[*.yaml]\n{editorconfig}\n"),
    )
    .expect("write .editorconfig");
    let file = dir.path().join("file.yaml");
    fs::write(&file, "").expect("write yaml file");
    let uri = Url::from_file_path(&file)
        .expect("valid file path")
        .to_string();
    EditorConfigFile { _dir: dir, uri }
}

/// The edit of the code action whose title contains `title_fragment`, asked for
/// from the first line that is not a comment, so a modeline does not hide the
/// key line.
async fn code_action_edit(
    settings: &Value,
    uri: &str,
    text: &str,
    title_fragment: &str,
) -> Option<String> {
    let mut service = open_document(settings, uri, text).await;
    let start_line = text
        .lines()
        .position(|line| !line.starts_with('#'))
        .unwrap_or(0);
    let end_line = text.lines().count();
    let resp = send(
        &mut service,
        code_action_request(
            2,
            uri,
            u32::try_from(start_line).expect("line number fits"),
            u32::try_from(end_line).expect("line number fits"),
        ),
    )
    .await
    .expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have a result");
    result
        .as_array()?
        .iter()
        .find(|action| {
            action["title"]
                .as_str()
                .is_some_and(|title| title.contains(title_fragment))
        })
        .map(|action| {
            action
                .get("edit")
                .and_then(|edit| edit.get("changes"))
                .and_then(|changes| changes.get(uri))
                .and_then(|edits| edits.get(0))
                .and_then(|edit| edit.get("newText"))
                .and_then(Value::as_str)
                .expect("the action should carry a text edit")
                .to_string()
        })
}

const WIDE_FLOW_SEQUENCE: &str = "k: [aaaa, bbbb, cccc, dddd, eeee]\n";
const BLOCK_SEQUENCE: &str = "items:\n  - aaaa\n  - bbbb\n  - cccc\n  - dddd\n  - eeee\n";

// ---- settings reach the formatter through `formatting` and `range_formatting` ----

#[rstest]
#[case::tab_size_of_the_request(json!({}), 4, "a:\n  b:\n    c: 1\n", "a:\n    b:\n        c: 1\n", "a:\n  b:\n    c: 1\n")]
#[case::print_width(json!({ "formatPrintWidth": 20 }), 2, WIDE_FLOW_SEQUENCE, "k: [\n  aaaa,\n  bbbb,\n  cccc,\n  dddd,\n  eeee\n  ]\n", WIDE_FLOW_SEQUENCE)]
#[case::single_quote(json!({ "formatSingleQuote": true }), 2, "k: [\"x, y\"]\n", "k: ['x, y']\n", "k: [\"x, y\"]\n")]
#[case::preserve_quotes(json!({ "formatPreserveQuotes": true }), 2, "a: \"abc\"\n", "a: \"abc\"\n", "a: abc\n")]
#[case::bracket_spacing_off(json!({ "formatBracketSpacing": false }), 2, "a: {x: 1}\n", "a: {x: 1}\n", "a: { x: 1 }\n")]
#[case::enforce_block_style(json!({ "formatEnforceBlockStyle": true }), 2, "a: {x: 1}\n", "a:\n  x: 1\n", "a: { x: 1 }\n")]
#[case::remove_duplicate_keys(json!({ "formatRemoveDuplicateKeys": true }), 2, "a: 1\na: 2\n", "a: 2\n", "a: 1\na: 2\n")]
#[case::indent_sequences_off(json!({ "formatIndentSequences": false }), 2, "a:\n  - 1\n", "a:\n- 1\n", "a:\n  - 1\n")]
#[case::yaml_version_1_1(json!({ "yamlVersion": "1.1" }), 2, "v: \"yes\"\n", "v: \"yes\"\n", "v: yes\n")]
#[case::max_nesting_depth(json!({ "maxNestingDepth": 2 }), 2, "a:\n  b:\n    c:   1\n", "a:\n  b:\n    c:   1\n", "a:\n  b:\n    c: 1\n")]
#[tokio::test]
async fn setting_reaches_the_formatter(
    #[values(Handler::Formatting, Handler::RangeFormatting)] handler: Handler,
    #[case] settings: Value,
    #[case] tab_size: u32,
    #[case] input: &str,
    #[case] expected: &str,
    #[case] default_expected: &str,
) {
    let uri = "file:///test/format-options.yaml";

    let with_setting = format_with(handler, &settings, uri, input, tab_size).await;
    let with_defaults = format_with(handler, &json!({}), uri, input, 2).await;

    assert_eq!(with_setting, expected, "{handler:?} with the setting");
    assert_eq!(with_defaults, default_expected, "{handler:?} with defaults");
}

// ---- `.editorconfig` reaches the formatter through `formatting` and `range_formatting` ----

#[tokio::test]
async fn editorconfig_max_line_length_is_the_print_width_without_a_setting() {
    for handler in BOTH_HANDLERS {
        let file = file_with_editorconfig("max_line_length = 20");

        let formatted = format_with(handler, &json!({}), &file.uri, WIDE_FLOW_SEQUENCE, 2).await;

        assert_ne!(formatted, WIDE_FLOW_SEQUENCE, "{handler:?}");
        assert!(formatted.lines().count() > 1, "{handler:?}: {formatted:?}");
    }
}

#[tokio::test]
async fn print_width_setting_wins_over_editorconfig_max_line_length() {
    for handler in BOTH_HANDLERS {
        let file = file_with_editorconfig("max_line_length = 20");
        let settings = json!({ "formatPrintWidth": 120 });

        let formatted = format_with(handler, &settings, &file.uri, WIDE_FLOW_SEQUENCE, 2).await;

        assert_eq!(formatted, WIDE_FLOW_SEQUENCE, "{handler:?}");
    }
}

#[tokio::test]
async fn disabled_respect_editorconfig_ignores_editorconfig_max_line_length() {
    for handler in BOTH_HANDLERS {
        let file = file_with_editorconfig("max_line_length = 20");
        let settings = json!({ "formatRespectEditorconfig": false });

        let formatted = format_with(handler, &settings, &file.uri, WIDE_FLOW_SEQUENCE, 2).await;

        assert_eq!(formatted, WIDE_FLOW_SEQUENCE, "{handler:?}");
    }
}

#[tokio::test]
async fn editorconfig_end_of_line_crlf_ends_every_line_with_crlf() {
    for handler in BOTH_HANDLERS {
        let file = file_with_editorconfig("end_of_line = crlf");

        let formatted = format_with(handler, &json!({}), &file.uri, "a:  1\nb: 2\n", 2).await;

        assert_eq!(formatted, "a: 1\r\nb: 2\r\n", "{handler:?}");
    }
}

#[tokio::test]
async fn editorconfig_insert_final_newline_false_leaves_the_last_line_open() {
    for handler in BOTH_HANDLERS {
        let file = file_with_editorconfig("insert_final_newline = false");

        let formatted = format_with(handler, &json!({}), &file.uri, "a:  1\nb: 2\n", 2).await;

        assert_eq!(formatted, "a: 1\nb: 2", "{handler:?}");
    }
}

#[tokio::test]
async fn disabled_respect_editorconfig_ignores_end_of_line_and_final_newline() {
    for handler in BOTH_HANDLERS {
        let file = file_with_editorconfig("end_of_line = crlf\ninsert_final_newline = false");
        let settings = json!({ "formatRespectEditorconfig": false });

        let formatted = format_with(handler, &settings, &file.uri, "a:  1\nb: 2\n", 2).await;

        assert_eq!(formatted, "a: 1\nb: 2\n", "{handler:?}");
    }
}

// ---- the edits the line-break handling produces, applied to the document ----

/// The byte offset of an LSP position in `text`, found by an own scan for the
/// breaks `\r\n`, `\r`, and `\n`, so the tests do not lean on the code they
/// check. The character counts UTF-16 units, and one past the end of its line
/// stands for that end.
fn offset_of(text: &str, position: &Value) -> usize {
    let line = position["line"].as_u64().expect("line");
    let character = usize::try_from(position["character"].as_u64().expect("character"))
        .expect("character fits");
    let mut start = 0;
    for _ in 0..line {
        let Some(found) = text[start..].find(['\r', '\n']) else {
            return text.len();
        };
        start += found;
        start += if text[start..].starts_with("\r\n") {
            2
        } else {
            1
        };
    }
    let mut offset = start;
    let mut units = 0;
    for ch in text[start..].chars() {
        if ch == '\r' || ch == '\n' || units >= character {
            break;
        }
        units += ch.len_utf16();
        offset += ch.len_utf8();
    }
    offset
}

/// `text` with the one edit of `result` applied, or `text` itself when
/// `result` is null.
fn apply_edit(text: &str, result: &Value) -> String {
    let Some(edits) = result.as_array() else {
        assert!(result.is_null(), "an edit list or null: {result}");
        return text.to_string();
    };
    assert_eq!(edits.len(), 1, "one edit: {result}");
    let edit = edits.first().expect("one edit");
    let position = |pointer: &str| edit.pointer(pointer).expect("a range position");
    let start = offset_of(text, position("/range/start"));
    let end = offset_of(text, position("/range/end"));
    assert!(start <= end, "the range runs forward: {result}");
    format!(
        "{}{}{}",
        text.get(..start).expect("a character boundary"),
        edit.pointer("/newText")
            .and_then(Value::as_str)
            .expect("newText"),
        text.get(end..).expect("a character boundary")
    )
}

/// What `handler` answers for `text` open at `uri`; the lines matter to range
/// formatting only.
async fn answer(
    handler: Handler,
    settings: &Value,
    uri: &str,
    text: &str,
    first_line: u32,
    last_line: u32,
) -> Value {
    let mut service = open_document(settings, uri, text).await;
    let request = match handler {
        Handler::Formatting => formatting_request_with_tab_size(2, uri, 2),
        Handler::RangeFormatting => {
            range_formatting_request_with_tab_size(2, uri, first_line, last_line, 2)
        }
    };
    let resp = send(&mut service, request)
        .await
        .expect("handler should return a response");
    resp.result().expect("handler should have a result").clone()
}

/// A URI no `.editorconfig` applies to.
const PLAIN_URI: &str = "file:///test/line-breaks.yaml";

#[rstest]
#[case::lf_with_final_newline("lf", "a:  1\nb: 2\n", "a: 1\nb: 2\n")]
#[case::lf_without_final_newline("lf\ninsert_final_newline = false", "a:  1\nb: 2\n", "a: 1\nb: 2")]
#[case::crlf_with_final_newline("crlf", "a:  1\nb: 2\n", "a: 1\r\nb: 2\r\n")]
#[case::crlf_without_final_newline(
    "crlf\ninsert_final_newline = false",
    "a:  1\nb: 2\n",
    "a: 1\r\nb: 2"
)]
#[case::cr_with_final_newline("cr", "a:  1\nb: 2\n", "a: 1\rb: 2\r")]
#[case::cr_without_final_newline("cr\ninsert_final_newline = false", "a:  1\nb: 2\n", "a: 1\rb: 2")]
#[case::unterminated_input_gets_the_final_newline("lf", "a:  1\nb: 2", "a: 1\nb: 2\n")]
#[case::unterminated_input_crlf_without_final_newline(
    "crlf\ninsert_final_newline = false",
    "a:  1\nb: 2",
    "a: 1\r\nb: 2"
)]
#[tokio::test]
async fn range_and_full_formatting_leave_the_same_document(
    #[case] editorconfig: &str,
    #[case] text: &str,
    #[case] expected: &str,
) {
    let file = file_with_editorconfig(&format!("end_of_line = {editorconfig}"));

    for handler in BOTH_HANDLERS {
        let result = answer(handler, &json!({}), &file.uri, text, 0, RANGE_END_LINE).await;

        assert_eq!(apply_edit(text, &result), expected, "{handler:?}");
    }
}

#[tokio::test]
async fn range_formatting_without_end_of_line_writes_lf_into_a_crlf_document() {
    let text = "a:  1\r\nb: 2\r\nc: 3\r\n";

    let full = answer(Handler::Formatting, &json!({}), PLAIN_URI, text, 0, 0).await;

    // Every line differs from its formatted self in its break, so a range on
    // any one line formats the whole text.
    for line in 0..3 {
        let result = answer(
            Handler::RangeFormatting,
            &json!({}),
            PLAIN_URI,
            text,
            line,
            line,
        )
        .await;

        assert_eq!(result[0]["newText"], "a: 1\nb: 2\nc: 3\n", "line {line}");
        assert_eq!(
            apply_edit(text, &result),
            "a: 1\nb: 2\nc: 3\n",
            "line {line}"
        );
    }
    assert_eq!(apply_edit(text, &full), "a: 1\nb: 2\nc: 3\n");
}

#[tokio::test]
async fn a_range_below_the_end_of_a_crlf_document_gets_no_edit() {
    let text = "a:  1\r\nb: 2\r\n";

    let result = answer(Handler::RangeFormatting, &json!({}), PLAIN_URI, text, 7, 7).await;

    assert!(result.is_null(), "{result}");
}

#[tokio::test]
async fn a_range_over_lines_that_differ_only_in_their_breaks_gets_an_edit() {
    let file = file_with_editorconfig("end_of_line = crlf");
    let text = "a: 1\nb: 2\n";

    let result = answer(Handler::RangeFormatting, &json!({}), &file.uri, text, 0, 0).await;

    assert_eq!(result[0]["newText"], "a: 1\r\nb: 2\r\n");
    assert_eq!(apply_edit(text, &result), "a: 1\r\nb: 2\r\n");
}

#[tokio::test]
async fn a_range_whose_lines_are_formatted_gets_no_edit() {
    let file = file_with_editorconfig("end_of_line = crlf");
    let text = "a: 1\r\nb:  2\r\n";

    let result = answer(Handler::RangeFormatting, &json!({}), &file.uri, text, 0, 0).await;

    assert!(result.is_null(), "{result}");
}

#[tokio::test]
async fn a_lone_cr_document_has_a_second_line_to_range_format() {
    let file = file_with_editorconfig("end_of_line = cr");
    let text = "a: 1\rb:  2\rc: 3\r";

    let touching = answer(Handler::RangeFormatting, &json!({}), &file.uri, text, 1, 1).await;
    let above = answer(Handler::RangeFormatting, &json!({}), &file.uri, text, 0, 0).await;
    let below = answer(Handler::RangeFormatting, &json!({}), &file.uri, text, 2, 2).await;

    assert_eq!(
        touching[0]["range"]["start"],
        json!({"line": 1, "character": 0})
    );
    assert_eq!(
        touching[0]["range"]["end"],
        json!({"line": 2, "character": 0})
    );
    assert_eq!(touching[0]["newText"], "b: 2\r");
    assert_eq!(apply_edit(text, &touching), "a: 1\rb: 2\rc: 3\r");
    assert!(above.is_null(), "{above}");
    assert!(below.is_null(), "{below}");
}

#[tokio::test]
async fn a_lone_cr_document_without_end_of_line_is_formatted_whole_by_any_range() {
    let text = "a: 1\rb:  2\rc: 3\r";

    let result = answer(Handler::RangeFormatting, &json!({}), PLAIN_URI, text, 1, 1).await;

    assert_eq!(
        result[0]["range"]["start"],
        json!({"line": 0, "character": 0})
    );
    assert_eq!(apply_edit(text, &result), "a: 1\nb: 2\nc: 3\n");
}

#[rstest]
#[case::terminated("a:  1\rb: 2\rc: 3\r", "a: 1\nb: 2\nc: 3\n", 3)]
#[case::unterminated("a:  1\rb: 2", "a: 1\nb: 2\n", 1)]
#[tokio::test]
async fn full_formatting_replaces_the_whole_lone_cr_document(
    #[case] text: &str,
    #[case] expected: &str,
    #[case] end_line: u64,
) {
    let result = answer(Handler::Formatting, &json!({}), PLAIN_URI, text, 0, 0).await;

    assert_eq!(
        result[0]["range"]["start"],
        json!({"line": 0, "character": 0})
    );
    assert_eq!(result[0]["range"]["end"]["line"], end_line);
    assert_eq!(apply_edit(text, &result), expected);
}

#[tokio::test]
async fn a_range_in_mixed_breaks_replaces_only_the_lines_formatting_changes() {
    let text = "a: 1\r\nb: 2\nc: 3\n";

    let first = answer(Handler::RangeFormatting, &json!({}), PLAIN_URI, text, 0, 0).await;
    let second = answer(Handler::RangeFormatting, &json!({}), PLAIN_URI, text, 1, 1).await;
    let third = answer(Handler::RangeFormatting, &json!({}), PLAIN_URI, text, 2, 2).await;

    assert_eq!(first[0]["newText"], "a: 1\n");
    assert_eq!(apply_edit(text, &first), "a: 1\nb: 2\nc: 3\n");
    assert!(second.is_null(), "{second}");
    assert!(third.is_null(), "{third}");
}

#[tokio::test]
async fn a_range_across_removed_blank_lines_keeps_every_key_once() {
    let text = "a: 1\n\n\n\nb: 2\n";
    let full = answer(Handler::Formatting, &json!({}), PLAIN_URI, text, 0, 0).await;
    let formatted = apply_edit(text, &full);

    let result = answer(Handler::RangeFormatting, &json!({}), PLAIN_URI, text, 0, 3).await;

    assert_eq!(apply_edit(text, &result), formatted);
    assert_eq!(formatted.matches("a:").count(), 1, "{formatted:?}");
    assert_eq!(formatted.matches("b:").count(), 1, "{formatted:?}");
}

#[rstest]
#[case::formatting(Handler::Formatting)]
#[case::range_formatting(Handler::RangeFormatting)]
#[tokio::test]
async fn an_open_last_line_with_an_astral_character_ends_at_its_utf16_length(
    #[case] handler: Handler,
) {
    let text = "a:  1\nk:  \u{1f600}";

    let result = answer(handler, &json!({}), PLAIN_URI, text, 1, 1).await;

    assert_eq!(
        result[0]["range"]["end"],
        json!({"line": 1, "character": 6})
    );
    assert_eq!(apply_edit(text, &result), "a: 1\nk: \u{1f600}\n");
}

#[rstest]
#[case::lf("lf", "a: 1\nb: 2\n")]
#[case::without_final_newline("lf\ninsert_final_newline = false", "a: 1\nb: 2")]
#[tokio::test]
async fn range_formatting_an_unterminated_last_line_ends_at_the_document_end(
    #[case] editorconfig: &str,
    #[case] expected: &str,
) {
    let file = file_with_editorconfig(&format!("end_of_line = {editorconfig}"));
    let text = "a: 1\nb:  2";

    let result = answer(Handler::RangeFormatting, &json!({}), &file.uri, text, 1, 1).await;

    assert_eq!(
        result[0]["range"]["start"],
        json!({"line": 1, "character": 0})
    );
    assert_eq!(result[0]["range"]["end"]["line"], 1);
    assert_eq!(apply_edit(text, &result), expected);
}

#[tokio::test]
async fn range_formatting_with_a_range_outside_the_document_is_safe() {
    let text = "a:  1\nb: 2\n";

    let past_the_end = answer(
        Handler::RangeFormatting,
        &json!({}),
        PLAIN_URI,
        text,
        7,
        u32::MAX,
    )
    .await;
    let backwards = answer(Handler::RangeFormatting, &json!({}), PLAIN_URI, text, 1, 0).await;
    let to_the_maximum = answer(
        Handler::RangeFormatting,
        &json!({}),
        PLAIN_URI,
        text,
        0,
        u32::MAX,
    )
    .await;

    assert!(past_the_end.is_null(), "{past_the_end}");
    assert!(backwards.is_null(), "{backwards}");
    assert_eq!(apply_edit(text, &to_the_maximum), "a: 1\nb: 2\n");
}

// ---- the options `code_action` builds ----

// The print_width case also pins CODE_ACTION_TAB_WIDTH: the wrapped lines are
// indented by it.
#[rstest]
#[case::print_width(json!({ "formatPrintWidth": 20 }), BLOCK_SEQUENCE, "block to flow", Some(" [\n    aaaa,\n    bbbb,\n    cccc,\n    dddd,\n    eeee\n    ]\n"), Some(" [aaaa, bbbb, cccc, dddd, eeee]\n"))]
#[case::bracket_spacing_off(json!({ "formatBracketSpacing": false }), "m:\n  a: 1\n  b: 2\n", "block to flow", Some(" {a: 1, b: 2}\n"), Some(" { a: 1, b: 2 }\n"))]
#[case::single_quote(json!({ "formatSingleQuote": true }), "items:\n  - \"x, y\"\n  - 1\n", "block to flow", Some(" ['x, y', 1]\n"), Some(" [\"x, y\", 1]\n"))]
#[case::preserve_quotes(json!({ "formatPreserveQuotes": true }), "items:\n  - \"abc\"\n  - d\n", "block to flow", Some(" [\"abc\", d]\n"), Some(" [abc, d]\n"))]
#[case::yaml_version_1_1(json!({ "yamlVersion": "1.1" }), "items:\n  - \"yes\"\n  - d\n", "block to flow", Some(" [\"yes\", d]\n"), Some(" [yes, d]\n"))]
#[case::yaml_version_modeline_over_setting(json!({ "yamlVersion": "1.1" }), "# yaml-language-server: $yamlVersion=1.2\nitems:\n  - \"yes\"\n  - d\n", "block to flow", Some(" [yes, d]\n"), Some(" [yes, d]\n"))]
#[case::enforce_block_style(json!({ "formatEnforceBlockStyle": true }), BLOCK_SEQUENCE, "block to flow", None, Some(" [aaaa, bbbb, cccc, dddd, eeee]\n"))]
#[case::max_nesting_depth(json!({ "maxNestingDepth": 1 }), BLOCK_SEQUENCE, "block to flow", None, Some(" [aaaa, bbbb, cccc, dddd, eeee]\n"))]
#[tokio::test]
async fn code_action_follows_the_settings(
    #[case] settings: Value,
    #[case] input: &str,
    #[case] title_fragment: &str,
    #[case] expected: Option<&str>,
    #[case] default_expected: Option<&str>,
) {
    let uri = "file:///test/code-action-options.yaml";

    let with_setting = code_action_edit(&settings, uri, input, title_fragment).await;
    let with_defaults = code_action_edit(&json!({}), uri, input, title_fragment).await;

    assert_eq!(with_setting.as_deref(), expected, "with the setting");
    assert_eq!(with_defaults.as_deref(), default_expected, "with defaults");
}

#[tokio::test]
async fn code_action_uses_editorconfig_max_line_length_without_a_setting() {
    let file = file_with_editorconfig("max_line_length = 20");

    let edit = code_action_edit(&json!({}), &file.uri, BLOCK_SEQUENCE, "block to flow").await;

    assert_eq!(
        edit.as_deref(),
        Some(" [\n    aaaa,\n    bbbb,\n    cccc,\n    dddd,\n    eeee\n    ]\n")
    );
}

#[tokio::test]
async fn code_action_print_width_setting_wins_over_editorconfig() {
    let file = file_with_editorconfig("max_line_length = 20");
    let settings = json!({ "formatPrintWidth": 120 });

    let edit = code_action_edit(&settings, &file.uri, BLOCK_SEQUENCE, "block to flow").await;

    assert_eq!(edit.as_deref(), Some(" [aaaa, bbbb, cccc, dddd, eeee]\n"));
}

#[tokio::test]
async fn code_action_ignores_editorconfig_when_respect_is_disabled() {
    let file = file_with_editorconfig("max_line_length = 20");
    let settings = json!({ "formatRespectEditorconfig": false });

    let edit = code_action_edit(&settings, &file.uri, BLOCK_SEQUENCE, "block to flow").await;

    assert_eq!(edit.as_deref(), Some(" [aaaa, bbbb, cccc, dddd, eeee]\n"));
}
