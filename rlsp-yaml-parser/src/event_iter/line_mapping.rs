// SPDX-License-Identifier: MIT

/// True when `trimmed` (content after stripping leading spaces) represents
/// an implicit mapping key: it contains `: `, `:\t`, or ends with `:`.
pub(in crate::event_iter) fn is_implicit_mapping_line(trimmed: &str) -> bool {
    find_value_indicator_offset(trimmed).is_some()
}

/// Returns `true` when `s` is a block structure indicator that cannot appear
/// at tab-based indentation: a block sequence entry (`-` followed by
/// whitespace or EOL), an explicit key marker (`?` followed by whitespace or
/// EOL), or an implicit mapping key (contains a `:` value indicator).
///
/// Used to detect tab-as-block-indentation violations (YAML 1.2 §6.1).
pub(in crate::event_iter) fn is_tab_indented_block_indicator(s: &str) -> bool {
    s.strip_prefix(['-', '?']).map_or_else(
        || is_implicit_mapping_line(s),
        |after| after.is_empty() || after.starts_with([' ', '\t']),
    )
}

/// Like `find_value_indicator_offset`, but skips any leading anchor (`&name`)
/// and/or tag (`!tag`) tokens before checking for a mapping key indicator.
///
/// This handles cases like `&anchor key: value` or `!!str &a key: value`
/// where the actual key content starts after the properties.
pub(in crate::event_iter) fn inline_contains_mapping_key(inline: &str) -> bool {
    if find_value_indicator_offset(inline).is_some() {
        return true;
    }
    // Skip leading anchor/tag tokens and retry
    let mut s = inline;
    loop {
        let trimmed = s.trim_start_matches([' ', '\t']);
        if let Some(after_amp) = trimmed.strip_prefix('&') {
            // skip anchor name (non-space chars)
            let name_end = after_amp.find([' ', '\t']).unwrap_or(after_amp.len());
            s = &after_amp[name_end..];
        } else if trimmed.starts_with('!') {
            // skip tag token (non-space chars)
            let tag_end = trimmed.find([' ', '\t']).unwrap_or(trimmed.len());
            s = &trimmed[tag_end..];
        } else {
            break;
        }
        if find_value_indicator_offset(s.trim_start_matches([' ', '\t'])).is_some() {
            return true;
        }
    }
    false
}

/// True when a block line that starts with a flow collection, alias, anchor or
/// tag is an implicit mapping key (`[x]: v`, `*a : v`, `&a k: v`); false when
/// it is a node (`[x]`, `*a`, `&a w`) or carries only properties (`&a`,
/// `!!str # c`), whose node follows on a later line.
///
/// Where the line is ambiguous it counts as a key: callers use `false` to
/// reject the line, so a key must never be taken for a node.
pub(in crate::event_iter) fn is_implicit_key_line(trimmed: &str) -> bool {
    let mut rest = trimmed;
    while rest.starts_with(['&', '!']) {
        let token_end = rest.find([' ', '\t']).unwrap_or(rest.len());
        rest = rest[token_end..].trim_start_matches([' ', '\t']);
    }
    if rest.is_empty() || rest.starts_with('#') {
        return false;
    }
    let after_node = match rest.as_bytes().first() {
        Some(b'*') => {
            let name_end = rest.find([' ', '\t']).unwrap_or(rest.len());
            &rest[name_end..]
        }
        Some(b'[' | b'{') => match flow_collection_end(rest) {
            Some(end) => &rest[end..],
            // Unclosed on this line: a multi-line value (a multi-line key is
            // an error the flow handler reports).
            None => return false,
        },
        _ => return find_value_indicator_offset(rest).is_some(),
    };
    let after_node = after_node.trim_start_matches([' ', '\t']);
    !(after_node.is_empty() || after_node.starts_with('#'))
}

