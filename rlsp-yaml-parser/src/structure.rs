// SPDX-License-Identifier: MIT

//! YAML 1.2 §6 structural productions [63]–[103].
//!
//! Covers indentation, separation spaces, comments, directives, and node
//! properties.  Each function is named after the spec production and
//! cross-referenced by its production number in a `// [N]` comment.

use crate::chars::{
    b_break, nb_char, ns_anchor_char, ns_dec_digit, ns_tag_char, ns_uri_char, ns_word_char, s_white,
};
use crate::combinator::{
    Context, Parser, State, alt, char_parser, many0, many1, neg_lookahead, opt, satisfy, seq,
    token, wrap_tokens,
};
use crate::token::Code;

// ---------------------------------------------------------------------------
// §6.1 – Indentation [63]–[65]
// ---------------------------------------------------------------------------

/// [63] s-indent(n) — exactly n spaces.
///
/// Note: `s-indent(n)` is exact — it succeeds only when there are exactly n
/// space characters and the next character is not an additional space.  This
/// differs from "at least n spaces".
#[must_use]
pub fn s_indent(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        // Count leading spaces without consuming.
        let count = i32::try_from(state.input.chars().take_while(|&ch| ch == ' ').count())
            .unwrap_or(i32::MAX);
        if count != n {
            return crate::combinator::Reply::Failure;
        }
        // Consume exactly n spaces.
        let mut s = state;
        for _ in 0..n {
            s = s.advance(' ');
        }
        crate::combinator::Reply::Success {
            tokens: Vec::new(),
            state: s,
        }
    })
}

/// s-indent-content(n) — require at least n spaces, consume exactly n.
///
/// Used for block scalar content lines per spec [63]: `s-indent(n)` consumes
/// n spaces even when more are available. Extra spaces become content for
/// `nb-char+` to consume.  Unlike the struct-level `s-indent`, this succeeds
/// when count ≥ n rather than count == n.
#[must_use]
pub fn s_indent_content(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        let count = i32::try_from(state.input.chars().take_while(|&ch| ch == ' ').count())
            .unwrap_or(i32::MAX);
        if count < n {
            return crate::combinator::Reply::Failure;
        }
        let mut s = state;
        for _ in 0..n {
            s = s.advance(' ');
        }
        crate::combinator::Reply::Success {
            tokens: Vec::new(),
            state: s,
        }
    })
}

/// [64] s-indent(<n) — fewer than n spaces (0..n-1 spaces).
#[must_use]
pub fn s_indent_lt(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        let count = i32::try_from(state.input.chars().take_while(|&ch| ch == ' ').count())
            .unwrap_or(i32::MAX);
        if count >= n {
            return crate::combinator::Reply::Failure;
        }
        let mut s = state;
        for _ in 0..count {
            s = s.advance(' ');
        }
        crate::combinator::Reply::Success {
            tokens: Vec::new(),
            state: s,
        }
    })
}

/// [65] s-indent(≤n) — at most n spaces (0..n spaces).
#[must_use]
pub fn s_indent_le(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        let count = i32::try_from(state.input.chars().take_while(|&ch| ch == ' ').count())
            .unwrap_or(i32::MAX);
        if count > n {
            return crate::combinator::Reply::Failure;
        }
        let mut s = state;
        for _ in 0..count {
            s = s.advance(' ');
        }
        crate::combinator::Reply::Success {
            tokens: Vec::new(),
            state: s,
        }
    })
}

/// s-indent(≥n) — at least n spaces (consumes all leading spaces when ≥ n).
///
/// Used for flow-context line prefixes where any indentation at or beyond
/// the minimum level is valid (e.g., continuation lines inside nested flow
/// collections).
#[must_use]
pub fn s_indent_ge(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        let count = i32::try_from(state.input.chars().take_while(|&ch| ch == ' ').count())
            .unwrap_or(i32::MAX);
        if count < n {
            return crate::combinator::Reply::Failure;
        }
        let mut s = state;
        for _ in 0..count {
            s = s.advance(' ');
        }
        crate::combinator::Reply::Success {
            tokens: Vec::new(),
            state: s,
        }
    })
}

// ---------------------------------------------------------------------------
// §6.2 – Separation spaces [66]–[68]
// ---------------------------------------------------------------------------

/// [66] s-separate-in-line — one or more white chars, or at start of line.
///
/// Consumes any leading whitespace.  If at column 0 and no whitespace is
/// present, succeeds with zero consumption (start-of-line is implicitly
/// separated).
#[must_use]
pub fn s_separate_in_line<'i>() -> Parser<'i> {
    Box::new(|state| {
        // Try to consume whitespace first.
        let whitespace_result = many1(s_white())(state.clone());
        match whitespace_result {
            crate::combinator::Reply::Success { .. } => whitespace_result,
            crate::combinator::Reply::Failure if state.pos.column == 0 => {
                // At start of line with no whitespace — zero-consumption success.
                crate::combinator::Reply::Success {
                    tokens: Vec::new(),
                    state,
                }
            }
            other @ (crate::combinator::Reply::Failure | crate::combinator::Reply::Error(_)) => {
                other
            }
        }
    })
}

/// [67] s-block-line-prefix(n) — indentation for block contexts: exactly n spaces.
#[must_use]
pub fn s_block_line_prefix(n: i32) -> Parser<'static> {
    s_indent(n)
}

/// [68] s-flow-line-prefix(n) — indentation for flow contexts: exactly
/// n-space indent then optional in-line separation.
#[must_use]
pub fn s_flow_line_prefix(n: i32) -> Parser<'static> {
    seq(s_indent(n), opt(s_separate_in_line()))
}

