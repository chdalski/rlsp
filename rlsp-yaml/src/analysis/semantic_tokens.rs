// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::{Document, Event, Node, Span};
use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend,
};

// Token type indices — order must match legend().
const TOKEN_PROPERTY: u32 = 0;
const TOKEN_STRING: u32 = 1;
const TOKEN_NUMBER: u32 = 2;
const TOKEN_KEYWORD: u32 = 3;
const TOKEN_VARIABLE: u32 = 4;
const TOKEN_TYPE: u32 = 5;
const TOKEN_COMMENT: u32 = 6;
const TOKEN_OPERATOR: u32 = 7;

// Token modifier bitmasks — order must match legend().
const MOD_DECLARATION: u32 = 1; // bit 0

/// Returns the semantic tokens legend (token types and modifiers).
#[must_use]
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::PROPERTY, // 0
            SemanticTokenType::STRING,   // 1
            SemanticTokenType::NUMBER,   // 2
            SemanticTokenType::KEYWORD,  // 3
            SemanticTokenType::VARIABLE, // 4
            SemanticTokenType::TYPE,     // 5
            SemanticTokenType::COMMENT,  // 6
            SemanticTokenType::OPERATOR, // 7
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION, // bit 0
        ],
    }
}

/// A token with absolute position, before delta encoding.
struct RawToken {
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
    token_modifiers_bitset: u32,
}

/// Produces semantic tokens for syntax highlighting from the YAML AST.
///
/// `text` is retained solely for `Event::Comment` extraction via
/// `rlsp_yaml_parser::parse_events` — all structural tokens come from `docs`.
#[must_use]
pub fn semantic_tokens(docs: &[Document<Span>], text: &str) -> Vec<SemanticToken> {
    let mut raw: Vec<RawToken> = Vec::new();

    for doc in docs {
        collect_node_tokens(&doc.root, &mut raw);
    }

    collect_comment_tokens(text, &mut raw);

    // Sort by (line, start) so delta encoding is correct regardless of the
    // order in which tokens were collected.
    raw.sort_by_key(|t| (t.line, t.start));

    // Delta-encode.
    let mut tokens = Vec::with_capacity(raw.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
    for rt in raw {
        let delta_line = rt.line - prev_line;
        let delta_start = if delta_line == 0 {
            rt.start - prev_start
        } else {
            rt.start
        };
        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length: rt.length,
            token_type: rt.token_type,
            token_modifiers_bitset: rt.token_modifiers_bitset,
        });
        prev_line = rt.line;
        prev_start = rt.start;
    }
    tokens
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Recursively walk a node and collect tokens for all token kinds except comments.
fn collect_node_tokens(node: &Node<Span>, out: &mut Vec<RawToken>) {
    // Emit anchor token if present (all node kinds except Alias carry anchor_loc).
    if let Some(span) = node.anchor_loc() {
        out.push(span_to_raw(span, TOKEN_VARIABLE, MOD_DECLARATION));
    }

    // Emit tag token if present (all node kinds except Alias carry tag_loc).
    if let Some(span) = node.tag_loc() {
        out.push(span_to_raw(span, TOKEN_TYPE, 0));
    }

    match node {
        Node::Scalar {
            loc, style, value, ..
        } => {
            if let Some(rt) = classify_scalar_node(value, *style, *loc) {
                out.push(rt);
            }
        }
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                // For scalar keys: emit TOKEN_PROPERTY at key.loc (the key text span),
                // plus any anchor/tag on the key. Do NOT emit a scalar content token
                // (that would be double-classifying the key text as STRING).
                // For non-scalar keys (complex keys): recurse normally.
                if let Node::Scalar {
                    loc,
                    anchor_loc,
                    tag_loc,
                    ..
                } = key
                {
                    out.push(span_to_raw(*loc, TOKEN_PROPERTY, 0));
                    if let Some(span) = anchor_loc {
                        out.push(span_to_raw(*span, TOKEN_VARIABLE, MOD_DECLARATION));
                    }
                    if let Some(span) = tag_loc {
                        out.push(span_to_raw(*span, TOKEN_TYPE, 0));
                    }
                } else {
                    collect_node_tokens(key, out);
                }
                collect_node_tokens(value, out);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_node_tokens(item, out);
            }
        }
        Node::Alias { loc, .. } => {
            out.push(span_to_raw(*loc, TOKEN_VARIABLE, 0));
        }
    }
}

