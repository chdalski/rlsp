// SPDX-License-Identifier: MIT
//! Server cases for the positions of documents with an astral character: a
//! character outside the Basic Multilingual Plane, one code point and two
//! UTF-16 code units. Clients count columns in UTF-16 code units (LSP 3.17,
//! "Position" and `PositionEncodingKind`; the default when the server names no
//! encoding). Each case here holds a position the server sends or reads in
//! UTF-16 code units.
use rlsp_yaml::server::Backend;
use rstest::rstest;
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedReceiver;
use tower_lsp::LspService;
use tower_lsp::lsp_types::PublishDiagnosticsParams;

use super::code_actions::{code_action_request, code_action_request_at};
use super::document_links::document_link_request;
use super::helpers::*;

/// U+1F600: one code point, two UTF-16 code units.
const ASTRAL: &str = "\u{1f600}";

fn range(start: (u32, u32), end: (u32, u32)) -> Value {
    json!({
        "start": { "line": start.0, "character": start.1 },
        "end": { "line": end.0, "character": end.1 }
    })
}

/// The ranges of the document links of `text`.
async fn link_ranges(text: &str) -> Vec<Value> {
    let uri = "file:///test/utf16-links.yaml";
    let mut service = open(text, uri).await;
    let resp = send(&mut service, document_link_request(2, uri)).await;
    let resp = resp.expect("documentLink should return a response");
    let result = resp.result().expect("documentLink should have a result");
    result
        .as_array()
        .expect("documentLink should be an array")
        .iter()
        .map(|link| link["range"].clone())
        .collect()
}

#[rstest]
#[case::plain("key: \u{1f600} https://example.com\n", &[range((0, 8), (0, 27))])]
#[case::double_quoted("key: \"\u{1f600} https://example.com\"\n", &[range((0, 9), (0, 28))])]
#[case::single_quoted("key: '\u{1f600} https://example.com'\n", &[range((0, 9), (0, 28))])]
#[case::two_urls_split_by_an_astral_character(
    "key: \u{1f600} https://a.com \u{1f600} https://b.com\n",
    &[range((0, 8), (0, 21)), range((0, 25), (0, 38))]
)]
#[case::astral_character_on_the_previous_line_lf(
    "a: \u{1f600}\nkey: https://example.com\n",
    &[range((1, 5), (1, 24))]
)]
#[case::astral_character_on_the_previous_line_crlf(
    "a: \u{1f600}\r\nkey: https://example.com\r\n",
    &[range((1, 5), (1, 24))]
)]
#[case::astral_character_on_the_previous_line_cr(
    "a: \u{1f600}\rkey: https://example.com\r",
    &[range((1, 5), (1, 24))]
)]
#[case::second_document(
    "---\na: b\n---\nkey: \u{1f600} https://example.com\n",
    &[range((3, 8), (3, 27))]
)]
#[tokio::test]
async fn a_document_link_after_an_astral_character_in_its_scalar_is_at_its_utf16_columns(
    #[case] text: &str,
    #[case] expected: &[Value],
) {
    assert_eq!(link_ranges(text).await, expected);
}

/// The scalar of `key` in the last document of `text`.
fn scalar_of(text: &str, key: &str) -> String {
    use rlsp_yaml_parser::Node;

    let docs = rlsp_yaml_parser::load(text).expect("the text loads");
    let Node::Mapping { entries, .. } = &docs.last().expect("a document").root else {
        unreachable!("a mapping at the root of {text:?}");
    };
    let (_, value) = entries
        .iter()
        .find(|(k, _)| matches!(k, Node::Scalar { value, .. } if value == key))
        .expect("the key");
    let Node::Scalar { value, .. } = value else {
        unreachable!("a scalar value of {key:?}");
    };
    value.clone()
}