/// s-flow-line-prefix-ge(n) — like s-flow-line-prefix but accepts ≥ n spaces.
///
/// Flow collections allow continuation lines with more than n spaces of
/// indentation. Used when separating content inside flow sequences and
/// mappings where deeper indentation is permitted.
#[must_use]
pub fn s_flow_line_prefix_ge(n: i32) -> Parser<'static> {
    seq(s_indent_ge(n), opt(s_separate_in_line()))
}

/// s-separate-lines-flow(n) — like s-separate-lines but for flow contexts.
///
/// Accepts ≥ n spaces on continuation lines, allowing deeply-indented
/// content inside flow collections.
#[must_use]
pub fn s_separate_lines_flow(n: i32) -> Parser<'static> {
    alt(
        seq(s_l_comments(), s_flow_line_prefix_ge(n)),
        s_separate_in_line(),
    )
}

// ---------------------------------------------------------------------------
// §6.3 – Line prefixes [69]
// ---------------------------------------------------------------------------

/// [69] s-line-prefix(n,c) — dispatch on context.
///
/// Block contexts use `s-block-line-prefix(n)`, flow contexts use
/// `s-flow-line-prefix(n)`.
#[must_use]
pub fn s_line_prefix(n: i32, c: Context) -> Parser<'static> {
    match c {
        Context::BlockOut | Context::BlockIn => s_block_line_prefix(n),
        Context::FlowOut | Context::FlowIn | Context::BlockKey | Context::FlowKey => {
            s_flow_line_prefix(n)
        }
    }
}

// ---------------------------------------------------------------------------
// §6.4 – Empty lines [70]–[71]
// ---------------------------------------------------------------------------

/// [70] l-empty(n,c) — an empty line: optional prefix then a line break.
///
/// Per spec: `( s-line-prefix(n,c) | s-indent(≤n) ) b-as-line-feed`.
/// For block contexts, lines must have at most n leading spaces (or exactly n
/// for the prefix path). For flow contexts, lines may have more than n spaces
/// because `s-flow-line-prefix` allows additional separation whitespace.
/// Trailing tabs are allowed on empty lines in all contexts.
#[must_use]
pub fn l_empty(n: i32, c: Context) -> Parser<'static> {
    seq(
        alt(s_line_prefix(n, c), s_indent_le(n)),
        seq(many0(s_white()), b_break()),
    )
}

/// [71] b-l-trimmed(n,c) — a line break followed by zero or more empty lines.
#[must_use]
pub fn b_l_trimmed(n: i32, c: Context) -> Parser<'static> {
    seq(b_break(), many1(l_empty(n, c)))
}

// ---------------------------------------------------------------------------
// §6.5 – Line folding [72]–[74]
// ---------------------------------------------------------------------------

/// [72] b-as-space — a line break treated as a single space.
#[must_use]
pub fn b_as_space<'i>() -> Parser<'i> {
    b_break()
}

/// [73] b-l-folded(n,c) — folded line break: trimmed or treated as space.
#[must_use]
pub fn b_l_folded(n: i32, c: Context) -> Parser<'static> {
    alt(b_l_trimmed(n, c), b_as_space())
}

/// Flow-context empty line: any whitespace then break.
/// Used in `s-flow-folded` because flow empty lines can have any indentation.
fn l_empty_flow(n: i32) -> Parser<'static> {
    seq(
        alt(s_flow_line_prefix_ge(n), s_indent_le(n)),
        seq(many0(s_white()), b_break()),
    )
}

/// [74] s-flow-folded(n) — flow scalar folding: optional whitespace,
/// folded break, then flow line prefix.
///
/// Uses a flow-specific empty line matcher that allows deeper indentation
/// on blank lines within flow contexts.
#[must_use]
pub fn s_flow_folded(n: i32) -> Parser<'static> {
    // b-l-folded(n, FlowIn) inlined with flow-specific l-empty:
    // alt(seq(b_break(), many1(l_empty_flow(n))), b_as_space())
    let b_l_folded_flow = alt(seq(b_break(), many1(l_empty_flow(n))), b_as_space());
    seq(
        opt(s_separate_in_line()),
        seq(b_l_folded_flow, s_flow_line_prefix_ge(n)),
    )
}

// ---------------------------------------------------------------------------
// §6.6 – Comments [75], [78]–[81]
// ---------------------------------------------------------------------------

/// [75] c-nb-comment-text — `#` followed by any non-break characters.
#[must_use]
pub fn c_nb_comment_text<'i>() -> Parser<'i> {
    seq(char_parser('#'), many0(nb_char()))
}

/// [78] b-comment — end of a comment: a line break or end-of-file.
#[must_use]
pub fn b_comment<'i>() -> Parser<'i> {
    Box::new(|state| {
        if state.input.is_empty() {
            // EOF is a valid comment terminator.
            return crate::combinator::Reply::Success {
                tokens: Vec::new(),
                state,
            };
        }
        b_break()(state)
    })
}

/// [79] s-b-comment — optional inline comment terminated by a break or EOF.
///
/// Emits `BeginComment`/`EndComment` token pair only when a `#`-prefixed
/// comment is present.  A bare newline (no comment) succeeds with no tokens.
#[must_use]
pub fn s_b_comment<'i>() -> Parser<'i> {
    use crate::combinator::Reply;
    Box::new(|state| {
        // Optional: s-separate-in-line then optional c-nb-comment-text.
        let (has_sep, after_ws) = match s_separate_in_line()(state.clone()) {
            Reply::Success { state: s, .. } => (true, s),
            Reply::Failure | Reply::Error(_) => (false, state.clone()),
        };
        // Try comment text (#...) — only when separation whitespace was present.
        if has_sep && after_ws.peek() == Some('#') {
            let comment_body = seq(
                token(Code::Indicator, char_parser('#')),
                token(Code::Text, many0(nb_char())),
            );
            if let Reply::Success {
                tokens: body_tokens,
                state: after_body,
            } = comment_body(after_ws.clone())
            {
                // b-comment after comment text.
                if let Reply::Success {
                    tokens: break_tokens,
                    state: final_state,
                } = b_comment()(after_body)
                {
                    let mut all = vec![crate::token::Token {
                        code: Code::BeginComment,
                        pos: final_state.pos,
                        text: "",
                    }];
                    all.extend(body_tokens);
                    all.push(crate::token::Token {
                        code: Code::EndComment,
                        pos: final_state.pos,
                        text: "",
                    });
                    all.extend(break_tokens);
                    return Reply::Success {
                        tokens: all,
                        state: final_state,
                    };
                }
            }
        }
        // No comment text — consume optional whitespace then b-comment.
        b_comment()(after_ws)
    })
}

