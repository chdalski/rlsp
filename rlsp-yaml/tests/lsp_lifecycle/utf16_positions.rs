// SPDX-License-Identifier: MIT
//! Server cases for the positions of documents with an astral character: a
//! character outside the Basic Multilingual Plane, one code point and two
//! UTF-16 code units. Clients count columns in UTF-16 code units (LSP 3.17,
//! "Position" and `PositionEncodingKind`; the default when the server names no
//! encoding). Each case here holds a position the server already answers in
//! UTF-16 code units.
use rstest::rstest;
use serde_json::{Value, json};

use super::code_actions::code_action_request;
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