#[rstest]
#[case::astral_key("{A}: \"{X}\"\n", 0, "{A}", (0, 4), (0, 46))]
#[case::astral_character_in_the_trailing_comment("{A}: \"{X}\" # {A}\n", 0, "{A}", (0, 4), (0, 51))]
#[case::astral_character_on_the_previous_line_lf("a: {A}\nk: \"{X}\"\n", 1, "k", (1, 3), (1, 45))]
#[case::astral_character_on_the_previous_line_crlf("a: {A}\r\nk: \"{X}\"\r\n", 1, "k", (1, 3), (1, 45))]
#[case::astral_character_on_the_previous_line_cr("a: {A}\rk: \"{X}\"\r", 1, "k", (1, 3), (1, 45))]
#[case::second_document("a: b\n---\n{A}: \"{X}\"\n", 2, "{A}", (2, 4), (2, 46))]
#[tokio::test]
async fn the_block_scalar_action_edits_from_and_to_utf16_columns(
    #[case] template: &str,
    #[case] line: u32,
    #[case] key: &str,
    #[case] start: (u32, u32),
    #[case] end: (u32, u32),
) {
    let uri = "file:///test/utf16-block-scalar.yaml";
    let value = "x".repeat(40);
    let text = template.replace("{A}", ASTRAL).replace("{X}", &value);
    let key = key.replace("{A}", ASTRAL);
    let mut service = open(&text, uri).await;

    let resp = send(&mut service, code_action_request(2, uri, line, line)).await;
    let resp = resp.expect("codeAction should return a response");
    let result = resp.result().expect("codeAction should have a result");
    let actions = result.as_array().expect("an array of actions");
    let titles: Vec<&str> = actions.iter().filter_map(|a| a["title"].as_str()).collect();
    assert_eq!(
        titles,
        [
            "Convert to block scalar (literal)",
            "Convert to block scalar (folded)"
        ]
    );
    for action in actions {
        assert_eq!(
            action["edit"]["changes"][uri][0]["range"],
            range(start, end),
            "{action}"
        );
        // The edit replaces the quoted scalar, and its trailing comment when it
        // has one, which the block header then carries; the key stays, and the
        // value is the same text plus the final line break that clip chomping
        // keeps.
        let edited = apply_action(&text, uri, action);
        if text.contains("# ") {
            assert_eq!(
                edited.matches(&format!("# {ASTRAL}")).count(),
                1,
                "{edited:?}"
            );
        }
        assert_eq!(
            scalar_of(&edited, &key).trim_end_matches('\n'),
            value,
            "{edited:?}"
        );
    }
}

// ---- published diagnostics and code actions ----

/// The boolean-or-string schema that `open_collecting` seeds: every field of an
/// object has the type `field_type`.
fn field_schema(field_type: &str) -> Value {
    json!({ "type": "object", "additionalProperties": { "type": field_type } })
}

const SCHEMA_URL: &str = "https://schemas.test/fields.json";

/// A server with `text` open at `uri` and, for `field_type`, the schema that
/// `SCHEMA_URL` names seeded, with what it publishes collected.
async fn open_collecting(
    text: &str,
    uri: &str,
    field_type: Option<&str>,
) -> (
    LspService<Backend>,
    UnboundedReceiver<PublishDiagnosticsParams>,
) {
    let (mut service, socket) = LspService::new(Backend::new);
    let published = collect_published_diagnostics(socket);
    if let Some(field_type) = field_type {
        service.inner().seed_schema_cache(
            SCHEMA_URL,
            rlsp_yaml::schema::parse_schema(&field_schema(field_type)).expect("the schema parses"),
        );
    }
    send(&mut service, initialize_request(1)).await;
    send(&mut service, initialized_notification()).await;
    send(&mut service, did_open_notification(uri, text)).await;
    (service, published)
}

/// The diagnostics of the first publication for `uri`, as JSON.
async fn first_publication(
    published: &mut UnboundedReceiver<PublishDiagnosticsParams>,
    uri: &str,
) -> Vec<Value> {
    loop {
        let publication = tokio::time::timeout(std::time::Duration::from_secs(5), published.recv())
            .await
            .expect("a publication arrives")
            .expect("the channel is open");
        if publication.uri.as_str() == uri {
            return publication
                .diagnostics
                .iter()
                .map(|diagnostic| serde_json::to_value(diagnostic).expect("JSON"))
                .collect();
        }
    }
}

/// The actions the server answers for the range `start..end`.
async fn actions_for(
    service: &mut LspService<Backend>,
    uri: &str,
    start: (u32, u32),
    end: (u32, u32),
) -> Vec<Value> {
    let resp = send(service, code_action_request_at(2, uri, start, end)).await;
    let resp = resp.expect("codeAction should return a response");
    resp.result()
        .expect("codeAction should have a result")
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The ranges of the diagnostics with the code `code`.
fn ranges_of(diagnostics: &[Value], code: &str) -> Vec<Value> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == code)
        .map(|diagnostic| diagnostic["range"].clone())
        .collect()
}