/// [80] l-comment — a full comment line: whitespace + `#` + text + break.
///
/// Emits `BeginComment`/`EndComment` around the comment content.
#[must_use]
pub fn l_comment<'i>() -> Parser<'i> {
    wrap_tokens(
        Code::BeginComment,
        Code::EndComment,
        seq(
            s_separate_in_line(),
            seq(
                token(Code::Indicator, char_parser('#')),
                seq(token(Code::Text, many0(nb_char())), b_comment()),
            ),
        ),
    )
}

/// [81] s-l-comments — inline comment (or break) followed by optional comment
/// or blank lines.
///
/// Blank lines (`b-break` with no content) are accepted between comment lines
/// so that `s-l-comments` correctly spans multi-line comment blocks with gaps.
#[must_use]
pub fn s_l_comments<'i>() -> Parser<'i> {
    seq(
        s_b_comment(),
        many0(alt(
            l_comment(),
            alt(seq(many0(s_white()), b_break()), b_break()),
        )),
    )
}

// ---------------------------------------------------------------------------
// §6.8 – Directives [82]–[95]
// ---------------------------------------------------------------------------

/// [84] ns-directive-name — one or more non-space characters.
#[must_use]
pub fn ns_directive_name<'i>() -> Parser<'i> {
    many1(satisfy(|ch| !matches!(ch, ' ' | '\t' | '\n' | '\r')))
}

/// [85] ns-directive-parameter — one or more non-space characters.
#[must_use]
pub fn ns_directive_parameter<'i>() -> Parser<'i> {
    many1(satisfy(|ch| !matches!(ch, ' ' | '\t' | '\n' | '\r')))
}

/// [83] ns-reserved-directive — a directive name followed by optional params.
#[must_use]
pub fn ns_reserved_directive<'i>() -> Parser<'i> {
    seq(
        ns_directive_name(),
        many0(seq(many1(s_white()), ns_directive_parameter())),
    )
}

/// [87] ns-yaml-version — `<dec-digit>+ '.' <dec-digit>+`.
#[must_use]
pub fn ns_yaml_version<'i>() -> Parser<'i> {
    seq(
        many1(ns_dec_digit()),
        seq(char_parser('.'), many1(ns_dec_digit())),
    )
}

/// [86] ns-yaml-directive — `"YAML" <sep> <version>`.
#[must_use]
pub fn ns_yaml_directive<'i>() -> Parser<'i> {
    seq(
        seq(
            char_parser('Y'),
            seq(char_parser('A'), seq(char_parser('M'), char_parser('L'))),
        ),
        seq(many1(s_white()), ns_yaml_version()),
    )
}

/// [90] c-primary-tag-handle — `!`
#[must_use]
pub fn c_primary_tag_handle<'i>() -> Parser<'i> {
    // Primary handle is `!` NOT followed by another `!` and not followed by
    // word chars with closing `!` (that would be named or secondary).
    // Here it is just the bare `!`.  Disambiguation is done in c_tag_handle.
    char_parser('!')
}

/// [91] c-secondary-tag-handle — `!!`
#[must_use]
pub fn c_secondary_tag_handle<'i>() -> Parser<'i> {
    seq(char_parser('!'), char_parser('!'))
}

/// [92] c-named-tag-handle — `! <ns-word-char>+ !`
#[must_use]
pub fn c_named_tag_handle<'i>() -> Parser<'i> {
    seq(
        char_parser('!'),
        seq(many1(ns_word_char()), char_parser('!')),
    )
}

/// [89] c-tag-handle — named (longest), secondary, or primary.
///
/// Named must be tried before secondary because both start with `!`.
/// Secondary must be tried before primary for the same reason.
#[must_use]
pub fn c_tag_handle<'i>() -> Parser<'i> {
    alt(
        c_named_tag_handle(),
        alt(c_secondary_tag_handle(), c_primary_tag_handle()),
    )
}

/// [94] c-ns-local-tag-prefix — `! <uri-char>*`
#[must_use]
pub fn c_ns_local_tag_prefix<'i>() -> Parser<'i> {
    seq(char_parser('!'), many0(ns_uri_char()))
}

/// [95] ns-global-tag-prefix — `<tag-char> <uri-char>*`
#[must_use]
pub fn ns_global_tag_prefix<'i>() -> Parser<'i> {
    seq(ns_tag_char(), many0(ns_uri_char()))
}

/// [93] ns-tag-prefix — local or global.
#[must_use]
pub fn ns_tag_prefix<'i>() -> Parser<'i> {
    alt(c_ns_local_tag_prefix(), ns_global_tag_prefix())
}

/// [88] ns-tag-directive — `"TAG" <sep> <handle> <sep> <prefix>`.
#[must_use]
pub fn ns_tag_directive<'i>() -> Parser<'i> {
    seq(
        seq(char_parser('T'), seq(char_parser('A'), char_parser('G'))),
        seq(
            many1(s_white()),
            seq(c_tag_handle(), seq(many1(s_white()), ns_tag_prefix())),
        ),
    )
}

