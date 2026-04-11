// SPDX-License-Identifier: MIT

use crate::error::Error;
use crate::limits::{MAX_ANCHOR_NAME_BYTES, MAX_TAG_LEN};
use crate::pos::Pos;

/// Scan an anchor name from `content`, returning the name slice.
///
/// `content` must begin immediately after the `&` or `*` indicator — the first
/// character is the first character of the name.  The name continues until
/// a character that is not `ns-anchor-char` (i.e., whitespace, flow indicator,
/// or end of content).
///
/// Returns `Ok(name)` where `name` is a non-empty borrowed slice of `content`.
///
/// Returns `Err` if:
/// - The name would be empty (first character is not `ns-anchor-char`).
/// - The name exceeds [`MAX_ANCHOR_NAME_BYTES`] bytes.
///
/// The caller is responsible for providing the correct [`Pos`] for error
/// reporting.
pub(in crate::event_iter) fn scan_anchor_name(
    content: &str,
    indicator_pos: Pos,
) -> Result<&str, Error> {
    use crate::chars::is_ns_anchor_char;
    let end = content
        .char_indices()
        .take_while(|&(_, ch)| is_ns_anchor_char(ch))
        .last()
        .map_or(0, |(i, ch)| i + ch.len_utf8());
    if end == 0 {
        return Err(Error {
            pos: indicator_pos,
            message: "anchor name must not be empty".into(),
        });
    }
    if end > MAX_ANCHOR_NAME_BYTES {
        return Err(Error {
            pos: indicator_pos,
            message: format!("anchor name exceeds maximum length of {MAX_ANCHOR_NAME_BYTES} bytes"),
        });
    }
    Ok(&content[..end])
}