/// A range as its start and end, each `(line, character)`.
type Span = ((u32, u32), (u32, u32));

#[rstest]
#[case::astral_key("{A}: &a v\n", &[((0, 4), (0, 6))])]
#[case::astral_on_the_previous_line_lf("{A}: 1\nk: &a v\n", &[((1, 3), (1, 5))])]
#[case::astral_on_the_previous_line_crlf("{A}: 1\r\nk: &a v\r\n", &[((1, 3), (1, 5))])]
#[case::astral_on_the_previous_line_cr("{A}: 1\rk: &a v\r", &[((1, 3), (1, 5))])]
#[case::second_document("a: b\n---\n{A}: &a v\n", &[((2, 4), (2, 6))])]
#[case::astral_after_the_token("k: &a v # {A}\n", &[((0, 3), (0, 5))])]
#[case::astral_between_two_tokens("k: [&a 1, {A}, &b 2]\n", &[((0, 4), (0, 6)), ((0, 14), (0, 16))])]
#[case::no_astral_character("k: &a v\n", &[((0, 3), (0, 5))])]
#[tokio::test]
async fn the_unused_anchor_diagnostic_is_published_at_utf16_columns(
    #[case] template: &str,
    #[case] expected: &[Span],
) {
    let uri = "file:///test/utf16-unused-anchor.yaml";
    let (_service, mut published) =
        open_collecting(&template.replace("{A}", ASTRAL), uri, None).await;

    let diagnostics = first_publication(&mut published, uri).await;

    let expected: Vec<Value> = expected.iter().map(|&(s, e)| range(s, e)).collect();
    assert_eq!(ranges_of(&diagnostics, "unusedAnchor"), expected);
}

#[tokio::test]
async fn a_syntax_error_after_an_astral_character_keeps_the_end_of_the_line_column() {
    let uri = "file:///test/utf16-syntax-error.yaml";
    let text = format!("{ASTRAL}: a: b\n");
    let internal = rlsp_yaml::parser::parse_yaml(&text).diagnostics;
    let internal = internal.first().expect("a syntax error").range;
    assert_eq!(internal.end.character, u32::MAX);
    assert!(
        internal.start.character > 1,
        "the error is after the character"
    );
    let (_service, mut published) = open_collecting(&text, uri, None).await;

    let diagnostics = first_publication(&mut published, uri).await;

    let published = &diagnostics[0]["range"];
    assert_eq!(published["start"]["line"], internal.start.line);
    assert_eq!(
        published["start"]["character"],
        internal.start.character + 1
    );
    assert_eq!(published["end"]["character"], u32::MAX);
}

#[tokio::test]
async fn the_stored_diagnostics_keep_internal_positions() {
    let uri = "file:///test/utf16-stored.yaml";
    let (service, mut published) = open_collecting(&format!("{ASTRAL}: &a v\n"), uri, None).await;
    let published = first_publication(&mut published, uri).await;

    let stored = service.inner().get_diagnostics(uri).expect("diagnostics");

    assert_eq!(published[0]["range"], range((0, 4), (0, 6)));
    assert_eq!(
        serde_json::to_value(stored[0].range).expect("JSON"),
        range((0, 3), (0, 5))
    );
}

#[tokio::test]
async fn the_delete_anchor_action_edits_at_utf16_columns_and_carries_the_published_diagnostic() {
    let uri = "file:///test/utf16-unused-anchor-fix.yaml";
    let text = format!("{ASTRAL}: &a v\n");
    let (mut service, mut published) = open_collecting(&text, uri, None).await;
    let diagnostics = first_publication(&mut published, uri).await;

    let actions = actions_for(&mut service, uri, (0, 0), (0, 0)).await;

    let fix = actions
        .iter()
        .find(|action| action["title"] == "Delete unused anchor")
        .expect("the fix is offered");
    assert_eq!(
        fix["edit"]["changes"][uri][0]["range"],
        range((0, 4), (0, 7))
    );
    assert_eq!(fix["diagnostics"], json!(diagnostics));
    assert_eq!(apply_action(&text, uri, fix), format!("{ASTRAL}: v\n"));
}

