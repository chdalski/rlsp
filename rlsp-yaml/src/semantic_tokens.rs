// SPDX-License-Identifier: MIT

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

/// Scans `text` line by line and returns semantic tokens in LSP delta-encoded format.
#[must_use]
pub fn semantic_tokens(text: &str) -> Vec<SemanticToken> {
    let mut raw: Vec<RawToken> = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        // LSP line numbers fit in u32 for any realistic document.
        #[allow(clippy::cast_possible_truncation)]
        let line_no = line_idx as u32;
        let trimmed = line.trim_start();
        let indent = to_u32(line.len() - trimmed.len());

        if trimmed.starts_with('#') {
            raw.push(RawToken {
                line: line_no,
                start: indent,
                length: to_u32(trimmed.trim_end().chars().count()),
                token_type: TOKEN_COMMENT,
                token_modifiers_bitset: 0,
            });
            continue;
        }

        // Scan for inline markers (tags, anchors, aliases).
        collect_inline_markers(line, line_no, &mut raw);

        // Split on mapping colon.
        if let Some(colon_pos) = find_mapping_colon(line) {
            // Key: text before the colon.
            let key_raw = &line[..colon_pos];
            let key_text = strip_sequence_prefix(key_raw).trim();
            if !key_text.is_empty() && !starts_with_inline_marker(key_text) {
                // key_text is a subslice of line — pointer arithmetic gives the
                // exact byte offset without substring search.
                let key_byte = key_text.as_ptr() as usize - line.as_ptr() as usize;
                raw.push(RawToken {
                    line: line_no,
                    start: to_u32(char_col_of(line, key_byte)),
                    length: to_u32(key_text.chars().count()),
                    token_type: TOKEN_PROPERTY,
                    token_modifiers_bitset: 0,
                });
            }

            // Value: text after `: `.
            let after_colon = &line[colon_pos + 1..];
            let stripped = after_colon.trim_start_matches([' ', '\t']);
            if !stripped.is_empty() && !starts_with_inline_marker(stripped) {
                let value_byte =
                    line.len() - after_colon.len() + (after_colon.len() - stripped.len());
                let value_start = to_u32(char_col_of(line, value_byte));
                if let Some(rt) = classify_scalar(stripped, line_no, value_start) {
                    raw.push(rt);
                }
            }
        } else {
            // No mapping colon — bare scalar or sequence item.
            let content = strip_sequence_prefix(trimmed).trim();
            if !content.is_empty() && !starts_with_inline_marker(content) {
                // content is a subslice of line — pointer arithmetic gives the exact offset.
                let byte = content.as_ptr() as usize - line.as_ptr() as usize;
                let start = to_u32(char_col_of(line, byte));
                if let Some(rt) = classify_scalar(content, line_no, start) {
                    raw.push(rt);
                }
            }
        }
    }

    // Sort by (line, start) so delta encoding is correct regardless of the
    // order in which key / value / inline-marker tokens were collected.
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

/// Classifies a scalar string and returns a `RawToken`, or `None` if empty.
fn classify_scalar(value: &str, line: u32, start: u32) -> Option<RawToken> {
    let trimmed = value.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    // Block scalar indicators.
    if matches!(trimmed, "|" | ">" | "|-" | ">-" | "|+" | ">+") {
        return Some(RawToken {
            line,
            start,
            length: to_u32(trimmed.chars().count()),
            token_type: TOKEN_OPERATOR,
            token_modifiers_bitset: 0,
        });
    }

    // Quoted strings.
    if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
    {
        return Some(RawToken {
            line,
            start,
            length: to_u32(trimmed.chars().count()),
            token_type: TOKEN_STRING,
            token_modifiers_bitset: 0,
        });
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
            length: to_u32(trimmed.chars().count()),
            token_type: TOKEN_KEYWORD,
            token_modifiers_bitset: 0,
        });
    }

    // Numbers.
    if is_number(trimmed) {
        return Some(RawToken {
            line,
            start,
            length: to_u32(trimmed.chars().count()),
            token_type: TOKEN_NUMBER,
            token_modifiers_bitset: 0,
        });
    }

    // Fallback: unquoted scalar → STRING.
    Some(RawToken {
        line,
        start,
        length: to_u32(trimmed.chars().count()),
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

/// Scans `line` for `!tag`, `&anchor`, and `*alias` outside quotes and appends
/// `RawToken`s to `out`.
fn collect_inline_markers(line: &str, line_no: u32, out: &mut Vec<RawToken>) {
    let mut in_single = false;
    let mut in_double = false;

    let mut iter = line.char_indices().peekable();
    while let Some((byte_pos, ch)) = iter.next() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => break,
            '!' | '&' | '*' if !in_single && !in_double => {
                let rest = &line[byte_pos..];
                let len_bytes = rest
                    .find(|c: char| c.is_whitespace() || c == ':' || c == ',')
                    .unwrap_or(rest.len());
                let token_str = &rest[..len_bytes];
                if token_str.len() > 1 {
                    let (token_type, modifiers) = match ch {
                        '!' => (TOKEN_TYPE, 0),
                        '&' => (TOKEN_VARIABLE, MOD_DECLARATION),
                        '*' => (TOKEN_VARIABLE, 0),
                        _ => unreachable!(),
                    };
                    out.push(RawToken {
                        line: line_no,
                        start: to_u32(char_col_of(line, byte_pos)),
                        length: to_u32(token_str.chars().count()),
                        token_type,
                        token_modifiers_bitset: modifiers,
                    });
                    // Advance the iterator past the rest of the token.
                    // We already consumed `ch`; skip `token_str.len() - 1` more bytes.
                    let end_byte = byte_pos + len_bytes;
                    // Drain characters until we reach end_byte.
                    while iter.peek().is_some_and(|(b, _)| *b < end_byte) {
                        iter.next();
                    }
                }
            }
            _ => {}
        }
    }
}