/// Scan a tag from `content`, returning the tag slice and its byte length in `content`.
///
/// `content` must begin immediately after the `!` indicator.  The function
/// handles all four YAML 1.2 §6.8.1 tag forms:
///
/// - **Verbatim** `!<URI>` → `content` starts with `<`; returns the URI
///   (between the angle brackets) and its length including the `<` and `>`.
/// - **Primary shorthand** `!!suffix` → `content` starts with `!`; returns
///   the full `!!suffix` slice (including the leading `!` that is part of
///   `content`).
/// - **Named-handle shorthand** `!handle!suffix` → returns the full slice
///   `!handle!suffix` (the leading `!` of `handle` is in `content`).
/// - **Secondary shorthand** `!suffix` → `content` starts with a tag-char;
///   returns `!suffix` via a slice that includes one byte before `content`
///   (the caller provides `full_tag_start` for this).
/// - **Non-specific** `!` alone → `content` is empty or starts with a
///   separator; returns `"!"` as a one-byte slice of the `!` indicator.
///
/// # Parameters
///
/// - `content`: the input slice immediately after the `!` indicator character.
/// - `tag_start`: the input slice starting at the `!` (one byte before `content`).
/// - `indicator_pos`: the [`Pos`] of the `!` indicator (for error reporting).
///
/// # Returns
///
/// `Ok((tag_slice, advance_past_exclamation))` where:
/// - `tag_slice` is the borrowed slice to store in `pending_tag`.
/// - `advance_past_exclamation` is the number of bytes to advance past the
///   `!` indicator (i.e. the advance for the entire tag token, not counting
///   the `!` itself).
///
/// Returns `Err` on invalid verbatim tags (unmatched `<`, empty URI, control
/// character in URI) or when the tag length exceeds [`MAX_TAG_LEN`].
#[allow(clippy::too_many_lines)]
pub(in crate::event_iter) fn scan_tag<'i>(
    content: &'i str,
    tag_start: &'i str,
    indicator_pos: Pos,
) -> Result<(&'i str, usize), Error> {
    // ---- Verbatim tag: `!<URI>` ----
    if let Some(after_open) = content.strip_prefix('<') {
        // Scan the URI body character-by-character, validating each character
        // against YAML 1.2 §6.8.1 production [38] (ns-uri-char).  Stop at the
        // first `>` (closing delimiter).  An embedded `>` inside the URI —
        // e.g. `!<foo>bar>` — terminates the URI at `foo`; the leftover `bar>`
        // is not consumed here and is handled by the caller as continuation input.
        //
        // Valid characters: ns-uri-char single-char form, OR a `%HH` sequence.
        // Invalid: spaces, non-ASCII, `{`, `}`, `>`, `^`, `\`, `` ` ``, bare `%`.
        use crate::chars::is_ns_uri_char_single;
        let bytes = after_open.as_bytes();
        let mut byte_offset = 0usize;
        loop {
            let Some(&b) = bytes.get(byte_offset) else {
                return Err(Error {
                    pos: indicator_pos,
                    message: "verbatim tag missing closing '>'".into(),
                });
            };
            if b == b'>' {
                break; // found the closing delimiter
            }
            if b == b'%' {
                // Percent-encoded sequence: must be followed by exactly two
                // ASCII hex digits.
                let h1 = bytes
                    .get(byte_offset + 1)
                    .copied()
                    .is_some_and(|b| b.is_ascii_hexdigit());
                let h2 = bytes
                    .get(byte_offset + 2)
                    .copied()
                    .is_some_and(|b| b.is_ascii_hexdigit());
                if h1 && h2 {
                    byte_offset += 3;
                    continue;
                }
                return Err(Error {
                    pos: indicator_pos,
                    message: format!(
                        "verbatim tag URI contains invalid percent-encoding at byte offset {byte_offset}"
                    ),
                });
            }
            // Decode the next char; all valid ns-uri-char singles are ASCII,
            // so a non-ASCII leading byte will fail the predicate and be rejected.
            let ch = after_open[byte_offset..].chars().next().unwrap_or('\0');
            if !is_ns_uri_char_single(ch) {
                return Err(Error {
                    pos: indicator_pos,
                    message: format!(
                        "verbatim tag URI contains character not allowed by YAML 1.2 §6.8.1 at byte offset {byte_offset}"
                    ),
                });
            }
            byte_offset += ch.len_utf8();
        }
        let uri = &after_open[..byte_offset];
        if uri.is_empty() {
            return Err(Error {
                pos: indicator_pos,
                message: "verbatim tag URI must not be empty".into(),
            });
        }
        if uri.len() > MAX_TAG_LEN {
            return Err(Error {
                pos: indicator_pos,
                message: format!("verbatim tag URI exceeds maximum length of {MAX_TAG_LEN} bytes"),
            });
        }
        // advance = 1 (for '<') + uri.len() + 1 (for '>') bytes past the `!`
        let advance = 1 + uri.len() + 1;
        return Ok((uri, advance));
    }

    // ---- Primary handle: `!!suffix` ----
    if let Some(suffix) = content.strip_prefix('!') {
        // suffix starts after the second `!`
        let suffix_bytes = scan_tag_suffix(suffix);
        // `!!` alone with no suffix is valid (empty suffix shorthand).
        if suffix_bytes > MAX_TAG_LEN {
            return Err(Error {
                pos: indicator_pos,
                message: format!("tag exceeds maximum length of {MAX_TAG_LEN} bytes"),
            });
        }
        // tag_slice = `!!suffix` — one byte back for the first `!` (in `tag_start`)
        // plus `!` in content plus suffix.
        let tag_slice = &tag_start[..2 + suffix_bytes]; // `!` + `!` + suffix
        let advance = 1 + suffix_bytes; // past the `!` in content and suffix
        return Ok((tag_slice, advance));
    }

    // ---- Non-specific tag: bare `!` (content is empty or starts with non-tag-char) ----
    // A `%` alone (without two following hex digits) also falls here via scan_tag_suffix.
    if scan_tag_suffix(content) == 0 {
        // The tag is just `!` — a one-byte slice from `tag_start`.
        let tag_slice = &tag_start[..1];
        return Ok((tag_slice, 0)); // 0 bytes advance past `!` (nothing follows the `!`)
    }

    // ---- Named handle `!handle!suffix` or secondary handle `!suffix` ----
    // Scan tag chars until we hit a `!` (named handle delimiter) or non-tag-char.
    let mut end = 0;
    let mut found_inner_bang = false;
    for (i, ch) in content.char_indices() {
        if ch == '!' {
            // Named handle: `!handle!suffix` — scan the suffix after the inner `!`.
            found_inner_bang = true;
            end = i + 1; // include the `!`
            // Scan suffix chars (and %HH sequences) after the inner `!`.
            end += scan_tag_suffix(&content[i + 1..]);
            break;
        } else if crate::chars::is_ns_tag_char_single(ch) {
            end = i + ch.len_utf8();
        } else if ch == '%' {
            // Percent-encoded sequence: %HH.
            let pct_len = scan_tag_suffix(&content[i..]);
            if pct_len == 0 {
                break; // bare `%` without two hex digits — stop
            }
            end = i + pct_len;
        } else {
            break;
        }
    }

    if end == 0 && !found_inner_bang {
        // No tag chars at all (covered by non-specific check above, but defensive).
        let tag_slice = &tag_start[..1];
        return Ok((tag_slice, 0));
    }

    if end > MAX_TAG_LEN {
        return Err(Error {
            pos: indicator_pos,
            message: format!("tag exceeds maximum length of {MAX_TAG_LEN} bytes"),
        });
    }

    // tag_slice = `!` + content[..end] — includes the leading `!` from tag_start.
    let tag_slice = &tag_start[..=end];
    Ok((tag_slice, end))
}

