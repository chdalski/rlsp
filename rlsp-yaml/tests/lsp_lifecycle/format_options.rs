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

/// The text a handler's first edit puts in place, or `original` when the
/// handler returns no edit.
fn edited_text(result: &Value, original: &str) -> String {
    result
        .as_array()
        .and_then(|edits| edits.first())
        .map_or_else(
            || original.to_string(),
            |edit| {
                edit["newText"]
                    .as_str()
                    .expect("newText should be a string")
                    .to_string()
            },
        )
}

/// The whole document as `handler` formats it.
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
        // The range ends far below the document: a formatted result longer than
        // the original is cut at the requested end line.
        Handler::RangeFormatting => {
            range_formatting_request_with_tab_size(2, uri, 0, RANGE_END_LINE, tab_size)
        }
    };
    let resp = send(&mut service, request)
        .await
        .expect("handler should return a response");
    edited_text(resp.result().expect("handler should have a result"), text)
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
    let file = file_with_editorconfig("end_of_line = crlf");

    let formatted = format_with(
        Handler::Formatting,
        &json!({}),
        &file.uri,
        "a:  1\nb: 2\n",
        2,
    )
    .await;

    assert_eq!(formatted, "a: 1\r\nb: 2\r\n");
}

#[tokio::test]
async fn editorconfig_insert_final_newline_false_leaves_the_last_line_open() {
    let file = file_with_editorconfig("insert_final_newline = false");

    let formatted = format_with(
        Handler::Formatting,
        &json!({}),
        &file.uri,
        "a:  1\nb: 2\n",
        2,
    )
    .await;

    assert_eq!(formatted, "a: 1\nb: 2");
}

#[tokio::test]
async fn disabled_respect_editorconfig_ignores_end_of_line_and_final_newline() {
    let file = file_with_editorconfig("end_of_line = crlf\ninsert_final_newline = false");
    let settings = json!({ "formatRespectEditorconfig": false });

    let formatted = format_with(
        Handler::Formatting,
        &settings,
        &file.uri,
        "a:  1\nb: 2\n",
        2,
    )
    .await;

    assert_eq!(formatted, "a: 1\nb: 2\n");
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