/// Returns `true` if `s` starts with a tag, anchor, or alias sigil.
fn starts_with_inline_marker(s: &str) -> bool {
    s.starts_with('!') || s.starts_with('&') || s.starts_with('*')
}

/// Strips a leading `- ` sequence prefix.
fn strip_sequence_prefix(s: &str) -> &str {
    s.strip_prefix("- ")
        .unwrap_or_else(|| if s == "-" { "" } else { s })
}

/// Returns the character-column offset of `byte_pos` in `line`.
fn char_col_of(line: &str, byte_pos: usize) -> usize {
    line[..byte_pos].chars().count()
}

/// Find the byte position of the mapping colon in a YAML line.
fn find_mapping_colon(line: &str) -> Option<usize> {
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for (i, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ':' if !in_single_quote && !in_double_quote => {
                let rest = &line[i + 1..];
                if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Converts a `usize` to `u32`, clamping at `u32::MAX`.
/// LSP positions are u32; documents exceeding 4 GB or 4 billion lines are
/// not supported in practice.
fn to_u32(v: usize) -> u32 {
    v.try_into().unwrap_or(u32::MAX)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use rstest::rstest;

    use super::*;

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
        assert!(semantic_tokens("").is_empty());
    }

    #[test]
    fn comment_line_produces_comment_token() {
        let abs = absolute(&semantic_tokens("# comment"));
        let comments: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_COMMENT).collect();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].0, 0); // line 0
        assert_eq!(comments[0].1, 0); // col 0
    }

    #[test]
    fn comment_line_with_indent_starts_at_hash() {
        let abs = absolute(&semantic_tokens("  # indented comment"));
        let comments: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_COMMENT).collect();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].1, 2); // col 2
    }

    #[test]
    fn mapping_key_produces_property_token() {
        let abs = absolute(&semantic_tokens("name: value"));
        let keys: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_PROPERTY).collect();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].0, 0); // line 0
        assert_eq!(keys[0].1, 0); // col 0
        assert_eq!(keys[0].2, 4); // len("name")
    }

    #[test]
    fn string_value_produces_string_token() {
        let abs = absolute(&semantic_tokens("key: hello"));
        let strings: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_STRING).collect();
        assert!(!strings.is_empty());
        assert_eq!(strings[0].2, 5); // len("hello")
    }

    #[test]
    fn quoted_string_value_produces_string_token() {
        let abs = absolute(&semantic_tokens(r#"key: "quoted""#));
        assert!(abs.iter().any(|t| t.3 == TOKEN_STRING));
    }

    #[test]
    fn integer_value_produces_number_token() {
        let abs = absolute(&semantic_tokens("count: 42"));
        let nums: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_NUMBER).collect();
        assert_eq!(nums.len(), 1);
        assert_eq!(nums[0].2, 2); // len("42")
    }

    #[test]
    fn float_value_produces_number_token() {
        let abs = absolute(&semantic_tokens("pi: 3.14"));
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
        let abs = absolute(&semantic_tokens(input));
        assert!(abs.iter().any(|t| t.3 == TOKEN_KEYWORD));
    }

    #[test]
    fn anchor_produces_variable_with_declaration_modifier() {
        let abs = absolute(&semantic_tokens("base: &anchor value"));
        assert!(
            abs.iter()
                .any(|t| t.3 == TOKEN_VARIABLE && t.4 == MOD_DECLARATION)
        );
    }

    #[test]
    fn alias_produces_variable_without_modifier() {
        let abs = absolute(&semantic_tokens("child: *anchor"));
        assert!(abs.iter().any(|t| t.3 == TOKEN_VARIABLE && t.4 == 0));
    }

    #[test]
    fn tag_produces_type_token() {
        let abs = absolute(&semantic_tokens("value: !include file.yaml"));
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
        let abs = absolute(&semantic_tokens(input));
        assert!(abs.iter().any(|t| t.3 == TOKEN_OPERATOR));
    }

    #[test]
    fn delta_encoding_correct_for_multi_line_document() {
        let text = "a: 1\nb: 2\n";
        let abs = absolute(&semantic_tokens(text));
        let keys: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_PROPERTY).collect();
        assert!(keys.iter().any(|k| k.0 == 0 && k.1 == 0)); // "a" line 0 col 0
        assert!(keys.iter().any(|k| k.0 == 1 && k.1 == 0)); // "b" line 1 col 0
    }

    #[test]
    fn delta_line_is_zero_for_tokens_on_same_line() {
        let tokens = semantic_tokens("key: value");
        for t in tokens.iter().skip(1) {
            assert_eq!(t.delta_line, 0);
        }
    }

    #[test]
    fn delta_start_is_relative_to_previous_token_on_same_line() {
        // "key: value" — property at col 0 (len 3), string at col 5 (len 5).
        let abs = absolute(&semantic_tokens("key: value"));
        let prop = abs.iter().find(|t| t.3 == TOKEN_PROPERTY).unwrap();
        let str_tok = abs.iter().find(|t| t.3 == TOKEN_STRING).unwrap();
        assert_eq!(prop.1, 0);
        assert_eq!(str_tok.1, 5);
    }

    // ---- Additional coverage tests ----

    // strip_sequence_prefix: bare dash returns empty (no token)
    #[test]
    fn bare_dash_sequence_item_produces_no_token() {
        // A bare "-" with no value should produce no scalar token
        let abs = absolute(&semantic_tokens("items:\n  -\n"));
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
        let abs = absolute(&semantic_tokens("temp: -42"));
        let nums: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_NUMBER).collect();
        assert_eq!(nums.len(), 1);
        assert_eq!(nums[0].2, 3); // len("-42")
    }

    // is_number: scientific notation
    #[test]
    fn scientific_notation_produces_number_token() {
        let abs = absolute(&semantic_tokens("val: 1.5e10"));
        assert_eq!(abs.iter().filter(|t| t.3 == TOKEN_NUMBER).count(), 1);
    }

    // is_number: negative scientific notation
    #[test]
    fn negative_float_produces_number_token() {
        let abs = absolute(&semantic_tokens("val: -3.14"));
        let nums: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_NUMBER).collect();
        assert_eq!(nums.len(), 1);
        assert_eq!(nums[0].2, 5); // len("-3.14")
    }

    // sequence item with string value (no mapping colon)
    #[test]
    fn sequence_item_string_value_produces_string_token() {
        let abs = absolute(&semantic_tokens("- hello"));
        let strings: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_STRING).collect();
        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0].2, 5); // len("hello")
    }

    // sequence item with number value
    #[test]
    fn sequence_item_number_value_produces_number_token() {
        let abs = absolute(&semantic_tokens("- 42"));
        assert_eq!(abs.iter().filter(|t| t.3 == TOKEN_NUMBER).count(), 1);
    }

    // sequence item with keyword value
    #[test]
    fn sequence_item_keyword_value_produces_keyword_token() {
        let abs = absolute(&semantic_tokens("- true"));
        assert!(abs.iter().any(|t| t.3 == TOKEN_KEYWORD));
    }

    // inline tag on value side (key: !tag value)
    #[test]
    fn tag_on_value_side_of_mapping_produces_type_token() {
        let abs = absolute(&semantic_tokens("key: !str hello"));
        assert!(
            abs.iter().any(|t| t.3 == TOKEN_TYPE),
            "tag on value side should produce type token"
        );
    }

    // anchor on sequence item
    #[test]
    fn anchor_on_sequence_item_produces_variable_with_declaration() {
        let abs = absolute(&semantic_tokens("- &myanchor value"));
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
        let abs = absolute(&semantic_tokens("key: value # &notananchor"));
        assert!(
            abs.iter().all(|t| t.3 != TOKEN_VARIABLE),
            "marker inside comment should not produce variable token"
        );
    }

    // delta_line is correct when crossing multiple lines
    #[test]
    fn delta_line_correct_across_multiple_lines() {
        let text = "a: 1\n\nb: 2\n";
        let abs = absolute(&semantic_tokens(text));
        let keys: Vec<_> = abs.iter().filter(|t| t.3 == TOKEN_PROPERTY).collect();
        assert!(keys.iter().any(|k| k.0 == 0)); // "a" on line 0
        assert!(keys.iter().any(|k| k.0 == 2)); // "b" on line 2 (blank line skipped)
    }
}