/// One case per kind of action that code actions offer, with an astral
/// character before the node of the action on its line (`{A}`), the title the
/// case must offer, the cursor column of the request in the document in which
/// `{A}` is the one-unit `x`, the line of the request, and the type of the
/// fields of the schema, for the cases that need one.
#[rstest]
#[case::flow_mapping("{A}: {k: v}\n", "Convert flow mapping to block style", 0, 0, None)]
#[case::flow_sequence("{A}: [1, 2]\n", "Convert flow sequence to block style", 0, 0, None)]
#[case::flow_sequence_over_two_lines(
    "k: [a,\n  {A}b]\n",
    "Convert flow sequence to block style",
    0,
    0,
    None
)]
#[case::block_to_flow("{A}:\n  a: 1\n  b: 2\n", "Convert block to flow style", 0, 0, None)]
#[case::block_to_flow_astral_on_the_last_line(
    "k:\n  a: 1\n  {A}: 2\n",
    "Convert block to flow style",
    0,
    0,
    None
)]
#[case::unused_anchor("{A}: &a v\n", "Delete unused anchor", 0, 0, None)]
#[case::unused_anchor_after_an_astral_line(
    "# {A}\n{A}{A}: &a v\n",
    "Delete unused anchor",
    1,
    0,
    None
)]
#[case::yaml11_boolean("{A}: yes\n", "Quote value", 0, 0, None)]
#[case::yaml11_octal("{A}: 0755\n", "Quote as string", 0, 0, None)]
#[case::tabs("{A}: v\t# c\n", "Convert tabs to spaces", 0, 0, None)]
#[case::quoted_boolean("{A}: \"true\"\n", "Convert quoted string to true", 0, 4, None)]
#[case::block_scalar(
    "{A}: \"this is a long string that goes on and on and on for ever\"\n",
    "Convert to block scalar (literal)",
    0,
    0,
    None
)]
#[case::schema_yaml11_boolean(
    "# yaml-language-server: $schema=https://schemas.test/fields.json\n{A}: yes\n",
    "Quote value",
    1,
    0,
    Some("string")
)]
#[case::schema_yaml11_octal(
    "# yaml-language-server: $schema=https://schemas.test/fields.json\n{A}: 0755\n",
    "Quote as string",
    1,
    0,
    Some("string")
)]
#[case::schema_yaml11_boolean_type(
    "# yaml-language-server: $schema=https://schemas.test/fields.json\n{A}: yes\n",
    "Convert to boolean",
    1,
    0,
    Some("boolean")
)]
#[tokio::test]
async fn every_action_edits_a_line_with_an_astral_character_like_one_without(
    #[case] template: &str,
    #[case] title: &str,
    #[case] line: u32,
    #[case] cursor: u32,
    #[case] field_type: Option<&str>,
) {
    let uri = "file:///test/utf16-actions.yaml";
    let astral = template.replace("{A}", ASTRAL);
    let narrow = template.replace("{A}", "x");
    assert_eq!(narrow.matches('x').count(), template.matches("{A}").count());

    let (mut service, mut published) = open_collecting(&astral, uri, field_type).await;
    let diagnostics = first_publication(&mut published, uri).await;
    // The cursor moves one unit right for each astral character before it.
    let shift = narrow
        .split('\n')
        .nth(line as usize)
        .expect("the request line exists")
        .chars()
        .take(cursor as usize)
        .filter(|&c| c == 'x')
        .count();
    let widened_cursor = cursor + u32::try_from(shift).expect("fits");
    let widened = actions_for(
        &mut service,
        uri,
        (line, widened_cursor),
        (line, widened_cursor),
    )
    .await;
    let (mut service, _published) = open_collecting(&narrow, uri, field_type).await;
    let narrowed = actions_for(&mut service, uri, (line, cursor), (line, cursor)).await;

    let titles = |actions: &[Value]| -> Vec<Value> {
        actions
            .iter()
            .map(|action| action["title"].clone())
            .collect()
    };
    assert!(
        titles(&widened).contains(&json!(title)),
        "{template:?} offers {title:?}: {widened:?}"
    );
    assert_eq!(titles(&widened), titles(&narrowed), "{template:?}");
    for (action, narrow_action) in widened.iter().zip(&narrowed) {
        assert_eq!(
            apply_action(&astral, uri, action),
            apply_action(&narrow, uri, narrow_action).replace('x', ASTRAL),
            "{template:?}: {action}"
        );
        for diagnostic in action["diagnostics"].as_array().into_iter().flatten() {
            assert!(
                diagnostics.contains(diagnostic),
                "{template:?}: {diagnostic} was not published: {diagnostics:?}"
            );
        }
    }
}