/// [82] l-directive — `% <yaml|tag|reserved> <optional-comment> <break>`.
///
/// A directive must be terminated by a line break (not EOF).  After the
/// directive name/params, an optional inline comment may appear before the
/// break.
///
/// Known directive names (`YAML`, `TAG`) commit: once the keyword matches,
/// if the rest is malformed the parser reports an error rather than falling
/// through to `ns-reserved-directive`. This matches the spec's intent that
/// `YAML` and `TAG` are reserved directive names.
#[must_use]
pub fn l_directive<'i>() -> Parser<'i> {
    seq(
        char_parser('%'),
        seq(
            alt(
                ns_yaml_directive(),
                alt(ns_tag_directive(), ns_reserved_directive()),
            ),
            seq(
                opt(seq(s_separate_in_line(), c_nb_comment_text())),
                b_break(),
            ),
        ),
    )
}

// ---------------------------------------------------------------------------
// §6.8.1 – Node properties [96]–[103]
// ---------------------------------------------------------------------------

/// [103] ns-anchor-name — one or more anchor characters.
#[must_use]
pub fn ns_anchor_name<'i>() -> Parser<'i> {
    many1(ns_anchor_char())
}

// [102] ns-anchor-char is defined in chars.rs as ns_anchor_char().

/// [101] c-ns-anchor-property — `& <ns-anchor-name>`.
///
/// Emits `BeginAnchor` / `Indicator` / `Text` / `EndAnchor`.
#[must_use]
pub fn c_ns_anchor_property<'i>() -> Parser<'i> {
    wrap_tokens(
        Code::BeginAnchor,
        Code::EndAnchor,
        seq(
            token(Code::Indicator, char_parser('&')),
            token(Code::Text, ns_anchor_name()),
        ),
    )
}

/// [98] c-verbatim-tag — `"!<" <uri-char>+ ">"`
///
/// Emits `BeginTag` / inner tokens / `EndTag`.
#[must_use]
pub fn c_verbatim_tag<'i>() -> Parser<'i> {
    wrap_tokens(
        Code::BeginTag,
        Code::EndTag,
        seq(
            token(Code::Indicator, seq(char_parser('!'), char_parser('<'))),
            seq(
                token(Code::Text, many1(ns_uri_char())),
                token(Code::Indicator, char_parser('>')),
            ),
        ),
    )
}

/// [99] c-ns-shorthand-tag — `<tag-handle> <ns-tag-char>+`
///
/// Emits `BeginTag` / inner tokens / `EndTag`.
#[must_use]
pub fn c_ns_shorthand_tag<'i>() -> Parser<'i> {
    wrap_tokens(
        Code::BeginTag,
        Code::EndTag,
        seq(
            token(Code::Indicator, c_tag_handle()),
            token(Code::Text, many1(ns_tag_char())),
        ),
    )
}

/// [100] c-non-specific-tag — bare `!` not followed by URI/word content.
///
/// The bare `!` is the non-specific tag; `!foo` is shorthand.  We use
/// `neg_lookahead` to ensure the `!` is not followed by a non-space character
/// that would make it a shorthand tag.
///
/// Emits `BeginTag` / `Indicator` / `EndTag`.
#[must_use]
pub fn c_non_specific_tag<'i>() -> Parser<'i> {
    wrap_tokens(
        Code::BeginTag,
        Code::EndTag,
        seq(
            token(
                Code::Indicator,
                seq(
                    char_parser('!'),
                    // Must NOT be followed by another `!` (secondary/named) or
                    // a tag char (shorthand suffix).
                    neg_lookahead(satisfy(|ch| {
                        ch != ' ' && ch != '\t' && ch != '\n' && ch != '\r'
                    })),
                ),
            ),
            // neg_lookahead consumed no input, but we need the seq to complete.
            // The neg_lookahead is embedded in the token above; pad with empty.
            Box::new(|state| crate::combinator::Reply::Success {
                tokens: Vec::new(),
                state,
            }),
        ),
    )
}

/// [97] c-ns-tag-property — verbatim, shorthand, or non-specific tag.
#[must_use]
pub fn c_ns_tag_property<'i>() -> Parser<'i> {
    // Verbatim checked first (`!<...>`), then shorthand (`!foo`), then
    // non-specific (bare `!`).
    alt(
        c_verbatim_tag(),
        alt(c_ns_shorthand_tag(), c_non_specific_tag()),
    )
}

/// [96] c-ns-properties(n,c) — tag and/or anchor in either order.
///
/// Accepts: tag only, anchor only, tag+anchor, or anchor+tag.
/// A separator is required between the two properties.
#[must_use]
pub fn c_ns_properties(n: i32, c: Context) -> Parser<'static> {
    // tag then optional (sep + anchor)
    let tag_first = seq(
        c_ns_tag_property(),
        opt(seq(s_separate(n, c), c_ns_anchor_property())),
    );
    // anchor then optional (sep + tag)
    let anchor_first = seq(
        c_ns_anchor_property(),
        opt(seq(s_separate(n, c), c_ns_tag_property())),
    );
    alt(tag_first, anchor_first)
}

// ---------------------------------------------------------------------------
// §9.1.2 – Forbidden positions [205] (shared utility)
// ---------------------------------------------------------------------------