/// Byte offset just past the flow collection that opens `s`, if it closes on
/// this line.  Quoted scalars are skipped; a quote only opens one at the start
/// of a scalar, so `it's` inside a plain scalar does not.
fn flow_collection_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0_usize;
    let mut pos = 0;
    let mut scalar_start = true;
    while let Some(&byte) = bytes.get(pos) {
        match byte {
            b'"' | b'\'' if scalar_start => {
                pos += 1;
                while let Some(&inner) = bytes.get(pos) {
                    pos += 1;
                    if byte == b'"' && inner == b'\\' {
                        pos += 1;
                    } else if inner == byte {
                        if byte == b'\'' && bytes.get(pos) == Some(&b'\'') {
                            pos += 1;
                        } else {
                            break;
                        }
                    }
                }
                scalar_start = false;
                continue;
            }
            b'[' | b'{' => {
                depth += 1;
                scalar_start = true;
            }
            b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(pos + 1);
                }
                scalar_start = false;
            }
            b',' | b':' | b' ' | b'\t' => scalar_start = true,
            _ => scalar_start = false,
        }
        pos += 1;
    }
    None
}

/// Return the byte offset of the `:` value indicator within `trimmed`, or
/// `None` if the line is not a mapping entry.
///
/// The `:` must be followed by a space, tab, newline/CR, or end-of-string to
/// count as a value indicator (YAML 1.2 §7.4).  A `:` immediately followed by
/// a non-space `ns-char` is part of a plain scalar.
///
/// Double-quoted and single-quoted spans are skipped correctly: a `:` inside
/// quotes is not a value indicator.
///
/// Lines that begin with YAML indicator characters that cannot start a plain
/// scalar (e.g. `%`, `@`, `` ` ``, `,`, `[`, `]`, `{`, `}`, `#`, `&`, `*`,
/// `!`, `|`, `>`) are rejected immediately — they are not implicit mapping
/// keys.  Quoted-scalar starts (`"`, `'`) and bare-indicator starts (`?`, `-`,
/// `:`) are handled specially.
pub(in crate::event_iter) fn find_value_indicator_offset(trimmed: &str) -> Option<usize> {
    // Reject lines that start with indicator characters that cannot begin a
    // plain scalar (and are thus not valid implicit mapping keys).
    // Also reject lines starting with `\t`: YAML 1.2 §6.1 forbids tabs as
    // indentation, so a line beginning with a tab cannot be a mapping entry.
    if matches!(
        trimmed.as_bytes().first().copied(),
        Some(
            b'\t'
                | b'%'
                | b'@'
                | b'`'
                | b','
                | b'['
                | b']'
                | b'{'
                | b'}'
                | b'#'
                | b'&'
                | b'*'
                | b'!'
                | b'|'
                | b'>'
        )
    ) {
        return None;
    }

    let bytes = trimmed.as_bytes();
    let mut pos = 0;

    // Handle leading double-quoted span at byte 0.
    // Only treat `"` as a quoted-span delimiter when it appears at the very
    // start of the key (pos == 0) — in YAML, `"key": value` has a
    // double-quoted key, but `a"b": value` has a literal `"` inside a plain
    // scalar key, which must not be mistaken for a quoted span.
    if bytes.first().copied() == Some(b'"') {
        pos = 1; // skip opening `"`
        while let Some(&inner) = bytes.get(pos) {
            match inner {
                b'\\' => pos += 2, // skip escape sequence (two bytes)
                b'"' => {
                    pos += 1; // skip closing `"`
                    break;
                }
                _ => pos += 1,
            }
        }
        // After a quoted span, fall through to the memchr scan below.
        // `prev_was_space` context: the closing `"` is not whitespace, so
        // a `#` immediately after is not a comment boundary.
    }
    // Handle leading single-quoted span at byte 0.
    else if bytes.first().copied() == Some(b'\'') {
        pos = 1; // skip opening `'`
        while let Some(&inner) = bytes.get(pos) {
            pos += 1;
            if inner == b'\'' {
                // `''` is an escaped single-quote; a lone `'` ends the span.
                if bytes.get(pos).copied() == Some(b'\'') {
                    pos += 1; // consume the second `'` of the `''` escape
                } else {
                    break; // lone `'` — end of quoted span
                }
            }
        }
        // After a quoted span, fall through to the memchr scan below.
    }

    // Main scan: use memchr2 to jump to the next `:` or `#` candidate.
    // Everything between `pos` and the candidate is safe content that
    // cannot terminate the scan — skip it in bulk.
    //
    // `prev_was_space` tracks whether the byte immediately before `pos` was
    // whitespace; used only when memchr lands on a `#`. We reconstruct it
    // lazily: after a quoted span the preceding byte is `"` or `'`
    // (non-whitespace), so `prev_was_space` starts false.
    let mut prev_was_space = false;

    while pos < bytes.len() {
        let rel = memchr::memchr2(b':', b'#', bytes.get(pos..).unwrap_or_default())?;
        let hit = pos + rel;

        // Scan the bytes between `pos` and `hit` to update `prev_was_space`.
        // All bytes in this range are neither `:` nor `#`, so they are safe
        // content. We only need the *last* byte's whitespace status.
        if rel > 0 {
            // UTF-8 continuation bytes (0x80–0xBF) are never whitespace.
            // hit - 1 is valid: rel > 0 implies hit > pos >= 0, so hit >= 1.
            let last = bytes.get(hit - 1).copied().unwrap_or(0);
            prev_was_space = last == b' ' || last == b'\t';
        }

        let Some(&b) = bytes.get(hit) else { break };
        if b == b'#' {
            // A `#` at position 0 is already rejected by the fast-path above
            // (hit==0 means pos==0 and rel==0, possible only if the string
            // starts with `#` — but those are caught by the indicator-prefix
            // check before we reach here; if somehow hit==0 treat as comment).
            if hit == 0 || prev_was_space {
                return None;
            }
            // `#` not preceded by whitespace: it is part of the content.
            // Continue scanning from the byte after `#`.
            prev_was_space = false;
            pos = hit + 1;
        } else {
            // b == b':'
            match bytes.get(hit + 1).copied() {
                None | Some(b' ' | b'\t' | b'\n' | b'\r') => return Some(hit),
                _ => {
                    // `:` followed by ns-char: not a value indicator; skip.
                    prev_was_space = false;
                    pos = hit + 1;
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        find_value_indicator_offset, inline_contains_mapping_key, is_implicit_key_line,
        is_implicit_mapping_line,
    };
    use rstest::rstest;

    /// Every line that `is_implicit_mapping_line` accepts must also produce
    /// `Some` from `find_value_indicator_offset`.  This is the contract
    /// enforced by the `unreachable!` at the `consume_mapping_entry` call site —
    /// if the two ever diverge a future change will trigger a runtime panic
    /// under `#[deny(clippy::panic)]`.
    ///
    /// The table covers: trailing colon, colon-space, colon-tab, colon in
    /// quoted spans (must be accepted by peek but offset still returned),
    /// multi-byte characters before the colon, and lines that should not
    /// be accepted.
    #[test]
    fn find_value_indicator_agrees_with_is_implicit_mapping_line() {
        let accepted = [
            "key:",
            "key: value",
            "key:\t",
            "key:  multiple spaces",
            "\"quoted key\": val",
            "'single quoted': val",
            "key with spaces: val",
            "k:",
            "longer-key-with-dashes: v",
            "unicode_\u{00e9}: v",
        ];
        for line in accepted {
            assert!(
                is_implicit_mapping_line(line),
                "expected is_implicit_mapping_line to accept: {line:?}"
            );
            assert!(
                find_value_indicator_offset(line).is_some(),
                "find_value_indicator_offset must return Some for accepted line: {line:?}"
            );
        }

        let rejected = [
            "plain scalar",
            "http://example.com",
            "no colon here",
            "# comment: not a key",
            "",
        ];
        for line in rejected {
            assert!(
                !is_implicit_mapping_line(line),
                "expected is_implicit_mapping_line to reject: {line:?}"
            );
            assert!(
                find_value_indicator_offset(line).is_none(),
                "find_value_indicator_offset must return None for rejected line: {line:?}"
            );
        }
    }

    // Group A: basic acceptance — verify correct byte offset returned
    #[rstest]
    #[case::trailing_colon("key:", 3)]
    #[case::colon_space("key: value", 3)]
    #[case::colon_tab("key:\tv", 3)]
    #[case::single_char_key("k:", 1)]
    #[case::key_with_spaces("key with spaces: v", 15)]
    #[case::dashes_in_key("a-b-c: v", 5)]
    #[case::unicode_before_colon("é: v", 2)] // U+00E9 = 2 UTF-8 bytes
    #[case::colon_at_start_of_value("a: b: c", 1)] // first `:` wins
    fn find_value_indicator_offset_returns_correct_byte_offset(
        #[case] input: &str,
        #[case] expected_offset: usize,
    ) {
        assert_eq!(find_value_indicator_offset(input), Some(expected_offset));
    }

    // Group B: rejection cases — returns None
    #[rstest]
    #[case::plain_scalar("plain scalar")]
    #[case::url("http://example.com")]
    #[case::colon_in_middle_of_word("abc:def")]
    #[case::comment_at_start("# comment: not a key")]
    #[case::comment_after_space("text # comment: x")]
    #[case::empty("")]
    #[case::starts_with_tab("\tkey: v")]
    #[case::starts_with_percent("%TAG")]
    #[case::starts_with_at("@node")]
    #[case::starts_with_backtick("`raw`")]
    #[case::starts_with_comma(",")]
    #[case::starts_with_open_bracket("[a: b]")]
    #[case::starts_with_close_bracket("]")]
    #[case::starts_with_open_brace("{a: b}")]
    #[case::starts_with_close_brace("}")]
    #[case::starts_with_hash_indicator("#")]
    #[case::starts_with_ampersand("&anchor")]
    #[case::starts_with_asterisk("*alias")]
    #[case::starts_with_bang("!tag")]
    #[case::starts_with_pipe("|")]
    #[case::starts_with_gt(">")]
    fn find_value_indicator_offset_rejects(#[case] input: &str) {
        assert!(find_value_indicator_offset(input).is_none());
    }

    // Group C: quoted span handling — colons inside quotes are not indicators
    #[rstest]
    #[case::double_quoted_key_colon_inside("\"ke:y\": val", 6)]
    #[case::single_quoted_key_colon_inside("'ke:y': val", 6)]
    // "ke\"y": val → bytes: " k e \ " y " : SP → colon at byte 7
    #[case::double_quoted_key_escaped_quote("\"ke\\\"y\": val", 7)]
    // 'ke''y': val → bytes: ' k e ' ' y ' : SP → colon at byte 7
    #[case::single_quoted_key_escaped_quote("'ke''y': val", 7)]
    // a"b": val → bytes: a " b " : SP → colon at byte 4 (quote not at pos 0)
    #[case::double_quote_not_at_start("a\"b\": val", 4)]
    fn find_value_indicator_offset_skips_quoted_colons(
        #[case] input: &str,
        #[case] expected_offset: usize,
    ) {
        assert_eq!(find_value_indicator_offset(input), Some(expected_offset));
    }

    // Group D: multi-byte UTF-8 — memchr must advance correctly past non-ASCII
    #[rstest]
    #[case::two_byte_char("é:", 2)] // U+00E9: 2 bytes
    #[case::three_byte_char("中:", 3)] // U+4E2D: 3 bytes
    #[case::four_byte_char("\u{1F600}:", 4)] // U+1F600: 4 bytes
    #[case::mixed_multibyte("é中\u{1F600}:", 9)] // 2+3+4=9 bytes
    fn find_value_indicator_offset_multibyte_utf8(
        #[case] input: &str,
        #[case] expected_offset: usize,
    ) {
        assert_eq!(find_value_indicator_offset(input), Some(expected_offset));
    }

    // Group E: is_implicit_mapping_line agrees extended
    #[test]
    fn is_implicit_mapping_line_agrees_with_find_value_indicator_offset() {
        let accepted = [
            "é:",
            "中:",
            "\u{1F600}:",
            "\"ke:y\": val",
            "'ke:y': val",
            "a\"b\": v", // plain `"` not at pos 0; colon after is indicator
        ];
        for line in accepted {
            assert!(
                is_implicit_mapping_line(line),
                "expected is_implicit_mapping_line to accept: {line:?}"
            );
            assert!(
                find_value_indicator_offset(line).is_some(),
                "find_value_indicator_offset must return Some for: {line:?}"
            );
        }

        let rejected = ["http://example.com", "\tkey: v"];
        for line in rejected {
            assert!(
                !is_implicit_mapping_line(line),
                "expected is_implicit_mapping_line to reject: {line:?}"
            );
            assert!(
                find_value_indicator_offset(line).is_none(),
                "find_value_indicator_offset must return None for: {line:?}"
            );
        }
    }

    // Group G: is_implicit_key_line
    #[rstest]
    #[case::flow_seq_node("[x]", false)]
    #[case::flow_map_node("{x: 1}", false)]
    #[case::flow_seq_node_with_comment("[x] # c", false)]
    #[case::flow_seq_key("[x]: 2", true)]
    #[case::flow_map_key("{x: 1}: 2", true)]
    #[case::flow_key_space_before_colon("[x] : 2", true)]
    #[case::flow_key_with_quoted_bracket("[\"a]\"]: 2", true)]
    #[case::flow_key_with_apostrophe("[it's]: 2", true)]
    #[case::flow_key_nested("[[a], b]: 2", true)]
    #[case::flow_unclosed_is_multiline_node("[a,", false)]
    #[case::flow_then_text_is_ambiguous_key("[x] y", true)]
    #[case::alias_node("*y", false)]
    #[case::alias_with_comment("*y # c", false)]
    #[case::alias_key("*y : 2", true)]
    #[case::alias_then_text_is_ambiguous_key("*y: 2", true)]
    #[case::anchor_plain_node("&x w", false)]
    #[case::anchor_key("&x b: 2", true)]
    #[case::tag_plain_node("!!str w", false)]
    #[case::tag_key("!!str b: 2", true)]
    #[case::anchor_and_tag_key("&x !!str b: 2", true)]
    #[case::anchor_flow_node("&x [a]", false)]
    #[case::anchor_flow_key("&x [a]: 2", true)]
    #[case::anchor_alone_is_standalone("&x", false)]
    #[case::tag_with_comment_is_standalone("!!str # c", false)]
    #[case::anchor_and_tag_alone_is_standalone("&x !!str", false)]
    fn is_implicit_key_line_cases(#[case] line: &str, #[case] expected: bool) {
        assert_eq!(is_implicit_key_line(line), expected);
    }

    // Group F: inline_contains_mapping_key with anchors and tags
    #[rstest]
    #[case::no_anchor_plain_key("key: v", true)]
    #[case::anchor_before_key("&a key: v", true)]
    #[case::tag_before_key("!str key: v", true)]
    #[case::anchor_and_tag("&a !str key: v", true)]
    #[case::anchor_no_key("&a plain", false)]
    #[case::no_mapping_at_all("plain scalar", false)]
    fn inline_contains_mapping_key_with_anchors_and_tags(
        #[case] input: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(inline_contains_mapping_key(input), expected);
    }
}