// The quoted scalar `"true"` of `{A}: "true"` stands at the client columns 4
// to 10; counted in code points it stands at 3 to 9, and a request at a client
// column finds the node there.
#[rstest]
#[case::at_the_end_of_the_scalar(10, true)]
#[case::at_its_start(4, true)]
#[case::inside_it(7, true)]
#[case::before_it(3, false)]
#[case::in_the_key(1, false)]
#[case::past_the_scalar(11, false)]
#[tokio::test]
async fn a_request_after_an_astral_character_finds_the_node_at_its_utf16_column(
    #[case] column: u32,
    #[case] found: bool,
) {
    let uri = "file:///test/utf16-cursor.yaml";
    let (mut service, _published) =
        open_collecting(&format!("{ASTRAL}: \"true\"\n"), uri, None).await;

    for end in [(0, column), (0, 999)] {
        let actions = actions_for(&mut service, uri, (0, column), end).await;

        let titles: Vec<&str> = actions.iter().filter_map(|a| a["title"].as_str()).collect();
        assert_eq!(
            titles.contains(&"Convert quoted string to true"),
            found,
            "{end:?}: {titles:?}"
        );
    }
}

#[tokio::test]
async fn a_document_without_an_astral_character_is_answered_in_the_same_columns() {
    let uri = "file:///test/utf16-ascii.yaml";
    let text = "k: &a v\n";
    let (mut service, mut published) = open_collecting(text, uri, None).await;
    let diagnostics = first_publication(&mut published, uri).await;

    let actions = actions_for(&mut service, uri, (0, 0), (0, 0)).await;

    assert_eq!(
        ranges_of(&diagnostics, "unusedAnchor"),
        [range((0, 3), (0, 5))]
    );
    let fix = actions
        .iter()
        .find(|action| action["title"] == "Delete unused anchor")
        .expect("the fix is offered");
    assert_eq!(
        fix["edit"]["changes"][uri][0]["range"],
        range((0, 3), (0, 6))
    );
}

#[rstest]
#[case::the_maximum_range((u32::MAX, u32::MAX), (u32::MAX, u32::MAX))]
#[case::past_the_document((9, 9), (9, 9))]
#[case::the_maximum_end((0, 1), (u32::MAX, u32::MAX))]
#[case::the_maximum_column((0, u32::MAX), (0, u32::MAX))]
#[tokio::test]
async fn a_request_with_a_range_far_outside_the_document_is_answered(
    #[case] start: (u32, u32),
    #[case] end: (u32, u32),
) {
    let uri = "file:///test/utf16-range.yaml";
    let (mut service, _published) =
        open_collecting(&format!("{ASTRAL}: &a [1]\n"), uri, None).await;

    let resp = send(&mut service, code_action_request_at(2, uri, start, end)).await;

    assert!(resp.expect("a response").is_ok());
}

// ---- requests that take a position ----

const REQUEST_URI: &str = "file:///test/utf16-requests.yaml";

/// The requests that read a position of the document.
#[derive(Debug, Clone, Copy)]
enum Handler {
    Hover,
    Completion,
    Definition,
    References { include_declaration: bool },
    PrepareRename,
    Rename,
    OnType,
}

impl Handler {
    fn request(self, position: (u32, u32)) -> tower_lsp::jsonrpc::Request {
        let (line, character) = position;
        match self {
            Self::Hover => super::hover::hover_request(2, REQUEST_URI, line, character),
            Self::Completion => {
                super::completion::completion_request(2, REQUEST_URI, line, character)
            }
            Self::Definition => {
                super::navigation::definition_request(2, REQUEST_URI, line, character)
            }
            Self::References {
                include_declaration,
            } => super::navigation::references_request(
                2,
                REQUEST_URI,
                line,
                character,
                include_declaration,
            ),
            Self::PrepareRename => {
                super::rename::prepare_rename_request(2, REQUEST_URI, line, character)
            }
            Self::Rename => super::rename::rename_request(2, REQUEST_URI, line, character, "zz"),
            Self::OnType => super::on_type_formatting::on_type_formatting_request(
                2,
                REQUEST_URI,
                line,
                character,
                "\n",
            ),
        }
    }
}

