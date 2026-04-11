// SPDX-License-Identifier: MIT

/// True when `trimmed` (content after stripping leading spaces) represents
/// an implicit mapping key: it contains `: `, `:\t`, or ends with `:`.
pub fn is_implicit_mapping_line(trimmed: &str) -> bool {
    find_value_indicator_offset(trimmed).is_some()
}

/// Returns `true` when `s` is a block structure indicator that cannot appear
/// at tab-based indentation: a block sequence entry (`-` followed by
/// whitespace or EOL), an explicit key marker (`?` followed by whitespace or
/// EOL), or an implicit mapping key (contains a `:` value indicator).
///
/// Used to detect tab-as-block-indentation violations (YAML 1.2 §6.1).
pub fn is_tab_indented_block_indicator(s: &str) -> bool {
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
pub fn inline_contains_mapping_key(inline: &str) -> bool {
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
pub fn find_value_indicator_offset(trimmed: &str) -> Option<usize> {
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
    let mut i = 0;
    let mut prev_was_space = false; // tracks whether the previous byte was whitespace
    while let Some(&ch) = bytes.get(i) {
        // Stop at an unquoted `#` preceded by whitespace (or at position 0):
        // YAML 1.2 §6.6 — a `#` after whitespace begins a comment; any `:` that
        // follows is inside the comment and cannot be a value indicator.
        if ch == b'#' && (i == 0 || prev_was_space) {
            return None;
        }

        // Skip double-quoted span (handles `\"` escapes).
        // Only treat `"` as a quoted-span delimiter when it appears at the
        // very start of the key (i == 0) — in YAML, `"key": value` has a
        // double-quoted key, but `a"b": value` has a literal `"` inside a
        // plain scalar key, which must not be mistaken for a quoted span.
        // After a quoted span, `prev_was_space` is false — a closing `"` is
        // not whitespace.
        if ch == b'"' && i == 0 {
            i += 1; // skip opening `"`
            while let Some(&inner) = bytes.get(i) {
                match inner {
                    b'\\' => i += 2, // skip escape sequence (two bytes)
                    b'"' => {
                        i += 1; // skip closing `"`
                        break;
                    }
                    _ => i += 1,
                }
            }
            prev_was_space = false;
            continue;
        }

        // Skip single-quoted span (handles `''` escape).
        // Same rule: only treat `'` as a quoted-span delimiter at position 0.
        // After a quoted span, `prev_was_space` is false — a closing `'` is
        // not whitespace.
        if ch == b'\'' && i == 0 {
            i += 1; // skip opening `'`
            while let Some(&inner) = bytes.get(i) {
                i += 1;
                if inner == b'\'' {
                    // `''` is an escaped single-quote; a lone `'` ends the span.
                    if bytes.get(i).copied() == Some(b'\'') {
                        i += 1; // consume the second `'` of the `''` escape
                    } else {
                        break; // lone `'` — end of quoted span
                    }
                }
            }
            prev_was_space = false;
            continue;
        }

        if ch == b':' {
            match bytes.get(i + 1).copied() {
                None | Some(b' ' | b'\t' | b'\n' | b'\r') => return Some(i),
                _ => {}
            }
        }

        prev_was_space = ch == b' ' || ch == b'\t';

        // Multi-byte char: advance by UTF-8 lead-byte length.
        i += if ch < 0x80 {
            1
        } else if ch & 0xE0 == 0xC0 {
            2
        } else if ch & 0xF0 == 0xE0 {
            3
        } else {
            4
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{find_value_indicator_offset, is_implicit_mapping_line};

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
}