/// Collect comment tokens from `Event::Comment` events in the event stream.
fn collect_comment_tokens(yaml: &str, out: &mut Vec<RawToken>) {
    for result in rlsp_yaml_parser::parse_events(yaml) {
        if let Ok((Event::Comment { .. }, span)) = result {
            out.push(span_to_raw(span, TOKEN_COMMENT, 0));
        }
    }
}

/// Convert an AST `Span` to a `RawToken`.
///
/// AST positions: line is 1-based, column is 0-based codepoints.
/// LSP positions: line is 0-based, column is 0-based codepoints.
const fn span_to_raw(span: Span, token_type: u32, token_modifiers_bitset: u32) -> RawToken {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    RawToken {
        line: (span.start.line.saturating_sub(1)) as u32,
        start: span.start.column as u32,
        length: (span.end.column.saturating_sub(span.start.column)) as u32,
        token_type,
        token_modifiers_bitset,
    }
}

/// Classifies a scalar node's value and returns a `RawToken`, or `None` if
/// the scalar should produce no token (empty value).
fn classify_scalar_node(
    value: &str,
    style: rlsp_yaml_parser::ScalarStyle,
    loc: Span,
) -> Option<RawToken> {
    use rlsp_yaml_parser::ScalarStyle;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let length = (loc.end.column.saturating_sub(loc.start.column)) as u32;
    if length == 0 {
        return None;
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "LSP line/col are u32; always fits"
    )]
    let (line, start) = (
        (loc.start.line.saturating_sub(1)) as u32,
        loc.start.column as u32,
    );

    // Block scalar indicators.
    match style {
        ScalarStyle::Literal(_) | ScalarStyle::Folded(_) => {
            return Some(RawToken {
                line,
                start,
                length: 1, // only the `|` or `>` sigil
                token_type: TOKEN_OPERATOR,
                token_modifiers_bitset: 0,
            });
        }
        ScalarStyle::Plain | ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => {}
    }

    // Quoted strings.
    match style {
        ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => {
            return Some(RawToken {
                line,
                start,
                length,
                token_type: TOKEN_STRING,
                token_modifiers_bitset: 0,
            });
        }
        ScalarStyle::Plain | ScalarStyle::Literal(_) | ScalarStyle::Folded(_) => {}
    }

    // Plain scalar — classify by content.
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Keywords: booleans and null.
    let lower = trimmed.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "~"
    ) {
        return Some(RawToken {
            line,
            start,
            length,
            token_type: TOKEN_KEYWORD,
            token_modifiers_bitset: 0,
        });
    }

    // Numbers.
    if is_number(trimmed) {
        return Some(RawToken {
            line,
            start,
            length,
            token_type: TOKEN_NUMBER,
            token_modifiers_bitset: 0,
        });
    }

    // Fallback: unquoted scalar → STRING.
    Some(RawToken {
        line,
        start,
        length,
        token_type: TOKEN_STRING,
        token_modifiers_bitset: 0,
    })
}

/// Returns `true` if `s` looks like a YAML number.
fn is_number(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let s = s.strip_prefix('-').unwrap_or(s);
    if s.is_empty() {
        return false;
    }
    let (int_part, rest) = split_digits(s);
    if int_part.is_empty() {
        return false;
    }
    let rest = if let Some(r) = rest.strip_prefix('.') {
        let (frac, r2) = split_digits(r);
        if frac.is_empty() {
            return false;
        }
        r2
    } else {
        rest
    };
    if rest.is_empty() {
        return true;
    }
    let rest = if rest.starts_with(['e', 'E']) {
        let r = &rest[1..];
        r.strip_prefix(['+', '-']).unwrap_or(r)
    } else {
        return false;
    };
    let (exp_digits, leftover) = split_digits(rest);
    !exp_digits.is_empty() && leftover.is_empty()
}