/// The result that `request` gets from a server with `text` open.
async fn result_of(
    service: &mut LspService<Backend>,
    request: tower_lsp::jsonrpc::Request,
) -> Value {
    let resp = send(service, request).await.expect("a response");
    assert!(resp.is_ok(), "{resp:?}");
    resp.result().expect("a result").clone()
}

fn location(start: (u32, u32), end: (u32, u32)) -> Value {
    json!({ "uri": REQUEST_URI, "range": range(start, end) })
}

fn hover_of(path: &str, value: &str) -> Value {
    json!({ "contents": {
        "kind": "markdown",
        "value": format!("**Path:** `{path}`\n\n**Type:** scalar\n\n**Value:** `{value}`\n")
    } })
}

const FIELDS_MODELINE: &str = "# yaml-language-server: $schema=https://schemas.test/fields.json\n";

// Each document has an astral character before the position on its line, and
// the answer is in client positions: counted in code points the positions
// would stand one column left for each such character.
#[rstest]
#[case::hover("[{A}, &a v, *a]\n", None, Handler::Hover, (0, 8), hover_of("[1]", "v"))]
#[case::hover_with_the_character_on_the_previous_line_lf(
    "{A}: 1\nk: v\n", None, Handler::Hover, (1, 3), hover_of("k", "v")
)]
#[case::hover_with_the_character_on_the_previous_line_crlf(
    "{A}: 1\r\nk: v\r\n", None, Handler::Hover, (1, 3), hover_of("k", "v")
)]
#[case::hover_with_the_character_on_the_previous_line_cr(
    "{A}: 1\rk: v\r", None, Handler::Hover, (1, 3), hover_of("k", "v")
)]
#[case::hover_in_the_second_document(
    "a: b\n---\n{A}: 1\nk: v\n", None, Handler::Hover, (3, 3), hover_of("k", "v")
)]
#[case::completion_after_the_character(
    &format!("{FIELDS_MODELINE}k: {{A}}x\n"),
    Some("boolean"),
    Handler::Completion,
    (1, 5),
    json!([{ "kind": 12, "label": "true" }, { "kind": 12, "label": "false" }])
)]
// This case puts the cursor on the second unit of the character (client column
// 4 of `k: {A}`), which finds the character like its start; a request that is
// not converted finds nothing there.
#[case::completion_inside_the_character(
    &format!("{FIELDS_MODELINE}k: {{A}}\n"),
    Some("boolean"),
    Handler::Completion,
    (1, 4),
    json!([{ "kind": 12, "label": "true" }, { "kind": 12, "label": "false" }])
)]
#[case::definition(
    "[{A}, &a v, *a]\n", None, Handler::Definition, (0, 12), location((0, 5), (0, 7))
)]
#[case::definition_at_the_start_of_the_alias(
    "[{A}, &a v, *a]\n", None, Handler::Definition, (0, 11), location((0, 5), (0, 7))
)]
#[case::references_with_the_declaration(
    "[{A}, &a v, *a]\n",
    None,
    Handler::References { include_declaration: true },
    (0, 12),
    json!([location((0, 5), (0, 7)), location((0, 11), (0, 13))])
)]
#[case::references_without_the_declaration(
    "[{A}, &a v, *a]\n",
    None,
    Handler::References { include_declaration: false },
    (0, 6),
    json!([location((0, 11), (0, 13))])
)]
#[case::prepare_rename(
    "[{A}, &a v, *a]\n", None, Handler::PrepareRename, (0, 12), range((0, 11), (0, 13))
)]
#[case::prepare_rename_of_the_anchor(
    "[{A}, &a v, *a]\n", None, Handler::PrepareRename, (0, 6), range((0, 5), (0, 7))
)]
#[case::rename(
    "[{A}, &a v, *a]\n",
    None,
    Handler::Rename,
    (0, 12),
    json!({ "changes": { REQUEST_URI: [
        { "range": range((0, 5), (0, 7)), "newText": "&zz" },
        { "range": range((0, 11), (0, 13)), "newText": "*zz" }
    ] } })
)]
#[case::on_type_formatting_with_the_character_on_the_cursor_line(
    "a:\n{A}:\n",
    None,
    Handler::OnType,
    (1, 3),
    json!([{ "range": range((1, 0), (1, 3)), "newText": "  " }])
)]
#[case::on_type_formatting_with_the_character_on_the_previous_line(
    "a:\n{A}:\n",
    None,
    Handler::OnType,
    (2, 0),
    json!([{ "range": range((2, 0), (2, 0)), "newText": "  " }])
)]
#[tokio::test]
async fn a_request_at_a_client_position_finds_the_node_and_answers_client_positions(
    #[case] template: &str,
    #[case] field_type: Option<&str>,
    #[case] handler: Handler,
    #[case] position: (u32, u32),
    #[case] expected: Value,
) {
    let text = template.replace("{A}", ASTRAL);
    let (mut service, _published) = open_collecting(&text, REQUEST_URI, field_type).await;

    let result = result_of(&mut service, handler.request(position)).await;

    assert_eq!(result, expected);
}