/// [205] c-forbidden — detects `---` or `...` at the start of a line
/// followed by a safe terminator (whitespace, line break, or EOF).
///
/// This is a zero-width check: it succeeds when the current position is
/// a document boundary, consuming no input.  Used by the stream parser and
/// by plain scalar continuation to prevent crossing document boundaries.
#[must_use]
pub fn c_forbidden() -> Parser<'static> {
    Box::new(|state: State<'_>| {
        use crate::combinator::Reply;
        if state.pos.column != 0 {
            return Reply::Failure;
        }
        let marker = if state.input.starts_with("---") {
            "---"
        } else if state.input.starts_with("...") {
            "..."
        } else {
            return Reply::Failure;
        };
        let after = &state.input[marker.len()..];
        let terminates = after.is_empty()
            || after.starts_with(' ')
            || after.starts_with('\t')
            || after.starts_with('\n')
            || after.starts_with('\r');
        if terminates {
            Reply::Success {
                tokens: Vec::new(),
                state,
            }
        } else {
            Reply::Failure
        }
    })
}

// ---------------------------------------------------------------------------
// §6.7 – Separation [76]–[77]
// ---------------------------------------------------------------------------

/// [77] s-separate-lines(n) — newline then indented continuation, or
/// in-line separation.
#[must_use]
pub fn s_separate_lines(n: i32) -> Parser<'static> {
    alt(
        seq(s_l_comments(), s_flow_line_prefix(n)),
        s_separate_in_line(),
    )
}

/// [76] s-separate(n,c) — dispatch on context.
///
/// Only `block-key` and `flow-key` contexts use `s-separate-in-line`; all
/// other contexts (block-out, block-in, flow-out, flow-in) use
/// `s-separate-lines(n)`.
///
/// Flow contexts (`FlowOut`, `FlowIn`) use `s_separate_lines_flow` which accepts
/// ≥ n spaces on continuation lines, because flow content can be indented
/// more deeply than the parent indent level.
#[must_use]
pub fn s_separate(n: i32, c: Context) -> Parser<'static> {
    match c {
        Context::BlockOut | Context::BlockIn => s_separate_lines(n),
        Context::FlowOut | Context::FlowIn => s_separate_lines_flow(n),
        Context::BlockKey | Context::FlowKey => s_separate_in_line(),
    }
}