/// Returns `(digit_prefix, remainder)`.
fn split_digits(s: &str) -> (&str, &str) {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    s.split_at(end)
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::wildcard_enum_match_arm,
    reason = "test code"
)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_utils::parse_docs;

    /// Decode delta-encoded tokens into `(abs_line, abs_start, length, token_type, modifiers)`.
    fn absolute(tokens: &[SemanticToken]) -> Vec<(u32, u32, u32, u32, u32)> {
        let mut line = 0u32;
        let mut start = 0u32;
        tokens
            .iter()
            .map(|t| {
                line += t.delta_line;
                start = if t.delta_line == 0 {
                    start + t.delta_start
                } else {
                    t.delta_start
                };
                (
                    line,
                    start,
                    t.length,
                    t.token_type,
                    t.token_modifiers_bitset,
                )
            })
            .collect()
    }

    #[test]
    fn legend_has_correct_token_type_count() {
        assert_eq!(legend().token_types.len(), 8);
    }

    #[test]
    fn legend_has_correct_modifier_count() {
        assert_eq!(legend().token_modifiers.len(), 1);
    }

    #[test]
    fn empty_document_produces_no_tokens() {
        assert!(semantic_tokens(&parse_docs(""), "").is_empty());
    }

    #[test]
    fn comment_line_produces_comment_token() {
        let text = "# comment";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let comments: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_COMMENT).collect();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].0, 0); // line 0
        assert_eq!(comments[0].1, 0); // col 0
    }

    #[test]
    fn comment_line_with_indent_starts_at_hash() {
        let text = "  # indented comment";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let comments: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_COMMENT).collect();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].1, 2); // col 2
    }

    #[test]
    fn mapping_key_produces_property_token() {
        let text = "name: value";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let keys: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_PROPERTY).collect();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].0, 0); // line 0
        assert_eq!(keys[0].1, 0); // col 0
        assert_eq!(keys[0].2, 4); // len("name")
    }

    #[test]
    fn string_value_produces_string_token() {
        let text = "key: hello";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let strings: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_STRING).collect();
        assert!(!strings.is_empty());
        assert_eq!(strings[0].2, 5); // len("hello")
    }

    #[test]
    fn quoted_string_value_produces_string_token() {
        let text = r#"key: "quoted""#;
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        assert!(abs.iter().any(|t| t.3 == TOKEN_STRING));
    }

    #[test]
    fn integer_value_produces_number_token() {
        let text = "count: 42";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let nums: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_NUMBER).collect();
        assert_eq!(nums.len(), 1);
        assert_eq!(nums[0].2, 2); // len("42")
    }

    #[test]
    fn float_value_produces_number_token() {
        let text = "pi: 3.14";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let nums: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_NUMBER).collect();
        assert_eq!(nums.len(), 1);
        assert_eq!(nums[0].2, 4); // len("3.14")
    }

    #[rstest]
    #[case::true_keyword("flag: true")]
    #[case::false_keyword("flag: false")]
    #[case::yes_keyword("flag: yes")]
    #[case::no_keyword("flag: no")]
    #[case::null_keyword("x: null")]
    #[case::tilde_null("x: ~")]
    #[case::on_keyword("flag: on")]
    #[case::off_keyword("flag: off")]
    fn produces_keyword_token(#[case] input: &str) {
        let abs = absolute(&semantic_tokens(&parse_docs(input), input));
        assert!(abs.iter().any(|t| t.3 == TOKEN_KEYWORD));
    }

    #[test]
    fn anchor_produces_variable_with_declaration_modifier() {
        let text = "base: &anchor value";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        assert!(
            abs.iter()
                .any(|t| t.3 == TOKEN_VARIABLE && t.4 == MOD_DECLARATION)
        );
    }

    #[test]
    fn alias_produces_variable_without_modifier() {
        let text = "a: &x val\nb: *x\n";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        assert!(abs.iter().any(|t| t.3 == TOKEN_VARIABLE && t.4 == 0));
    }

    #[test]
    fn tag_produces_type_token() {
        let text = "value: !include file.yaml";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        assert!(abs.iter().any(|t| t.3 == TOKEN_TYPE));
    }

    #[rstest]
    #[case::pipe("text: |")]
    #[case::gt("text: >")]
    #[case::pipe_minus("text: |-")]
    #[case::gt_minus("text: >-")]
    #[case::pipe_plus("text: |+")]
    #[case::gt_plus("text: >+")]
    fn block_scalar_produces_operator_token(#[case] input: &str) {
        let abs = absolute(&semantic_tokens(&parse_docs(input), input));
        assert!(abs.iter().any(|t| t.3 == TOKEN_OPERATOR));
    }

    #[test]
    fn delta_encoding_correct_for_multi_line_document() {
        let text = "a: 1\nb: 2\n";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let keys: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_PROPERTY).collect();
        assert!(keys.iter().any(|k| k.0 == 0 && k.1 == 0)); // "a" line 0 col 0
        assert!(keys.iter().any(|k| k.0 == 1 && k.1 == 0)); // "b" line 1 col 0
    }

    #[test]
    fn delta_line_is_zero_for_tokens_on_same_line() {
        let text = "key: value";
        let tokens = semantic_tokens(&parse_docs(text), text);
        for t in tokens.iter().skip(1) {
            assert_eq!(t.delta_line, 0);
        }
    }

    #[test]
    fn delta_start_is_relative_to_previous_token_on_same_line() {
        // "key: value" — property at col 0 (len 3), string at col 5 (len 5).
        let text = "key: value";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let prop = abs.iter().find(|t| t.3 == TOKEN_PROPERTY).unwrap();
        let str_tok = abs.iter().find(|t| t.3 == TOKEN_STRING).unwrap();
        assert_eq!(prop.1, 0);
        assert_eq!(str_tok.1, 5);
    }

    // ---- Additional coverage tests ----

    // strip_sequence_prefix: bare dash returns empty (no token)
    #[test]
    fn bare_dash_sequence_item_produces_no_token() {
        // A bare "-" with no value should produce no scalar token.
        // The parser emits an empty scalar for the bare "-" item.
        let text = "items:\n  -\n";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        // Only "items" key token expected; no STRING/NUMBER/KEYWORD for bare "-"
        let non_property: Vec<_> = abs.iter().filter(|t| t.3 != TOKEN_PROPERTY).collect();
        assert!(
            non_property.is_empty(),
            "bare '-' should produce no scalar token, got: {non_property:?}"
        );
    }

    // is_number: negative integer
    #[test]
    fn negative_integer_produces_number_token() {
        let text = "temp: -42";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let nums: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_NUMBER).collect();
        assert_eq!(nums.len(), 1);
        assert_eq!(nums[0].2, 3); // len("-42")
    }

    // is_number: scientific notation
    #[test]
    fn scientific_notation_produces_number_token() {
        let text = "val: 1.5e10";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        assert_eq!(abs.iter().filter(|t| t.3 == TOKEN_NUMBER).count(), 1);
    }

    // is_number: negative scientific notation
    #[test]
    fn negative_float_produces_number_token() {
        let text = "val: -3.14";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let nums: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_NUMBER).collect();
        assert_eq!(nums.len(), 1);
        assert_eq!(nums[0].2, 5); // len("-3.14")
    }

    // sequence item with string value (no mapping colon)
    #[test]
    fn sequence_item_string_value_produces_string_token() {
        let text = "- hello";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let strings: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_STRING).collect();
        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0].2, 5); // len("hello")
    }

    // sequence item with number value
    #[test]
    fn sequence_item_number_value_produces_number_token() {
        let text = "- 42";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        assert_eq!(abs.iter().filter(|t| t.3 == TOKEN_NUMBER).count(), 1);
    }

    // sequence item with keyword value
    #[test]
    fn sequence_item_keyword_value_produces_keyword_token() {
        let text = "- true";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        assert!(abs.iter().any(|t| t.3 == TOKEN_KEYWORD));
    }

    // inline tag on value side (key: !tag value)
    #[test]
    fn tag_on_value_side_of_mapping_produces_type_token() {
        let text = "key: !str hello";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        assert!(
            abs.iter().any(|t| t.3 == TOKEN_TYPE),
            "tag on value side should produce type token"
        );
    }

    // anchor on sequence item
    #[test]
    fn anchor_on_sequence_item_produces_variable_with_declaration() {
        let text = "- &myanchor value";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        assert!(
            abs.iter()
                .any(|t| t.3 == TOKEN_VARIABLE && t.4 == MOD_DECLARATION),
            "anchor on sequence item should produce variable with declaration modifier"
        );
    }

    // comment stops inline marker scan (# in middle of line)
    #[test]
    fn inline_comment_stops_marker_scan() {
        // "&anchor" after "#" should NOT be treated as an anchor token
        let text = "key: value # &notananchor";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        assert!(
            abs.iter().all(|t| t.3 != TOKEN_VARIABLE),
            "marker inside comment should not produce variable token"
        );
    }

    // delta_line is correct when crossing multiple lines
    #[test]
    fn delta_line_correct_across_multiple_lines() {
        let text = "a: 1\n\nb: 2\n";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let keys: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_PROPERTY).collect();
        assert!(keys.iter().any(|k| k.0 == 0)); // "a" on line 0
        assert!(keys.iter().any(|k| k.0 == 2)); // "b" on line 2 (blank line skipped)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Mandatory regression cases (Task 3)
    // ─────────────────────────────────────────────────────────────────────────

    // (a) Mapping key token position matches AST key.loc
    #[rstest]
    #[case::simple_key("name: value\n", 0u32, 0u32, 4u32)]
    fn mapping_key_token_position_matches_ast_key_loc(
        #[case] text: &str,
        #[case] expected_line: u32,
        #[case] expected_col: u32,
        #[case] expected_len: u32,
    ) {
        let docs = parse_docs(text);
        let abs = absolute(&semantic_tokens(&docs, text));
        let keys: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_PROPERTY).collect();
        assert!(!keys.is_empty(), "expected at least one property token");
        let key = keys[0];
        assert_eq!(key.0, expected_line, "key line mismatch");
        assert_eq!(key.1, expected_col, "key col mismatch");
        assert_eq!(key.2, expected_len, "key length mismatch");
    }

    // (b) Anchor token position matches anchor_loc, NOT scalar loc
    #[rstest]
    #[case::anchor_before_value("base: &anchor value\n")]
    fn anchor_token_position_matches_anchor_loc_not_scalar_loc(#[case] text: &str) {
        let docs = parse_docs(text);
        let abs = absolute(&semantic_tokens(&docs, text));

        let anchor_tokens: Vec<_> = abs
            .iter()
            .filter(|t| t.3 == TOKEN_VARIABLE && t.4 == MOD_DECLARATION)
            .collect();
        assert_eq!(anchor_tokens.len(), 1, "expected exactly one anchor token");
        let tok = anchor_tokens[0];

        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected mapping root");
        };
        let (_, value) = &entries[0];
        let anchor_loc = value.anchor_loc().expect("expected anchor_loc on value");
        let scalar_loc = match value {
            Node::Scalar { loc, .. } => *loc,
            _ => panic!("expected scalar value"),
        };

        // Token must be at anchor_loc, not scalar_loc
        assert_eq!(
            tok.0,
            (anchor_loc.start.line.saturating_sub(1)) as u32,
            "anchor token line must match anchor_loc"
        );
        assert_eq!(
            tok.1, anchor_loc.start.column as u32,
            "anchor token col must match anchor_loc, not scalar_loc col {}",
            scalar_loc.start.column
        );
        assert_eq!(
            tok.2,
            (anchor_loc
                .end
                .column
                .saturating_sub(anchor_loc.start.column)) as u32,
            "anchor token length must span '&anchor'"
        );
    }

    // (c) Alias token position matches alias.loc
    #[rstest]
    #[case::alias_reference("a: &x val\nb: *x\n")]
    fn alias_token_position_matches_alias_loc(#[case] text: &str) {
        let docs = parse_docs(text);
        let abs = absolute(&semantic_tokens(&docs, text));

        let alias_tokens: Vec<_> = abs
            .iter()
            .filter(|t| t.3 == TOKEN_VARIABLE && t.4 == 0)
            .collect();
        assert!(
            !alias_tokens.is_empty(),
            "expected at least one alias token"
        );

        // The alias "*x" is on line 1 (0-based), col 3 (after "b: ")
        let tok = alias_tokens
            .iter()
            .find(|t| t.0 == 1)
            .expect("alias on line 1");

        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected mapping root");
        };
        let (_, alias_node) = &entries[1];
        let Node::Alias { loc, .. } = alias_node else {
            panic!("expected alias node");
        };

        assert_eq!(
            tok.1, loc.start.column as u32,
            "alias token col must match alias.loc"
        );
        assert_eq!(
            tok.2,
            (loc.end.column.saturating_sub(loc.start.column)) as u32,
            "alias token length must span '*x'"
        );
    }

    // (d) Comment token position matches Event::Comment span
    #[rstest]
    #[case::simple_comment("# hello\n", 0u32, 0u32, 7u32)]
    fn comment_token_position_matches_event_comment_span(
        #[case] text: &str,
        #[case] expected_line: u32,
        #[case] expected_col: u32,
        #[case] expected_len: u32,
    ) {
        let docs = parse_docs(text);
        let abs = absolute(&semantic_tokens(&docs, text));

        let comment_tok = abs
            .iter()
            .find(|t| t.3 == TOKEN_COMMENT)
            .expect("expected a comment token");

        // Verify against event stream span directly
        let comment_span = rlsp_yaml_parser::parse_events(text)
            .find_map(|r| {
                if let Ok((Event::Comment { .. }, span)) = r {
                    Some(span)
                } else {
                    None
                }
            })
            .expect("expected Event::Comment");

        assert_eq!(
            comment_tok.0,
            (comment_span.start.line.saturating_sub(1)) as u32,
            "comment token line must match event span"
        );
        assert_eq!(
            comment_tok.1, comment_span.start.column as u32,
            "comment token col must match event span"
        );
        assert_eq!(comment_tok.0, expected_line, "comment line");
        assert_eq!(comment_tok.1, expected_col, "comment col");
        assert_eq!(comment_tok.2, expected_len, "comment length");
    }

    // (e) Tagged scalar: tag token at tag_loc, scalar token starts after
    #[rstest]
    #[case::int_tag("n: !!int 42\n")]
    fn tagged_scalar_tag_token_at_tag_loc_not_scalar_loc(#[case] text: &str) {
        let docs = parse_docs(text);
        let abs = absolute(&semantic_tokens(&docs, text));

        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("expected mapping root");
        };
        let (_, value) = &entries[0];
        let tag_loc = value.tag_loc().expect("expected tag_loc");
        let scalar_loc = match value {
            Node::Scalar { loc, .. } => *loc,
            _ => panic!("expected scalar value"),
        };

        let tag_tokens: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_TYPE).collect();
        assert_eq!(tag_tokens.len(), 1, "expected exactly one type token");
        let tag_tok = tag_tokens[0];

        assert_eq!(
            tag_tok.1, tag_loc.start.column as u32,
            "tag token col must match tag_loc (col {}), not scalar col {}",
            tag_loc.start.column, scalar_loc.start.column
        );
        assert_eq!(
            tag_tok.2,
            (tag_loc.end.column.saturating_sub(tag_loc.start.column)) as u32,
            "tag length must span '!!int'"
        );

        // Scalar (42) must be at a different, later column than the tag
        let number_tok = abs.iter().find(|t| t.3 == TOKEN_NUMBER);
        if let Some(num) = number_tok {
            assert!(
                num.1 > tag_tok.1,
                "scalar token (col {}) must start after tag token (col {})",
                num.1,
                tag_tok.1
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Edge cases (new standalone tests)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn anchor_on_mapping_node_produces_anchor_token() {
        let text = "&m\na: 1\n";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let anchor_tokens: Vec<_> = abs
            .iter()
            .filter(|t| t.3 == TOKEN_VARIABLE && t.4 == MOD_DECLARATION)
            .collect();
        assert!(
            !anchor_tokens.is_empty(),
            "expected anchor token for mapping node"
        );
        // "&m" is at line 0, col 0, length 2
        assert!(
            anchor_tokens
                .iter()
                .any(|t| t.0 == 0 && t.1 == 0 && t.2 == 2),
            "anchor '&m' must be at (0, 0) len 2, got: {anchor_tokens:?}"
        );
    }

    #[test]
    fn anchor_on_sequence_node_produces_anchor_token() {
        let text = "items: &seq\n  - one\n";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        assert!(
            abs.iter()
                .any(|t| t.3 == TOKEN_VARIABLE && t.4 == MOD_DECLARATION),
            "anchor on sequence node should produce variable with declaration modifier"
        );
    }

    #[test]
    fn tag_on_mapping_node_produces_tag_token() {
        let text = "!!map\na: 1\n";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let type_tokens: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_TYPE).collect();
        assert!(
            !type_tokens.is_empty(),
            "expected tag token for mapping node"
        );
        // "!!map" is at line 0, col 0, length 5
        assert!(
            type_tokens.iter().any(|t| t.0 == 0 && t.1 == 0 && t.2 == 5),
            "tag '!!map' must be at (0, 0) len 5, got: {type_tokens:?}"
        );
    }

    #[test]
    fn nested_mapping_keys_all_produce_property_tokens() {
        let text = "outer:\n  inner: value\n";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let keys: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_PROPERTY).collect();
        assert_eq!(keys.len(), 2, "expected 2 property tokens, got: {keys:?}");
        assert!(
            keys.iter().any(|k| k.0 == 0 && k.1 == 0),
            "'outer' at (0, 0) missing, got: {keys:?}"
        );
        assert!(
            keys.iter().any(|k| k.0 == 1 && k.1 == 2),
            "'inner' at (1, 2) missing, got: {keys:?}"
        );
    }

    #[test]
    fn indented_comment_position_from_event_span() {
        let text = "  # indented\n";
        let abs = absolute(&semantic_tokens(&parse_docs(text), text));
        let comment_tok = abs
            .iter()
            .find(|t| t.3 == TOKEN_COMMENT)
            .expect("expected comment token");
        assert_eq!(comment_tok.0, 0, "comment on line 0");
        assert_eq!(comment_tok.1, 2, "comment starts at col 2 (the '#')");
        assert_eq!(comment_tok.2, 10, "length of '# indented'");
    }

    #[test]
    fn multiple_tokens_on_same_document_sorted_by_position() {
        let text = "key: &anchor value\n";
        let docs = parse_docs(text);
        let abs = absolute(&semantic_tokens(&docs, text));
        assert!(!abs.is_empty(), "expected tokens");
        for window in abs.windows(2) {
            let a = window[0];
            let b = window[1];
            assert!(
                (a.0, a.1) <= (b.0, b.1),
                "tokens not sorted: {:?} > {:?}",
                (a.0, a.1),
                (b.0, b.1)
            );
        }
    }
}