/// `text` with the `changes` of a rename or the `edits` of a formatting
/// applied, last edit first so the earlier ranges stay valid.
fn apply_edits(text: &str, edits: &Value) -> String {
    let mut edits: Vec<&Value> = edits.as_array().expect("edits").iter().collect();
    edits.sort_by_key(|edit| offset_of(text, edit.pointer("/range/start").expect("a start")));
    let mut edited = text.to_string();
    for edit in edits.into_iter().rev() {
        let start = offset_of(text, edit.pointer("/range/start").expect("a start"));
        let end = offset_of(text, edit.pointer("/range/end").expect("an end"));
        edited.replace_range(start..end, edit["newText"].as_str().expect("new text"));
    }
    edited
}

#[tokio::test]
async fn a_rename_after_an_astral_character_edits_the_document_of_the_request_only() {
    let text = format!("[{ASTRAL}, &a v, *a]\n");
    let mut service = open(&text, REQUEST_URI).await;

    let result = result_of(&mut service, Handler::Rename.request((0, 12))).await;

    let changes = result["changes"].as_object().expect("changes");
    assert_eq!(changes.keys().collect::<Vec<_>>(), [REQUEST_URI]);
    assert!(result.get("documentChanges").is_none(), "{result}");
    assert_eq!(
        apply_edits(&text, &changes[REQUEST_URI]),
        format!("[{ASTRAL}, &zz v, *zz]\n")
    );
}

#[rstest]
#[case::handler_hover(Handler::Hover)]
#[case::handler_completion(Handler::Completion)]
#[case::handler_definition(Handler::Definition)]
#[case::handler_references(Handler::References { include_declaration: true })]
#[case::handler_prepare_rename(Handler::PrepareRename)]
#[case::handler_rename(Handler::Rename)]
#[case::handler_on_type(Handler::OnType)]
#[tokio::test]
async fn a_request_at_an_extreme_position_is_answered(#[case] handler: Handler) {
    let text = format!("{ASTRAL}: &a [{ASTRAL}, *a]\n{ASTRAL}{ASTRAL}: v\n");
    let mut service = open(&text, REQUEST_URI).await;

    for position in [
        (0, u32::MAX),
        (u32::MAX, 0),
        (u32::MAX, u32::MAX),
        (1, u32::MAX),
        (0, 1),
        (0, 2),
        (0, 3),
    ] {
        let resp = send(&mut service, handler.request(position))
            .await
            .expect("a response");
        assert!(resp.is_ok(), "{handler:?} at {position:?}: {resp:?}");
    }
}

fn selection_chain(ranges: &[Span]) -> Value {
    let mut chain = Value::Null;
    for &(start, end) in ranges.iter().rev() {
        chain = if chain.is_null() {
            json!({ "range": range(start, end) })
        } else {
            json!({ "range": range(start, end), "parent": chain })
        };
    }
    chain
}

async fn selection_result(text: &str, positions: &[(u32, u32)]) -> Value {
    let mut service = open(text, REQUEST_URI).await;
    result_of(
        &mut service,
        super::selection_ranges::selection_range_request_for(2, REQUEST_URI, positions),
    )
    .await
}

#[rstest]
#[case::the_value_in_a_block_mapping(
    "{A}: &a val\nk: *a\n",
    (0, 9),
    selection_chain(&[((0, 7), (0, 10)), ((0, 0), (0, 10)), ((0, 0), (2, 0))])
)]
#[case::the_value_after_two_characters(
    "{A}{A}: val\n",
    (0, 8),
    selection_chain(&[((0, 6), (0, 9)), ((0, 0), (0, 9)), ((0, 0), (1, 0))])
)]
#[case::a_column_inside_the_character_selects_what_the_character_does(
    "k: a{A}b\n",
    (0, 5),
    selection_chain(&[((0, 3), (0, 7)), ((0, 0), (0, 7)), ((0, 0), (1, 0))])
)]
#[tokio::test]
async fn a_selection_range_after_an_astral_character_is_in_client_positions(
    #[case] template: &str,
    #[case] position: (u32, u32),
    #[case] chain: Value,
) {
    let result = selection_result(&template.replace("{A}", ASTRAL), &[position]).await;

    assert_eq!(result, json!([chain]));
}

