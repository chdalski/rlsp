// SPDX-License-Identifier: MIT

use std::fmt::Write as _;

use rlsp_fmt::{Doc, concat, hard_line, indent, text};
use rlsp_yaml_parser::{Chomp, ScalarStyle};

use crate::server::YamlVersion;

use super::options::YamlFormatOptions;

/// Returns `true` if the tag string is a YAML Core Schema tag.
pub(super) fn is_core_schema_tag(tag: &str) -> bool {
    tag.starts_with("tag:yaml.org,2002:")
}

/// Format a tag string for emission in YAML output.
///
/// Tags stored in the AST fall into two forms:
/// - Handle-based: already start with `!` (e.g. `!circle`, `!!str`). Emit as-is.
/// - URI-based: a bare URI without a `!` prefix (e.g. `tag:example.com:shape`).
///   These must be wrapped as `!<uri>` — the verbatim tag form in YAML syntax.
pub(super) fn format_tag(tag: &str) -> String {
    if tag.starts_with('!') {
        tag.to_string()
    } else {
        format!("!<{tag}>")
    }
}

/// Convert a string scalar to a Doc, quoting as necessary.
///
/// When `in_key` is `true`, the `single_quote` option is ignored — keys are
/// never wrapped in single quotes by style preference alone.
pub(super) fn string_to_doc(s: &str, options: &YamlFormatOptions, in_key: bool) -> Doc {
    if needs_quoting(s, options.yaml_version) {
        // Must quote — use the preferred style.
        if options.single_quote && !s.contains('\'') {
            text(format!("'{s}'"))
        } else {
            // Double-quote and escape.
            text(format!("\"{}\"", escape_double_quoted(s)))
        }
    } else if options.single_quote && !in_key {
        text(format!("'{s}'"))
    } else {
        // Plain — no quotes needed.
        text(s.to_string())
    }
}

/// Returns true if a plain scalar value would be ambiguous in a flow collection
/// context — specifically if it contains characters that serve as flow delimiters
/// (`,`, `[`, `]`, `{`, `}`).
pub(super) fn needs_flow_quoting(s: &str) -> bool {
    s.contains([',', '[', ']', '{', '}'])
}

/// Returns true if a string value requires quoting to avoid YAML ambiguity.
///
/// The `version` parameter controls whether YAML 1.1-only boolean keywords
/// (`yes`, `no`, `on`, `off` and their capitalised variants) count as reserved.
/// In YAML 1.2 those words are plain strings and do not need quoting.
pub(super) fn needs_quoting(s: &str, version: YamlVersion) -> bool {
    if s.is_empty() {
        return true;
    }

    // All-whitespace values would be trimmed to nothing by YAML's flow-scalar
    // trimming rules, so they must be quoted.
    if s.chars().all(char::is_whitespace) {
        return true;
    }

    // Values with leading or trailing whitespace lose those spaces when emitted as
    // a plain scalar and re-parsed — YAML trims leading and trailing whitespace from
    // plain scalars, so the formatter output would not be idempotent.
    if s.starts_with(char::is_whitespace) || s.ends_with(char::is_whitespace) {
        return true;
    }

    // Values that are reserved YAML keywords in all versions.
    let always_reserved = matches!(
        s,
        "null" | "~" | "true" | "false" | "Null" | "NULL" | "True" | "TRUE" | "False" | "FALSE"
    );

    // Values that are reserved only under YAML 1.1.
    let v1_1_reserved = version == YamlVersion::V1_1
        && matches!(
            s,
            "yes" | "no" | "on" | "off" | "Yes" | "No" | "On" | "Off" | "YES" | "NO" | "ON" | "OFF"
        );

    // A string with an embedded newline cannot be emitted as a plain scalar.
    // In YAML, plain scalars that span multiple lines fold line breaks to spaces
    // (unless a blank line separates them, in which case a `\n` is preserved).
    // However the formatter emits scalars as single-line plain text, so a value
    // containing `\n` would be split across lines and misinterpreted (the second
    // line would be parsed as a new key or value at the wrong indentation level).
    if s.contains('\n') {
        return true;
    }

    always_reserved
        || v1_1_reserved
        || looks_like_number(s)
        || s.starts_with(|c: char| {
            matches!(
                c,
                ':' | '#'
                    | '&'
                    | '*'
                    | '?'
                    | '|'
                    | '-'
                    | '<'
                    | '>'
                    | '='
                    | '!'
                    | '%'
                    | '@'
                    | '`'
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | '"'
                    | '\''
            )
        })
        || s.contains(": ")
        || s.contains(" #")
        || s.starts_with("- ")
        || s.starts_with("--- ")
        || s == "---"
        || s == "..."
}

/// Returns true if the string looks like a YAML number (integer or float).
pub(super) fn looks_like_number(s: &str) -> bool {
    s.parse::<i64>().is_ok()
        || s.parse::<f64>().is_ok()
        || matches!(
            s,
            ".inf" | ".Inf" | ".INF" | "+.inf" | "-.inf" | ".nan" | ".NaN" | ".NAN"
        )
}