/// s-separate-ge(n,c) — like s-separate but accepts ≥ n spaces of indentation.
///
/// Used for separating flow content within block context where deeper
/// indentation than the minimum is permitted.
#[must_use]
pub fn s_separate_ge(n: i32, c: Context) -> Parser<'static> {
    match c {
        Context::BlockOut | Context::BlockIn | Context::FlowOut | Context::FlowIn => {
            s_separate_lines_flow(n)
        }
        Context::BlockKey | Context::FlowKey => s_separate_in_line(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::combinator::{Reply, State};
    use crate::token::Code;

    fn state(input: &str) -> State<'_> {
        State::new(input)
    }

    fn state_with(input: &str, n: i32, c: Context) -> State<'_> {
        State::with_context(input, n, c)
    }

    fn is_success(reply: &Reply<'_>) -> bool {
        matches!(reply, Reply::Success { .. })
    }

    fn is_failure(reply: &Reply<'_>) -> bool {
        matches!(reply, Reply::Failure)
    }

    fn remaining<'a>(reply: &'a Reply<'a>) -> &'a str {
        match reply {
            Reply::Success { state, .. } => state.input,
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    fn codes(reply: Reply<'_>) -> Vec<Code> {
        match reply {
            Reply::Success { tokens, .. } => tokens.into_iter().map(|t| t.code).collect(),
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    // -----------------------------------------------------------------------
    // Group 1: Indentation [63]–[65]
    // -----------------------------------------------------------------------

    #[test]
    fn s_indent_matches_exactly_n_spaces() {
        let reply = s_indent(3)(state("   rest"));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_indent_fails_when_fewer_than_n_spaces() {
        let reply = s_indent(3)(state("  rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn s_indent_fails_when_more_spaces_present() {
        let reply = s_indent(3)(state("    rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn s_indent_zero_matches_empty() {
        let reply = s_indent(0)(state("rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_indent_fails_on_tab() {
        let reply = s_indent(1)(state("\trest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn s_indent_fails_on_empty_input_when_n_positive() {
        let reply = s_indent(2)(state(""));
        assert!(is_failure(&reply));
    }

    #[test]
    fn s_indent_lt_accepts_zero_spaces() {
        let reply = s_indent_lt(3)(state("rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_indent_lt_accepts_fewer_than_n_spaces() {
        let reply = s_indent_lt(3)(state("  rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_indent_lt_fails_when_n_or_more_spaces() {
        let reply = s_indent_lt(3)(state("   rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn s_indent_le_accepts_zero_spaces() {
        let reply = s_indent_le(3)(state("rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn s_indent_le_accepts_exactly_n_spaces() {
        let reply = s_indent_le(3)(state("   rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_indent_le_fails_when_more_than_n_spaces() {
        let reply = s_indent_le(3)(state("    rest"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 2: Separation spaces [66]–[80]
    // -----------------------------------------------------------------------

    #[test]
    fn s_separate_in_line_accepts_one_space() {
        let reply = s_separate_in_line()(state(" rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn s_separate_in_line_accepts_multiple_spaces_and_tabs() {
        let reply = s_separate_in_line()(state("  \t rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_separate_in_line_accepts_at_start_of_line() {
        // column == 0 at state origin
        let reply = s_separate_in_line()(state(""));
        assert!(is_success(&reply));
    }

    #[test]
    fn s_separate_in_line_fails_on_non_whitespace_in_mid_line() {
        // Advance past column 0 by building a state with non-zero column.
        // We do this by running a parser that consumes 'x' first.
        let p = seq(crate::combinator::char_parser('x'), s_separate_in_line());
        let reply = p(state("xarest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn s_line_prefix_block_context_requires_indent() {
        let reply = s_line_prefix(2, Context::BlockOut)(state("  rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_line_prefix_flow_context_requires_indent_for_nonzero_n() {
        let reply = s_line_prefix(2, Context::FlowOut)(state("  rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_line_prefix_flow_context_fails_when_indent_below_n() {
        let reply = s_line_prefix(2, Context::FlowOut)(state("rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn s_line_prefix_dispatches_all_six_contexts() {
        for c in [
            Context::BlockOut,
            Context::BlockIn,
            Context::FlowOut,
            Context::FlowIn,
            Context::BlockKey,
            Context::FlowKey,
        ] {
            let s = state_with("rest", 0, c);
            let reply = s_line_prefix(0, c)(s);
            assert!(is_success(&reply), "failed for {c:?}");
        }
    }

    #[test]
    fn l_empty_accepts_blank_line_with_just_newline() {
        let reply = l_empty(0, Context::BlockOut)(state("\nrest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn l_empty_accepts_line_with_only_spaces_before_newline() {
        let reply = l_empty(2, Context::BlockOut)(state("  \nrest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_empty_fails_on_non_empty_content() {
        let reply = l_empty(0, Context::BlockOut)(state("a\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn b_l_trimmed_consumes_break_then_empty_lines() {
        let reply = b_l_trimmed(0, Context::BlockOut)(state("\n\n\nrest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn b_l_trimmed_fails_when_no_break() {
        let reply = b_l_trimmed(0, Context::BlockOut)(state("rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn b_as_space_succeeds_on_break() {
        let reply = b_as_space()(state("\nrest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn b_as_space_fails_on_space() {
        let reply = b_as_space()(state(" rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn s_separate_block_context_uses_separate_lines() {
        // In block context, s_separate delegates to s_separate_lines which
        // accepts s_l_comments followed by s_flow_line_prefix.
        // A bare newline followed by the indent satisfies this.
        let reply = s_separate(2, Context::BlockOut)(state("\n  rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_separate_flow_context_uses_separate_in_line() {
        let reply = s_separate(2, Context::FlowOut)(state(" rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn s_separate_dispatches_all_six_contexts() {
        // Flow contexts accept a space; block contexts accept a newline.
        for c in [
            Context::FlowOut,
            Context::FlowIn,
            Context::BlockKey,
            Context::FlowKey,
        ] {
            let reply = s_separate(0, c)(state(" "));
            assert!(is_success(&reply), "failed for {c:?}");
        }
        for c in [Context::BlockOut, Context::BlockIn] {
            let reply = s_separate(0, c)(state("\n"));
            assert!(is_success(&reply), "failed for {c:?}");
        }
    }

    // -----------------------------------------------------------------------
    // Group 3: Comments [75], [78]–[81]
    // -----------------------------------------------------------------------

    #[test]
    fn c_nb_comment_text_accepts_hash_followed_by_text() {
        let reply = c_nb_comment_text()(state("# hello world\nrest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "\nrest");
    }

    #[test]
    fn c_nb_comment_text_accepts_empty_comment() {
        let reply = c_nb_comment_text()(state("#\nrest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "\nrest");
    }

    #[test]
    fn c_nb_comment_text_fails_when_no_hash() {
        let reply = c_nb_comment_text()(state("hello"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn b_comment_accepts_newline() {
        let reply = b_comment()(state("\nrest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn b_comment_accepts_end_of_input() {
        let reply = b_comment()(state(""));
        assert!(is_success(&reply));
    }

    #[test]
    fn b_comment_fails_on_non_break_non_eof() {
        let reply = b_comment()(state("arest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn s_b_comment_emits_begin_comment_token() {
        let reply = s_b_comment()(state(" # hello\n"));
        let cs = codes(reply);
        assert_eq!(cs.first().copied(), Some(Code::BeginComment));
    }

    #[test]
    fn s_b_comment_emits_end_comment_token() {
        let reply = s_b_comment()(state(" # hello\n"));
        let cs = codes(reply);
        assert_eq!(cs.last().copied(), Some(Code::EndComment));
    }

    #[test]
    fn s_b_comment_text_token_is_between_begin_and_end() {
        let reply = s_b_comment()(state(" # hello\n"));
        let cs = codes(reply);
        assert!(cs.contains(&Code::Text));
    }

    #[test]
    fn s_b_comment_succeeds_with_no_comment_at_eol() {
        let reply = s_b_comment()(state("\nrest"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert!(!cs.contains(&Code::BeginComment));
        assert!(!cs.contains(&Code::EndComment));
    }

    #[test]
    fn s_b_comment_succeeds_at_eof_with_no_comment() {
        let reply = s_b_comment()(state(""));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert!(cs.is_empty());
    }

    #[test]
    fn l_comment_accepts_full_comment_line() {
        let reply = l_comment()(state("   # full comment\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginComment));
        assert!(cs.contains(&Code::EndComment));
    }

    #[test]
    fn l_comment_fails_on_non_whitespace_start() {
        // `l_comment` starts with `s_separate_in_line` which at column 0
        // succeeds with zero consumption — so `# comment\n` would actually
        // succeed here via the zero-consumption path.  Per the test spec,
        // `l_comment` requires at least one space before `#`.
        // We test a state at non-zero column to validate the real constraint.
        let p = seq(crate::combinator::char_parser('x'), l_comment());
        let reply = p(state("x# comment\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn l_comment_fails_when_non_whitespace_before_hash() {
        let p = seq(crate::combinator::char_parser('a'), l_comment());
        let reply = p(state("a# comment\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn s_l_comments_accepts_single_comment_line() {
        let reply = s_l_comments()(state(" # comment\nrest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_l_comments_accepts_multiple_consecutive_comment_lines() {
        let reply = s_l_comments()(state(" # line1\n # line2\nrest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
        let cs = codes(reply);
        let begin_count = cs.iter().filter(|&&c| c == Code::BeginComment).count();
        assert!(begin_count >= 2, "expected at least 2 BeginComment tokens");
    }

    #[test]
    fn s_l_comments_accepts_empty_lines_between_comments() {
        let reply = s_l_comments()(state(" # line1\n\n # line2\nrest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_l_comments_succeeds_with_only_a_newline() {
        let reply = s_l_comments()(state("\nrest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn s_l_comments_fails_on_non_comment_non_newline() {
        let reply = s_l_comments()(state("rest"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 4: Directives [82]–[95]
    // -----------------------------------------------------------------------

    #[test]
    fn ns_yaml_version_accepts_major_minor() {
        let reply = ns_yaml_version()(state("1.2rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn ns_yaml_version_accepts_multi_digit_parts() {
        let reply = ns_yaml_version()(state("1.12rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn ns_yaml_version_fails_without_dot() {
        let reply = ns_yaml_version()(state("12rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_yaml_version_fails_with_non_digit_after_dot() {
        let reply = ns_yaml_version()(state("1.xrest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_yaml_directive_accepts_yaml_version_line() {
        let reply = ns_yaml_directive()(state("YAML 1.2"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_yaml_directive_fails_on_wrong_keyword() {
        let reply = ns_yaml_directive()(state("YAMLX 1.2"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_yaml_directive_fails_without_version() {
        let reply = ns_yaml_directive()(state("YAML "));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_tag_handle_primary_is_single_exclamation() {
        // Primary: `!` not followed by `!` or word chars+`!`
        // Use a state where `!` is followed by a space (non-word).
        let reply = c_tag_handle()(state("! rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_tag_handle_secondary_is_double_exclamation() {
        let reply = c_tag_handle()(state("!!rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn c_tag_handle_named_has_word_chars_between_bangs() {
        let reply = c_tag_handle()(state("!yaml!rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn c_tag_handle_named_requires_closing_exclamation() {
        // `!yaml` with no closing `!` cannot be a named handle.
        // It will fall through to secondary (fails) then primary (succeeds with `!`).
        // We verify the named path doesn't consume `!yaml` as a named handle.
        let reply = c_named_tag_handle()(state("!yaml"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_tag_handle_secondary_not_confused_with_named() {
        // `!!rest` — secondary handle `!!` must be correctly identified.
        let reply = c_tag_handle()(state("!!rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn ns_tag_prefix_local_starts_with_exclamation() {
        let reply = ns_tag_prefix()(state("!foo:rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_tag_prefix_global_starts_with_tag_char() {
        let reply = ns_tag_prefix()(state("tag:yaml.org,2002:rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_tag_prefix_fails_when_empty() {
        let reply = ns_tag_prefix()(state(""));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_tag_directive_accepts_primary_handle_with_prefix() {
        let reply = ns_tag_directive()(state("TAG ! yaml:"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_tag_directive_accepts_secondary_handle_with_prefix() {
        let reply = ns_tag_directive()(state("TAG !! tag:yaml.org,2002:"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_tag_directive_accepts_named_handle_with_prefix() {
        let reply = ns_tag_directive()(state("TAG !yaml! tag:yaml.org,2002:"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_tag_directive_fails_wrong_keyword() {
        let reply = ns_tag_directive()(state("TAGX ! yaml:"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn l_directive_accepts_yaml_directive_line() {
        let reply = l_directive()(state("%YAML 1.2\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_directive_accepts_tag_directive_line() {
        let reply = l_directive()(state("%TAG !! tag:yaml.org,2002:\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_directive_accepts_reserved_directive() {
        let reply = l_directive()(state("%SOMEOTHER param1 param2\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_directive_fails_without_percent() {
        let reply = l_directive()(state("YAML 1.2\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn l_directive_fails_without_trailing_newline() {
        let reply = l_directive()(state("%YAML 1.2"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_reserved_directive_accepts_name_and_params() {
        let reply = ns_reserved_directive()(state("FUTURE param1 param2"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_reserved_directive_accepts_name_only_no_params() {
        let reply = ns_reserved_directive()(state("FUTURE"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn ns_reserved_directive_fails_on_empty_name() {
        let reply = ns_reserved_directive()(state(" param"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 5: Node properties [96]–[103]
    // -----------------------------------------------------------------------

    #[test]
    fn ns_anchor_name_accepts_word_chars() {
        let reply = ns_anchor_name()(state("my-anchor rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn ns_anchor_name_accepts_single_char() {
        let reply = ns_anchor_name()(state("a "));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " ");
    }

    #[test]
    fn ns_anchor_name_fails_on_space() {
        let reply = ns_anchor_name()(state(" rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_anchor_name_stops_at_flow_indicator() {
        let reply = ns_anchor_name()(state("name]rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "]rest");
    }

    #[test]
    fn c_ns_anchor_property_emits_begin_anchor() {
        let reply = c_ns_anchor_property()(state("&myanchor rest"));
        let cs = codes(reply);
        assert_eq!(cs.first().copied(), Some(Code::BeginAnchor));
    }

    #[test]
    fn c_ns_anchor_property_emits_end_anchor() {
        let reply = c_ns_anchor_property()(state("&myanchor rest"));
        let cs = codes(reply);
        assert_eq!(cs.last().copied(), Some(Code::EndAnchor));
    }

    #[test]
    fn c_ns_anchor_property_emits_indicator_for_ampersand() {
        let reply = c_ns_anchor_property()(state("&myanchor rest"));
        let cs = codes(reply);
        assert!(cs.contains(&Code::Indicator));
    }

    #[test]
    fn c_ns_anchor_property_emits_text_for_name() {
        let reply = c_ns_anchor_property()(state("&myanchor rest"));
        let cs = codes(reply);
        assert!(cs.contains(&Code::Text));
    }

    #[test]
    fn c_ns_anchor_property_fails_without_ampersand() {
        let reply = c_ns_anchor_property()(state("myanchor rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_ns_anchor_property_fails_with_empty_name() {
        let reply = c_ns_anchor_property()(state("& rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_verbatim_tag_emits_begin_tag() {
        let reply = c_verbatim_tag()(state("!<tag:yaml.org,2002:str>rest"));
        let cs = codes(reply);
        assert_eq!(cs.first().copied(), Some(Code::BeginTag));
    }

    #[test]
    fn c_verbatim_tag_emits_end_tag() {
        let reply = c_verbatim_tag()(state("!<tag:yaml.org,2002:str>rest"));
        let cs = codes(reply);
        assert_eq!(cs.last().copied(), Some(Code::EndTag));
    }

    #[test]
    fn c_verbatim_tag_consumes_full_verbatim_syntax() {
        let reply = c_verbatim_tag()(state("!<tag:yaml.org>rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn c_verbatim_tag_fails_without_opening_angle_bracket() {
        let reply = c_verbatim_tag()(state("!tag:yaml.org>rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_verbatim_tag_fails_without_closing_angle_bracket() {
        let reply = c_verbatim_tag()(state("!<tag:yaml.orgrest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_verbatim_tag_fails_with_empty_uri() {
        let reply = c_verbatim_tag()(state("!<>rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_ns_shorthand_tag_emits_begin_and_end_tag() {
        let reply = c_ns_shorthand_tag()(state("!!str rest"));
        let cs = codes(reply);
        assert_eq!(cs.first().copied(), Some(Code::BeginTag));
        assert_eq!(cs.last().copied(), Some(Code::EndTag));
    }

    #[test]
    fn c_ns_shorthand_tag_accepts_primary_handle() {
        let reply = c_ns_shorthand_tag()(state("!local rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_ns_shorthand_tag_accepts_secondary_handle() {
        let reply = c_ns_shorthand_tag()(state("!!str rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_ns_shorthand_tag_accepts_named_handle() {
        let reply = c_ns_shorthand_tag()(state("!yaml!str rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_ns_shorthand_tag_fails_with_empty_suffix() {
        // `!! ` — secondary handle with space as suffix (not a tag char).
        let reply = c_ns_shorthand_tag()(state("!! rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_non_specific_tag_emits_begin_and_end_tag() {
        let reply = c_non_specific_tag()(state("! rest"));
        let cs = codes(reply);
        assert_eq!(cs.first().copied(), Some(Code::BeginTag));
        assert_eq!(cs.last().copied(), Some(Code::EndTag));
    }

    #[test]
    fn c_non_specific_tag_accepts_lone_exclamation() {
        let reply = c_non_specific_tag()(state("! rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_non_specific_tag_fails_when_exclamation_starts_suffix() {
        let reply = c_non_specific_tag()(state("!x rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_ns_tag_property_dispatches_to_verbatim() {
        let reply = c_ns_tag_property()(state("!<uri>rest"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginTag));
        assert!(cs.contains(&Code::EndTag));
    }

    #[test]
    fn c_ns_tag_property_dispatches_to_shorthand() {
        let reply = c_ns_tag_property()(state("!!str rest"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginTag));
        assert!(cs.contains(&Code::EndTag));
    }

    #[test]
    fn c_ns_tag_property_dispatches_to_non_specific() {
        let reply = c_ns_tag_property()(state("! rest"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginTag));
        assert!(cs.contains(&Code::EndTag));
    }

    #[test]
    fn c_ns_tag_property_fails_when_no_exclamation() {
        let reply = c_ns_tag_property()(state("str rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_ns_properties_accepts_tag_only() {
        let reply = c_ns_properties(0, Context::BlockOut)(state("!!str rest"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginTag));
        assert!(!cs.contains(&Code::BeginAnchor));
    }

    #[test]
    fn c_ns_properties_accepts_anchor_only() {
        let reply = c_ns_properties(0, Context::BlockOut)(state("&anchor rest"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginAnchor));
        assert!(!cs.contains(&Code::BeginTag));
    }

    #[test]
    fn c_ns_properties_accepts_tag_then_anchor() {
        let reply = c_ns_properties(0, Context::BlockOut)(state("!!str &anchor rest"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        let begin_tag_pos = cs.iter().position(|&c| c == Code::BeginTag).unwrap();
        let begin_anchor_pos = cs.iter().position(|&c| c == Code::BeginAnchor).unwrap();
        assert!(begin_tag_pos < begin_anchor_pos);
    }

    #[test]
    fn c_ns_properties_accepts_anchor_then_tag() {
        let reply = c_ns_properties(0, Context::BlockOut)(state("&anchor !!str rest"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        let begin_anchor_pos = cs.iter().position(|&c| c == Code::BeginAnchor).unwrap();
        let begin_tag_pos = cs.iter().position(|&c| c == Code::BeginTag).unwrap();
        assert!(begin_anchor_pos < begin_tag_pos);
    }

    #[test]
    fn c_ns_properties_fails_when_neither_tag_nor_anchor() {
        let reply = c_ns_properties(0, Context::BlockOut)(state("rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_ns_properties_requires_whitespace_between_tag_and_anchor() {
        let reply = c_ns_properties(0, Context::BlockOut)(state("!!str&anchor rest"));
        // The tag `!!str` is consumed. Then s_separate fails (no whitespace).
        // The opt() around (sep + anchor) returns success with no anchor.
        // So the overall reply is success but only tag tokens, not anchor.
        // The test verifies that `&anchor` is NOT consumed as part of properties.
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert!(
            !cs.contains(&Code::BeginAnchor),
            "anchor should not be consumed without whitespace separator"
        );
    }
}