#[rstest]
#[case::inside_the_character((0, 3))]
#[case::after_the_character((0, 4))]
#[case::past_the_end_of_the_line((0, 50))]
#[case::the_maximum_column((0, u32::MAX))]
#[case::past_the_last_line((99, 0))]
#[case::the_maximum_position((u32::MAX, u32::MAX))]
#[tokio::test]
async fn a_selection_range_without_a_node_is_the_empty_range_at_the_position_sent(
    #[case] position: (u32, u32),
) {
    for text in [
        format!("# {ASTRAL}x\n"),
        // A syntax error leaves no document.
        format!("{ASTRAL}: a: b\n"),
        String::new(),
    ] {
        let result = selection_result(&text, &[position]).await;

        assert_eq!(
            result,
            json!([{ "range": range(position, position) }]),
            "{text:?}"
        );
    }
}

#[tokio::test]
async fn selection_ranges_for_several_positions_keep_their_order() {
    let text = format!("# {ASTRAL}\nk: {ASTRAL}v\n");
    let positions = [(1, 4), (0, 3), (1, 6), (0, 3)];

    let result = selection_result(&text, &positions).await;

    let result = result.as_array().expect("an array");
    assert_eq!(result.len(), 4);
    let empty = |position: (u32, u32)| json!({ "range": range(position, position) });
    assert_eq!(result[1], empty((0, 3)));
    assert_eq!(result[3], empty((0, 3)));
    for index in [0, 2] {
        assert_eq!(result[index]["range"], range((1, 3), (1, 6)), "{index}");
        assert!(result[index].get("parent").is_some(), "{index}");
    }
}

/// The edits that range formatting answers for `start..end`.
async fn formatted_lines(
    service: &mut LspService<Backend>,
    start: (u32, u32),
    end: (u32, u32),
) -> Value {
    let request =
        super::formatting_integration::range_formatting_request_at(2, REQUEST_URI, start, end, 2);
    result_of(service, request).await
}

#[tokio::test]
async fn range_formatting_after_an_astral_character_formats_the_lines_of_the_range() {
    let text = format!("a:\n    {ASTRAL}: 1\n    b: {ASTRAL}\nc:   2\n");
    let mut service = open(&text, REQUEST_URI).await;
    let whole_lines = formatted_lines(&mut service, (1, 0), (2, 0)).await;

    assert_eq!(
        apply_edits(&text, &whole_lines),
        format!("a:\n  {ASTRAL}: 1\n  b: {ASTRAL}\nc: 2\n")
    );
    for (start, end) in [
        // After the character, inside it, and past the end of the line.
        ((1, 7), (2, 9)),
        ((1, 6), (2, 8)),
        ((1, 99), (2, 99)),
        ((1, 3), (2, u32::MAX)),
        ((1, u32::MAX), (2, 0)),
    ] {
        assert_eq!(
            formatted_lines(&mut service, start, end).await,
            whole_lines,
            "{start:?}..{end:?}"
        );
    }
}

#[tokio::test]
async fn a_selection_request_with_many_positions_is_answered_one_entry_each_in_order() {
    let text = format!("# {ASTRAL}\n{ASTRAL}: val\n");
    let positions: Vec<(u32, u32)> = (0..10_000_u32)
        .map(|i| match i % 3 {
            0 => (1, 6),
            1 => (0, u32::MAX),
            _ => (u32::MAX, u32::MAX),
        })
        .collect();

    let result = selection_result(&text, &positions).await;

    let result = result.as_array().expect("an array");
    assert_eq!(result.len(), positions.len());
    for (entry, &position) in result.iter().zip(&positions) {
        if position == (1, 6) {
            assert_eq!(entry.get("range"), Some(&range((1, 4), (1, 7))));
        } else {
            assert_eq!(entry, &json!({ "range": range(position, position) }));
        }
    }
}