/// Returns `true` if the decoded string value contains characters that require
/// double-quoting to represent in YAML — control characters, backslash, or any
/// other C0 character (U+0000–U+001F).
///
/// This check must happen *before* `needs_quoting` in the `DoubleQuoted` branch
/// so that decoded values with raw control bytes are never emitted as plain
/// scalars (which would produce unparseable YAML).
pub(super) fn requires_double_quoting(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c, '\\')
            || (c as u32) <= 0x1F
            || c == '\u{0085}' // NEL
            || c == '\u{2028}' // line separator
            || c == '\u{2029}' // paragraph separator
    })
}

/// Escape a string for use in a double-quoted YAML scalar.
///
/// Handles all YAML 1.2 §5.7 named escapes and falls back to `\xNN` hex
/// notation for remaining C0 control characters.
pub(super) fn escape_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x00' => out.push_str("\\0"),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\x0B' => out.push_str("\\v"),
            '\x0C' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '\x1B' => out.push_str("\\e"),
            '\u{0085}' => out.push_str("\\N"),
            '\u{00A0}' => out.push_str("\\_"),
            '\u{2028}' => out.push_str("\\L"),
            '\u{2029}' => out.push_str("\\P"),
            c if (c as u32) <= 0x1F => {
                // Remaining C0 controls as \xNN
                let _ = write!(out, "\\x{:02X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Convert a block scalar to Doc using hard lines.
///
/// The parser preserves the original chomping indicator, so we emit it
/// faithfully (`|`, `|-`, `|+`, `>`, `>-`, `>+`).
///
/// Content lines are wrapped in `indent()` so the Wadler-Lindig printer
/// indents them one level relative to the surrounding context.
///
/// When any content line begins with a space or tab, an explicit indentation
/// indicator digit equal to `tab_width` is appended to the header (e.g. `|2`
/// or `>2`).  Without it the YAML parser would auto-detect indentation from the
/// first content line and misparse any line whose content starts with a leading
/// space.
///
/// For literal scalars, blank lines (empty strings from `str::lines()`) are
/// omitted here — `attach_comments` re-inserts them from the original input
/// when it matches content signatures.
///
/// For folded scalars, blank lines must be emitted explicitly because folding
/// semantics require them to represent embedded newlines in the decoded value.
/// N blank lines between two content lines produces N newlines in the value;
/// without them, adjacent content lines would fold into a single space on
/// re-parse, making the output non-idempotent.
pub(super) fn repr_block_to_doc(s: &str, style: ScalarStyle, tab_width: usize) -> Doc {
    // Detect whether the first non-empty content line requires an explicit
    // indentation indicator digit.
    //
    // Case 1 — leading space: a leading space in the decoded value means the
    // YAML parser would auto-detect a higher indentation level than intended
    // (treating the extra space as indentation rather than content).
    //
    // Case 2 — whitespace-only line: when the first non-empty content line
    // consists entirely of whitespace characters (spaces and/or tabs), YAML
    // auto-detection is unreliable.  The parser sees the indentation byte count
    // as only one level (e.g. 1 space) rather than the formatter's `tab_width`,
    // causing the re-parsed value to have an extra leading space prepended.
    // An explicit indicator digit forces the correct indent level on re-parse.
    //
    // Only the first non-empty content line matters for auto-detection.
    let needs_indent_indicator = s
        .lines()
        .find(|l| !l.is_empty())
        .is_some_and(|l| l.starts_with(' ') || l.chars().all(char::is_whitespace));

    let base_header = match style {
        ScalarStyle::Literal(Chomp::Clip) => "|",
        ScalarStyle::Literal(Chomp::Strip) => "|-",
        ScalarStyle::Literal(Chomp::Keep) => "|+",
        ScalarStyle::Folded(Chomp::Clip) => ">",
        ScalarStyle::Folded(Chomp::Strip) => ">-",
        ScalarStyle::Folded(Chomp::Keep) => ">+",
        ScalarStyle::Plain | ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => "",
    };

    let header = if needs_indent_indicator && !base_header.is_empty() {
        // Insert the digit between the block indicator character and any chomp
        // indicator: `|` → `|2`, `|-` → `|2-`, `>+` → `>2+`.
        let (block_char, chomp_char) = base_header.split_at(1);
        format!("{block_char}{tab_width}{chomp_char}")
    } else {
        base_header.to_string()
    };

    let mut parts = vec![text(header)];

    if matches!(style, ScalarStyle::Folded(_)) {
        // For folded scalars, blank lines encode the newline structure of the
        // decoded value.  We split on `\n` (not `.lines()`) to count the empty
        // segments that represent extra newlines in the decoded value.
        //
        // The number of blank lines to emit between two consecutive content
        // segments depends on whether either segment is "more-indented" (starts
        // with a space or tab, meaning it was at a greater indentation level in
        // the original YAML):
        //
        //   - Both at base level (no leading whitespace): the line break between
        //     them would be folded to a space on re-parse, so we need one blank
        //     line per `\n` between them.  K empty segments → K+1 `\n`s → K+1
        //     blanks.
        //
        //   - Either side is more-indented: the more-indented line's own line
        //     break is "free" (the parser preserves it without needing a blank).
        //     One blank is therefore "absorbed" by the free line break, so we
        //     emit max(0, K) blanks for K empty segments (= K+1 `\n`s → K blanks).
        //
        // Trailing `\n` from Clip chomp is implicit — strip the trailing empty
        // segment so it is not counted as an extra blank.
        let mut segments: Vec<&str> = s.split('\n').collect();
        if segments.last() == Some(&"") {
            segments.pop();
        }

        let mut pending_empty: usize = 0;
        let mut prev_content: Option<&str> = None;

        for seg in &segments {
            if seg.is_empty() {
                pending_empty += 1;
            } else {
                if let Some(prev) = prev_content {
                    // Determine whether either the previous or the current
                    // content line is "more-indented" (has a leading space or
                    // tab beyond the block scalar's base indentation level).
                    // YAML treats lines starting with a tab as more-indented
                    // for folding purposes — their line break is preserved
                    // (not folded to a space) and no extra blank line is needed
                    // to represent the transition.
                    let prev_more = prev.starts_with([' ', '\t']);
                    let curr_more = seg.starts_with([' ', '\t']);
                    let either_more = prev_more || curr_more;

                    let blank_count = if either_more {
                        // Free line-break absorbed; only emit extra blanks.
                        pending_empty
                    } else {
                        // Both at base level: each `\n` needs a blank.
                        pending_empty + 1
                    };
                    for _ in 0..blank_count {
                        parts.push(hard_line());
                    }
                }
                pending_empty = 0;
                parts.push(indent(concat(vec![hard_line(), text(seg.to_string())])));
                prev_content = Some(seg);
            }
        }
    } else {
        // For literal scalars, blank lines are omitted here; attach_comments
        // re-inserts them from the original source, preserving blank-line
        // semantics without producing trailing-whitespace lines or double blanks.
        for line_str in s.lines() {
            if !line_str.is_empty() {
                parts.push(indent(concat(vec![
                    hard_line(),
                    text(line_str.to_string()),
                ])));
            }
        }
    }

    concat(parts)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    // ---- Group: escape_double_quoted unit tests ----

    // EDQ1: Newline, carriage return, and tab are escaped.
    // EDQ2: Double-quote and backslash are escaped.
    #[rstest]
    #[case::newline_escaped("a\nb", "a\\nb")]
    #[case::carriage_return_escaped("a\rb", "a\\rb")]
    #[case::tab_escaped("a\tb", "a\\tb")]
    #[case::double_quote_escaped("say \"hi\"", "say \\\"hi\\\"")]
    #[case::backslash_escaped("a\\b", "a\\\\b")]
    fn escape_double_quoted_escapes(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(escape_double_quoted(input), expected);
    }

    // ---- Group: needs_quoting — returns true ----

    // NQ1 (empty string), NQ2 (numeric), and version-aware cases.
    #[rstest]
    #[case::on_v1_1("on", YamlVersion::V1_1)]
    #[case::yes_v1_1("yes", YamlVersion::V1_1)]
    #[case::off_v1_1("off", YamlVersion::V1_1)]
    #[case::no_v1_1("no", YamlVersion::V1_1)]
    #[case::true_v1_1("true", YamlVersion::V1_1)]
    #[case::true_v1_2("true", YamlVersion::V1_2)]
    #[case::null_v1_1("null", YamlVersion::V1_1)]
    #[case::null_v1_2("null", YamlVersion::V1_2)]
    #[case::uppercase_yes_v1_1("YES", YamlVersion::V1_1)]
    #[case::empty_string_v1_1("", YamlVersion::V1_1)]
    #[case::empty_string_v1_2("", YamlVersion::V1_2)]
    #[case::numeric_123_v1_1("123", YamlVersion::V1_1)]
    #[case::numeric_123_v1_2("123", YamlVersion::V1_2)]
    #[case::numeric_3_14_v1_2("3.14", YamlVersion::V1_2)]
    fn needs_quoting_returns_true(#[case] word: &str, #[case] version: YamlVersion) {
        assert!(
            needs_quoting(word, version),
            "{word:?} should require quoting in {version:?}"
        );
    }

    // ---- Group: needs_quoting — returns false ----

    #[rstest]
    #[case::on_v1_2("on", YamlVersion::V1_2)]
    #[case::yes_v1_2("yes", YamlVersion::V1_2)]
    #[case::off_v1_2("off", YamlVersion::V1_2)]
    #[case::no_v1_2("no", YamlVersion::V1_2)]
    #[case::uppercase_yes_v1_2("YES", YamlVersion::V1_2)]
    fn needs_quoting_returns_false(#[case] word: &str, #[case] version: YamlVersion) {
        assert!(
            !needs_quoting(word, version),
            "{word:?} should not require quoting in {version:?}"
        );
    }
}