/// Returns the byte length of the valid tag suffix starting at `s`.
///
/// A tag suffix is a sequence of `ns-tag-char` characters and percent-encoded
/// `%HH` sequences (YAML 1.2 §6.8.1).  Scanning stops at the first character
/// that does not satisfy either condition.
pub(in crate::event_iter) fn scan_tag_suffix(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        // Percent-encoded sequence: `%` followed by exactly two hex digits.
        if bytes.get(pos) == Some(&b'%') {
            let h1 = bytes
                .get(pos + 1)
                .copied()
                .is_some_and(|b| b.is_ascii_hexdigit());
            let h2 = bytes
                .get(pos + 2)
                .copied()
                .is_some_and(|b| b.is_ascii_hexdigit());
            if h1 && h2 {
                pos += 3;
                continue;
            }
            break;
        }
        // Safe to decode the next char: all is_ns_tag_char_single matches are ASCII,
        // so multi-byte UTF-8 chars will fail is_ns_tag_char_single and stop the scan.
        let Some(ch) = s[pos..].chars().next() else {
            break;
        };
        if crate::chars::is_ns_tag_char_single(ch) {
            pos += ch.len_utf8();
        } else {
            break;
        }
    }
    pos
}

/// Returns `true` if `handle` is a syntactically valid YAML tag handle.
///
/// Valid forms per YAML 1.2 §6.8.1 productions [89]–[92]:
/// - `!`   — primary tag handle
/// - `!!`  — secondary tag handle
/// - `!<word-chars>!` — named tag handle, where word chars are `[a-zA-Z0-9_-]`
pub(in crate::event_iter) fn is_valid_tag_handle(handle: &str) -> bool {
    match handle {
        "!" | "!!" => true,
        _ => {
            // Named handle: starts and ends with `!`, interior non-empty word chars.
            let inner = handle.strip_prefix('!').and_then(|s| s.strip_suffix('!'));
            match inner {
                Some(word) if !word.is_empty() => word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                _ => false,
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::limits::{MAX_ANCHOR_NAME_BYTES, MAX_TAG_LEN};
    use crate::pos::Pos;

    const POS: Pos = Pos::ORIGIN;

    // -----------------------------------------------------------------------
    // scan_anchor_name
    // -----------------------------------------------------------------------

    #[test]
    fn scan_anchor_name_returns_plain_word() {
        assert_eq!(scan_anchor_name("foo bar", POS).unwrap(), "foo");
    }

    #[test]
    fn scan_anchor_name_stops_at_space() {
        assert_eq!(scan_anchor_name("anchor value", POS).unwrap(), "anchor");
    }

    #[test]
    fn scan_anchor_name_stops_at_tab() {
        assert_eq!(scan_anchor_name("anchor\tvalue", POS).unwrap(), "anchor");
    }

    #[test]
    fn scan_anchor_name_stops_at_newline() {
        assert_eq!(scan_anchor_name("anchor\nvalue", POS).unwrap(), "anchor");
    }

    #[test]
    fn scan_anchor_name_stops_at_flow_indicator_comma() {
        assert_eq!(scan_anchor_name("anchor,more", POS).unwrap(), "anchor");
    }

    #[test]
    fn scan_anchor_name_stops_at_flow_indicator_open_bracket() {
        assert_eq!(scan_anchor_name("anchor[more", POS).unwrap(), "anchor");
    }

    #[test]
    fn scan_anchor_name_stops_at_flow_indicator_close_bracket() {
        assert_eq!(scan_anchor_name("anchor]more", POS).unwrap(), "anchor");
    }

    #[test]
    fn scan_anchor_name_stops_at_flow_indicator_open_brace() {
        assert_eq!(scan_anchor_name("anchor{more", POS).unwrap(), "anchor");
    }

    #[test]
    fn scan_anchor_name_stops_at_flow_indicator_close_brace() {
        assert_eq!(scan_anchor_name("anchor}more", POS).unwrap(), "anchor");
    }

    #[test]
    fn scan_anchor_name_accepts_entire_content_when_no_terminator() {
        assert_eq!(
            scan_anchor_name("anchor-name_with.dots", POS).unwrap(),
            "anchor-name_with.dots"
        );
    }

    #[test]
    fn scan_anchor_name_accepts_multibyte_unicode_chars() {
        assert_eq!(scan_anchor_name("锚点", POS).unwrap(), "锚点");
    }

    #[test]
    fn scan_anchor_name_errors_on_empty_content() {
        let err = scan_anchor_name("", POS).unwrap_err();
        assert!(err.message.contains("empty"));
    }

    #[test]
    fn scan_anchor_name_errors_when_first_char_is_space() {
        assert!(scan_anchor_name(" foo", POS).is_err());
    }

    #[test]
    fn scan_anchor_name_errors_when_first_char_is_flow_indicator() {
        assert!(scan_anchor_name("[foo", POS).is_err());
    }

    #[test]
    fn scan_anchor_name_errors_when_name_exceeds_max_bytes() {
        let long = "a".repeat(MAX_ANCHOR_NAME_BYTES + 1);
        let err = scan_anchor_name(&long, POS).unwrap_err();
        assert!(err.message.contains("exceeds maximum length"));
    }

    #[test]
    fn scan_anchor_name_accepts_name_at_exact_max_bytes() {
        let name = "a".repeat(MAX_ANCHOR_NAME_BYTES);
        let result = scan_anchor_name(&name, POS).unwrap();
        assert_eq!(result.len(), MAX_ANCHOR_NAME_BYTES);
    }

    // -----------------------------------------------------------------------
    // scan_tag_suffix
    // -----------------------------------------------------------------------

    #[test]
    fn scan_tag_suffix_empty_string() {
        assert_eq!(scan_tag_suffix(""), 0);
    }

    #[test]
    fn scan_tag_suffix_all_tag_chars() {
        assert_eq!(scan_tag_suffix("foo-bar"), 7);
    }

    #[test]
    fn scan_tag_suffix_stops_at_space() {
        assert_eq!(scan_tag_suffix("foo bar"), 3);
    }

    #[test]
    fn scan_tag_suffix_stops_at_exclamation() {
        assert_eq!(scan_tag_suffix("foo!bar"), 3);
    }

    #[test]
    fn scan_tag_suffix_stops_at_flow_indicator() {
        assert_eq!(scan_tag_suffix("foo,bar"), 3);
    }

    #[test]
    fn scan_tag_suffix_counts_percent_encoded_sequence() {
        assert_eq!(scan_tag_suffix("%2F"), 3);
    }

    #[test]
    fn scan_tag_suffix_counts_multiple_percent_encoded_sequences() {
        assert_eq!(scan_tag_suffix("%2F%41"), 6);
    }

    #[test]
    fn scan_tag_suffix_stops_at_bare_percent() {
        assert_eq!(scan_tag_suffix("%"), 0);
    }

    #[test]
    fn scan_tag_suffix_stops_at_percent_with_one_hex() {
        assert_eq!(scan_tag_suffix("%2"), 0);
    }

    #[test]
    fn scan_tag_suffix_stops_at_percent_with_non_hex() {
        assert_eq!(scan_tag_suffix("%GG"), 0);
    }

    #[test]
    fn scan_tag_suffix_mixed_tag_chars_and_percent_encoded() {
        assert_eq!(scan_tag_suffix("foo%2Fbar"), 9);
    }

    // -----------------------------------------------------------------------
    // is_valid_tag_handle
    // -----------------------------------------------------------------------

    #[test]
    fn is_valid_tag_handle_primary() {
        assert!(is_valid_tag_handle("!"));
    }

    #[test]
    fn is_valid_tag_handle_secondary() {
        assert!(is_valid_tag_handle("!!"));
    }

    #[test]
    fn is_valid_tag_handle_named_alpha() {
        assert!(is_valid_tag_handle("!foo!"));
    }

    #[test]
    fn is_valid_tag_handle_named_with_digits() {
        assert!(is_valid_tag_handle("!foo2!"));
    }

    #[test]
    fn is_valid_tag_handle_named_with_hyphen_and_underscore() {
        assert!(is_valid_tag_handle("!my-handle_1!"));
    }

    #[test]
    fn is_valid_tag_handle_named_single_char() {
        assert!(is_valid_tag_handle("!a!"));
    }

    #[test]
    fn is_valid_tag_handle_rejects_missing_trailing_bang() {
        assert!(!is_valid_tag_handle("!foo"));
    }

    #[test]
    fn is_valid_tag_handle_rejects_missing_leading_bang() {
        assert!(!is_valid_tag_handle("foo!"));
    }

    #[test]
    fn is_valid_tag_handle_rejects_empty_inner_word() {
        assert!(!is_valid_tag_handle("!!!"));
    }

    #[test]
    fn is_valid_tag_handle_rejects_non_word_char_in_inner() {
        assert!(!is_valid_tag_handle("!foo-bar.baz!"));
    }

    #[test]
    fn is_valid_tag_handle_rejects_empty_string() {
        assert!(!is_valid_tag_handle(""));
    }

    // -----------------------------------------------------------------------
    // scan_tag — helper: build tag_start / content slices from a &str
    // -----------------------------------------------------------------------

    fn scan(full: &str) -> Result<(&str, usize), crate::error::Error> {
        let content = &full[1..];
        scan_tag(content, full, POS)
    }

    // -----------------------------------------------------------------------
    // scan_tag — non-specific tag
    // -----------------------------------------------------------------------

    #[test]
    fn scan_tag_non_specific_bare_bang() {
        assert_eq!(scan("!").unwrap(), ("!", 0));
    }

    #[test]
    fn scan_tag_non_specific_bang_before_space() {
        assert_eq!(scan("! rest").unwrap(), ("!", 0));
    }

    // -----------------------------------------------------------------------
    // scan_tag — secondary handle (`!!`)
    // -----------------------------------------------------------------------

    #[test]
    fn scan_tag_secondary_handle_no_suffix() {
        assert_eq!(scan("!!").unwrap(), ("!!", 1));
    }

    #[test]
    fn scan_tag_secondary_handle_with_suffix() {
        assert_eq!(scan("!!str").unwrap(), ("!!str", 4));
    }

    #[test]
    fn scan_tag_secondary_exceeds_max_len() {
        let full = format!("!!{}", "a".repeat(MAX_TAG_LEN + 1));
        let err = scan(&full).unwrap_err();
        assert!(err.message.contains("exceeds maximum length"));
    }

    // -----------------------------------------------------------------------
    // scan_tag — secondary shorthand `!suffix` (no inner `!`)
    // -----------------------------------------------------------------------

    #[test]
    fn scan_tag_secondary_handle_suffix_only() {
        assert_eq!(scan("!foo").unwrap(), ("!foo", 3));
    }

    // -----------------------------------------------------------------------
    // scan_tag — named handle (`!handle!suffix`)
    // -----------------------------------------------------------------------

    #[test]
    fn scan_tag_named_handle() {
        assert_eq!(scan("!yaml!str").unwrap(), ("!yaml!str", 8));
    }

    #[test]
    fn scan_tag_named_handle_with_percent_encoded_suffix() {
        assert_eq!(scan("!h!%2F").unwrap(), ("!h!%2F", 5));
    }

    #[test]
    fn scan_tag_named_handle_with_empty_suffix() {
        assert_eq!(scan("!foo!").unwrap(), ("!foo!", 4));
    }

    // -----------------------------------------------------------------------
    // scan_tag — verbatim tag: happy paths
    // -----------------------------------------------------------------------

    #[test]
    fn scan_tag_verbatim_simple_uri() {
        let (uri, advance) = scan("!<tag:yaml.org,2002:str>").unwrap();
        assert_eq!(uri, "tag:yaml.org,2002:str");
        assert_eq!(advance, 23);
    }

    #[test]
    fn scan_tag_verbatim_uri_with_percent_encoded() {
        let (uri, advance) = scan("!<foo%2Fbar>").unwrap();
        assert_eq!(uri, "foo%2Fbar");
        assert_eq!(advance, 11);
    }

    #[test]
    fn scan_tag_verbatim_http_uri() {
        assert!(scan("!<http://example.com/ns/foo>").is_ok());
    }

    #[test]
    fn scan_tag_verbatim_urn_uri() {
        assert!(scan("!<urn:foo:a123,z456>").is_ok());
    }

    #[test]
    fn scan_tag_verbatim_single_char_uri() {
        let (uri, _) = scan("!<a>").unwrap();
        assert_eq!(uri, "a");
    }

    #[test]
    fn scan_tag_verbatim_percent_lowercase_hex() {
        assert!(scan("!<%ff>").is_ok());
    }

    #[test]
    fn scan_tag_verbatim_percent_uppercase_hex() {
        assert!(scan("!<%FF>").is_ok());
    }

    #[test]
    fn scan_tag_verbatim_percent_41_is_valid() {
        let (uri, _) = scan("!<%41>").unwrap();
        assert_eq!(uri, "%41");
    }

    #[test]
    fn scan_tag_verbatim_accepts_uri_at_exact_max_len() {
        let uri_body = "a".repeat(MAX_TAG_LEN);
        let full = format!("!<{uri_body}>");
        let (uri, _) = scan(&full).unwrap();
        assert_eq!(uri.len(), MAX_TAG_LEN);
    }

    #[test]
    fn scan_tag_verbatim_embedded_close_delimiter_terminates_uri() {
        // `!<foo>bar>` terminates the URI at the first `>`. The leftover `bar>`
        // is not consumed by scan_tag — the caller handles it as continuation input.
        let (uri, advance) = scan("!<foo>bar>").unwrap();
        assert_eq!(uri, "foo");
        assert_eq!(advance, 5); // 1 (`<`) + 3 (`foo`) + 1 (`>`)
    }

    // -----------------------------------------------------------------------
    // scan_tag — verbatim tag: error paths
    // -----------------------------------------------------------------------

    #[test]
    fn scan_tag_verbatim_rejects_empty_uri() {
        let err = scan("!<>").unwrap_err();
        assert!(err.message.contains("empty"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_unclosed() {
        let err = scan("!<noclose").unwrap_err();
        assert!(err.message.contains("missing closing"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_bare_percent_in_uri() {
        let err = scan("!<%GG>").unwrap_err();
        assert!(err.message.contains("percent-encoding"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_percent_one_hex_digit() {
        let err = scan("!<%4>").unwrap_err();
        assert!(err.message.contains("percent-encoding"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_percent_at_end_no_closing() {
        let err = scan("!<%41").unwrap_err();
        assert!(err.message.contains("missing closing"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_bare_percent_at_buffer_end() {
        let err = scan("!<%>").unwrap_err();
        assert!(err.message.contains("percent-encoding"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_space_in_uri() {
        let err = scan("!<foo bar>").unwrap_err();
        assert!(err.message.contains("§6.8.1"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_non_ascii_two_byte_char() {
        // é is U+00E9, 2-byte UTF-8 — leading byte 0xC3 is not ns-uri-char-single
        let err = scan("!<foo\u{00E9}>").unwrap_err();
        assert!(err.message.contains("§6.8.1"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_non_ascii_four_byte_char() {
        // U+1F600 is 4-byte UTF-8 — not ns-uri-char-single
        let err = scan("!<foo\u{1F600}>").unwrap_err();
        assert!(err.message.contains("§6.8.1"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_multibyte_after_valid_ascii() {
        let err = scan("!<abc\u{00E9}def>").unwrap_err();
        assert!(err.message.contains("§6.8.1"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_null_byte() {
        let err = scan("!<foo\x00bar>").unwrap_err();
        assert!(err.message.contains("§6.8.1"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_control_char_0x1f() {
        let err = scan("!<foo\x1Fbar>").unwrap_err();
        assert!(err.message.contains("§6.8.1"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_open_brace() {
        let err = scan("!<foo{bar>").unwrap_err();
        assert!(err.message.contains("§6.8.1"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_close_brace() {
        let err = scan("!<foo}bar>").unwrap_err();
        assert!(err.message.contains("§6.8.1"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_caret() {
        let err = scan("!<foo^bar>").unwrap_err();
        assert!(err.message.contains("§6.8.1"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_backslash() {
        let err = scan("!<foo\\bar>").unwrap_err();
        assert!(err.message.contains("§6.8.1"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_backtick() {
        let err = scan("!<foo`bar>").unwrap_err();
        assert!(err.message.contains("§6.8.1"));
    }

    #[test]
    fn scan_tag_verbatim_rejects_uri_exceeding_max_len() {
        let uri_body = "a".repeat(MAX_TAG_LEN + 1);
        let full = format!("!<{uri_body}>");
        let err = scan(&full).unwrap_err();
        assert!(err.message.contains("exceeds maximum length"));
    }
}
