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
pub fn scan_anchor_name(content: &str, indicator_pos: Pos) -> Result<&str, Error> {
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
pub fn scan_tag<'i>(
    content: &'i str,
    tag_start: &'i str,
    indicator_pos: Pos,
) -> Result<(&'i str, usize), Error> {
    // ---- Verbatim tag: `!<URI>` ----
    if let Some(after_open) = content.strip_prefix('<') {
        // Scan the URI body character-by-character, validating each character
        // against YAML 1.2 §6.8.1 production [38] (ns-uri-char).  Stop at the
        // first `>` (closing delimiter) or reject immediately on any invalid
        // character.  This order ensures that an embedded `>` inside the URI —
        // e.g. `!<foo>bar>` — is caught as an invalid character rather than
        // silently truncating the URI at the first `>`.
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
pub fn scan_tag_suffix(s: &str) -> usize {
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
pub fn is_valid_tag_handle(handle: &str) -> bool {
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
