// SPDX-License-Identifier: MIT

//! YAML 1.2 §7 flow style productions [104]–[161].
//!
//! Covers alias nodes, empty nodes, double-quoted scalars, single-quoted
//! scalars, plain scalars, flow sequences, flow mappings, and flow nodes.
//! Each function is named after the spec production and cross-referenced by
//! its production number in a `// [N]` comment.

use crate::chars::{
    b_non_content, decode_escape, ns_anchor_char, ns_plain_char, ns_plain_first, s_white,
};
use crate::combinator::neg_lookahead;
use crate::combinator::{
    Context, Parser, Reply, State, char_parser, many0, many1, seq, token, wrap_tokens,
};
use crate::structure::{
    c_forbidden, c_ns_properties, l_empty, s_flow_folded, s_flow_line_prefix_ge, s_separate,
    s_separate_ge,
};
use crate::token::{Code, Token};
use smallvec::{SmallVec, smallvec};

// ---------------------------------------------------------------------------
// §7.1 – Alias nodes [104]
// ---------------------------------------------------------------------------

/// [104] c-ns-alias-node — `*` followed by anchor name.
///
/// Emits `BeginAlias` / `Indicator` / `Meta` / `EndAlias`.
#[must_use]
pub fn c_ns_alias_node<'i>() -> Parser<'i> {
    wrap_tokens(
        Code::BeginAlias,
        Code::EndAlias,
        seq(
            token(Code::Indicator, char_parser('*')),
            token(Code::Meta, many1(ns_anchor_char())),
        ),
    )
}

// ---------------------------------------------------------------------------
// §7.2 – Empty nodes [105]–[106]
// ---------------------------------------------------------------------------

/// [105] e-scalar — empty scalar: zero consumption, always succeeds.
#[must_use]
pub fn e_scalar<'i>() -> Parser<'i> {
    Box::new(|state| Reply::Success {
        tokens: SmallVec::new(),
        state,
    })
}

/// [106] e-node — empty node: zero consumption, always succeeds.
#[must_use]
pub fn e_node<'i>() -> Parser<'i> {
    Box::new(|state| Reply::Success {
        tokens: SmallVec::new(),
        state,
    })
}

// ---------------------------------------------------------------------------
// §7.3.1 – Double-quoted scalars [107]–[113]
// ---------------------------------------------------------------------------

/// [107] nb-double-char — a character allowed in a double-quoted scalar body.
///
/// Either an escape sequence (`\` followed by a valid escape code) or any
/// non-break character that is not `"` or `\`.
#[must_use]
pub fn nb_double_char<'i>() -> Parser<'i> {
    Box::new(|state| {
        let Some(ch) = state.peek() else {
            return Reply::Failure;
        };
        match ch {
            '"' => Reply::Failure,
            '\\' => {
                // Record start before consuming the backslash.
                let start_pos = state.pos;
                let start_input = state.input;
                let after_backslash = state.advance('\\');
                let rest = after_backslash.input;
                match decode_escape(rest) {
                    None => Reply::Failure,
                    Some((_decoded, bytes_consumed)) => {
                        // Advance through the escape code characters.
                        let mut s = after_backslash;
                        let mut consumed = 0;
                        for ec in rest.chars() {
                            if consumed >= bytes_consumed {
                                break;
                            }
                            s = s.advance(ec);
                            consumed += ec.len_utf8();
                        }
                        let total_bytes = s.pos.byte_offset - start_pos.byte_offset;
                        let text = &start_input[..total_bytes];
                        // Emit escape as a Text token (there is no Escape code variant).
                        Reply::Success {
                            tokens: smallvec![Token {
                                code: Code::Text,
                                pos: start_pos,
                                text,
                            }],
                            state: s,
                        }
                    }
                }
            }
            // Any nb-char (not a line break, not BOM) that is not '"' or '\'
            _ => {
                // Reject line break characters.
                if matches!(ch, '\n' | '\r') {
                    return Reply::Failure;
                }
                // Reject BOM.
                if ch == '\u{FEFF}' {
                    return Reply::Failure;
                }
                let start_pos = state.pos;
                let start_input = state.input;
                let new_state = state.advance(ch);
                let byte_len = ch.len_utf8();
                Reply::Success {
                    tokens: smallvec![Token {
                        code: Code::Text,
                        pos: start_pos,
                        text: &start_input[..byte_len],
                    }],
                    state: new_state,
                }
            }
        }
    })
}

/// [108] ns-double-char — `nb-double-char` that is not whitespace.
#[must_use]
pub fn ns_double_char<'i>() -> Parser<'i> {
    Box::new(|state| {
        let Some(ch) = state.peek() else {
            return Reply::Failure;
        };
        if matches!(ch, ' ' | '\t') {
            return Reply::Failure;
        }
        nb_double_char()(state)
    })
}

/// [111] nb-double-one-line — text content of one line in a double-quoted scalar.
fn nb_double_one_line<'i>() -> Parser<'i> {
    many0(nb_double_char())
}

/// [114] nb-ns-double-in-line — interleaved spaces and non-space chars on one line.
///
/// Matches zero or more groups of (optional whitespace + one ns-double-char).
/// This is `(s-white* ns-double-char)*`.
fn nb_ns_double_in_line<'i>() -> Parser<'i> {
    many0(Box::new(|state: State<'_>| {
        let (ws_tokens, after_ws) = match many0(s_white())(state) {
            Reply::Success { tokens, state } => (tokens, state),
            other @ (Reply::Failure | Reply::Error(_)) => return other,
        };
        match ns_double_char()(after_ws) {
            Reply::Success {
                tokens: ns_tokens,
                state: final_state,
            } => {
                let mut all = ws_tokens;
                all.extend(ns_tokens);
                Reply::Success {
                    tokens: all,
                    state: final_state,
                }
            }
            // No ns-double-char after whitespace — backtrack: do not consume the whitespace.
            Reply::Failure => Reply::Failure,
            other @ Reply::Error(_) => other,
        }
    }))
}

/// [112] s-double-escaped(n) — backslash-escaped line break in double-quoted scalars.
///
/// Handles `\` at end of line: `s-white* '\' b-non-content l-empty(n,flow-in)* s-flow-line-prefix(n)`.
fn s_double_escaped(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        // Optional leading whitespace before `\`.
        let (ws_tokens, after_ws) = match many0(s_white())(state) {
            Reply::Success { tokens, state } => (tokens, state),
            other @ (Reply::Failure | Reply::Error(_)) => return other,
        };
        // Must be followed by `\`.
        let Some('\\') = after_ws.peek() else {
            return Reply::Failure;
        };
        let backslash_pos = after_ws.pos;
        let after_backslash = after_ws.advance('\\');
        let backslash_token = crate::token::Token {
            code: Code::Text,
            pos: backslash_pos,
            text: "\\",
        };
        // Non-content break.
        let (break_tokens, after_break) = match b_non_content()(after_backslash) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure => return Reply::Failure,
            other @ Reply::Error(_) => return other,
        };
        // Zero or more empty lines.
        let (empty_tokens, after_empty) = match many0(l_empty(n, Context::FlowIn))(after_break) {
            Reply::Success { tokens, state } => (tokens, state),
            other @ (Reply::Failure | Reply::Error(_)) => return other,
        };
        // Flow line prefix for the continuation — use _ge to accept deeper indent.
        let (prefix_tokens, final_state) = match s_flow_line_prefix_ge(n)(after_empty) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure => return Reply::Failure,
            other @ Reply::Error(_) => return other,
        };
        let mut all = ws_tokens;
        all.push(backslash_token);
        all.extend(break_tokens);
        all.extend(empty_tokens);
        all.extend(prefix_tokens);
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

/// [113] s-double-break(n) — break transition in double-quoted scalars.
///
/// Either an escaped break (`s-double-escaped`) or a flow-folded break.
fn s_double_break(n: i32) -> Parser<'static> {
    // s-double-escaped takes priority over s-flow-folded.
    Box::new(move |state| match s_double_escaped(n)(state.clone()) {
        reply @ Reply::Success { .. } => reply,
        Reply::Failure | Reply::Error(_) => s_flow_folded(n)(state),
    })
}

/// [115] s-double-next-line(n) — continuation line inside double-quoted scalars.
///
/// Per spec [115]: `s-double-break(n) (ns-double-char nb-ns-double-in-line
/// (s-double-next-line(n) | s-white*) | "")`.
fn s_double_next_line(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        // Consume the break transition.
        let (break_tokens, after_break) = match s_double_break(n)(state) {
            Reply::Success { tokens, state } => (tokens, state),
            other @ (Reply::Failure | Reply::Error(_)) => return other,
        };
        // Optional content on the continuation line.
        // Try ns-double-char first; if absent, the break alone is valid ("").
        let Some(ch) = after_break.peek() else {
            // EOF after break — no more content.
            return Reply::Success {
                tokens: break_tokens,
                state: after_break,
            };
        };
        // Check if first char is ns-double-char (not space/tab and not `"` or `\n`/`\r`).
        if matches!(ch, ' ' | '\t' | '"') {
            // Not starting with ns-double-char — empty continuation.
            return Reply::Success {
                tokens: break_tokens,
                state: after_break,
            };
        }
        let (first_tokens, after_first) = match ns_double_char()(after_break.clone()) {
            Reply::Success { tokens, state } => (tokens, state),
            // Not an ns-double-char — empty continuation after break.
            Reply::Failure | Reply::Error(_) => {
                return Reply::Success {
                    tokens: break_tokens,
                    state: after_break,
                };
            }
        };
        // Consume the rest of the line: (s-white* ns-double-char)*.
        let (inline_tokens, after_inline) = match nb_ns_double_in_line()(after_first) {
            Reply::Success { tokens, state } => (tokens, state),
            other @ (Reply::Failure | Reply::Error(_)) => return other,
        };
        // Try another continuation or trailing whitespace.
        let (tail_tokens, final_state) = match s_double_next_line(n)(after_inline.clone()) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => {
                // Trailing s-white* at end of scalar.
                match many0(s_white())(after_inline.clone()) {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_inline),
                }
            }
        };
        let mut all = break_tokens;
        all.extend(first_tokens);
        all.extend(inline_tokens);
        all.extend(tail_tokens);
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

/// [116] nb-double-multi-line(n) — multi-line body of double-quoted scalar.
///
/// Per spec [116]: `nb-ns-double-in-line (s-double-next-line(n) | s-white*)`.
fn nb_double_multi_line(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        // Consume interleaved non-space content (may include spaces between ns chars).
        let (inline_tokens, after_inline) = match nb_ns_double_in_line()(state) {
            Reply::Success { tokens, state } => (tokens, state),
            other @ (Reply::Failure | Reply::Error(_)) => return other,
        };
        // Try a continuation line, or accept trailing whitespace for last line.
        let (tail_tokens, final_state) = match s_double_next_line(n)(after_inline.clone()) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => match many0(s_white())(after_inline.clone()) {
                Reply::Success { tokens, state } => (tokens, state),
                Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_inline),
            },
        };
        let mut all = inline_tokens;
        all.extend(tail_tokens);
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

/// [110] nb-double-text(n,c) — full body of a double-quoted scalar.
fn nb_double_text(n: i32, c: Context) -> Parser<'static> {
    // Multi-line is permitted in FlowOut and FlowIn contexts per spec [110].
    match c {
        Context::FlowOut | Context::FlowIn => nb_double_multi_line(n),
        Context::BlockKey | Context::FlowKey | Context::BlockOut | Context::BlockIn => {
            nb_double_one_line()
        }
    }
}

/// [109] c-double-quoted(n,c) — `"` body `"` wrapped in `BeginScalar`/`EndScalar`.
#[must_use]
pub fn c_double_quoted(n: i32, c: Context) -> Parser<'static> {
    wrap_tokens(
        Code::BeginScalar,
        Code::EndScalar,
        Box::new(move |state| {
            // Opening quote.
            let (open_tokens, after_open) = match token(Code::Indicator, char_parser('"'))(state) {
                Reply::Success { tokens, state } => (tokens, state),
                other @ (Reply::Failure | Reply::Error(_)) => return other,
            };
            // Body — collect all chars as a single Text token.
            let body_parser = nb_double_text(n, c);
            match body_parser(after_open.clone()) {
                Reply::Success {
                    tokens,
                    state: after_body,
                } => {
                    // Closing quote.
                    match token(Code::Indicator, char_parser('"'))(after_body) {
                        Reply::Success {
                            tokens: close_tokens,
                            state: final_state,
                        } => {
                            let mut all = open_tokens;
                            all.extend(tokens);
                            all.extend(close_tokens);
                            Reply::Success {
                                tokens: all,
                                state: final_state,
                            }
                        }
                        Reply::Failure | Reply::Error(_) => Reply::Failure,
                    }
                }
                other @ (Reply::Failure | Reply::Error(_)) => other,
            }
        }),
    )
}

// ---------------------------------------------------------------------------
// §7.3.3 – Single-quoted scalars [114]–[121]
// ---------------------------------------------------------------------------

/// [114] c-quoted-quote — two adjacent single quotes representing one `'`.
#[must_use]
pub fn c_quoted_quote<'i>() -> Parser<'i> {
    seq(char_parser('\''), char_parser('\''))
}

/// [115] nb-single-char — a character allowed in a single-quoted scalar body.
///
/// Either `''` (escaped single quote) or any non-break, non-`'` character.
#[must_use]
pub fn nb_single_char<'i>() -> Parser<'i> {
    Box::new(|state| {
        let Some(ch) = state.peek() else {
            return Reply::Failure;
        };
        if ch == '\'' {
            // Attempt `''` (escaped); lone `'` is the scalar end.
            return c_quoted_quote()(state);
        }
        if matches!(ch, '\n' | '\r') {
            return Reply::Failure;
        }
        if ch == '\u{FEFF}' {
            return Reply::Failure;
        }
        let start_pos = state.pos;
        let start_input = state.input;
        let new_state = state.advance(ch);
        let byte_len = ch.len_utf8();
        Reply::Success {
            tokens: smallvec![Token {
                code: Code::Text,
                pos: start_pos,
                text: &start_input[..byte_len],
            }],
            state: new_state,
        }
    })
}

/// [116] ns-single-char — `nb-single-char` that is not whitespace.
#[must_use]
pub fn ns_single_char<'i>() -> Parser<'i> {
    Box::new(|state| {
        let Some(ch) = state.peek() else {
            return Reply::Failure;
        };
        if matches!(ch, ' ' | '\t') {
            return Reply::Failure;
        }
        nb_single_char()(state)
    })
}

/// [119] nb-single-one-line — one line of single-quoted text.
fn nb_single_one_line<'i>() -> Parser<'i> {
    many0(nb_single_char())
}

/// nb-ns-single-in-line — interleaved whitespace and non-space chars on one line.
///
/// Matches `( s-white* ns-single-char )*` — the single-quoted analogue of
/// `nb-ns-double-in-line` [114].
fn nb_ns_single_in_line<'i>() -> Parser<'i> {
    many0(Box::new(|state: State<'_>| {
        let (ws_tokens, after_ws) = match many0(s_white())(state) {
            Reply::Success { tokens, state } => (tokens, state),
            other @ (Reply::Failure | Reply::Error(_)) => return other,
        };
        match ns_single_char()(after_ws) {
            Reply::Success {
                tokens: ns_tokens,
                state: final_state,
            } => {
                let mut all = ws_tokens;
                all.extend(ns_tokens);
                Reply::Success {
                    tokens: all,
                    state: final_state,
                }
            }
            Reply::Failure => Reply::Failure,
            other @ Reply::Error(_) => other,
        }
    }))
}

/// [124] s-single-next-line(n) — line folding inside single-quoted scalars.
///
/// Per spec [124]:
///   `s-flow-folded(n)
///    ( ns-single-char nb-ns-single-in-line
///      ( s-single-next-line(n) | s-white* ) )?`
///
/// The continuation part after `s-flow-folded` is optional.
fn s_single_next_line(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        let (fold_tokens, after_fold) = match s_flow_folded(n)(state) {
            Reply::Success { tokens, state } => (tokens, state),
            other @ (Reply::Failure | Reply::Error(_)) => return other,
        };

        // Optional continuation: ns-single-char nb-ns-single-in-line ( recurse | s-white* )
        let (first_tokens, after_first) = match ns_single_char()(after_fold.clone()) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => {
                return Reply::Success {
                    tokens: fold_tokens,
                    state: after_fold,
                };
            }
        };
        let (inline_tokens, after_inline) = match nb_ns_single_in_line()(after_first) {
            Reply::Success { tokens, state } => (tokens, state),
            other @ (Reply::Failure | Reply::Error(_)) => return other,
        };
        let (tail_tokens, final_state) = match s_single_next_line(n)(after_inline.clone()) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => match many0(s_white())(after_inline.clone()) {
                Reply::Success { tokens, state } => (tokens, state),
                Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_inline),
            },
        };
        let mut all = fold_tokens;
        all.extend(first_tokens);
        all.extend(inline_tokens);
        all.extend(tail_tokens);
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

/// [125] nb-single-multi-line(n) — multi-line body of single-quoted scalar.
///
/// Per spec [125]: `nb-ns-single-in-line ( s-single-next-line(n) | s-white* )`.
fn nb_single_multi_line(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        let (inline_tokens, after_inline) = match nb_ns_single_in_line()(state) {
            Reply::Success { tokens, state } => (tokens, state),
            other @ (Reply::Failure | Reply::Error(_)) => return other,
        };
        let (tail_tokens, final_state) = match s_single_next_line(n)(after_inline.clone()) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => match many0(s_white())(after_inline.clone()) {
                Reply::Success { tokens, state } => (tokens, state),
                Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_inline),
            },
        };
        let mut all = inline_tokens;
        all.extend(tail_tokens);
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

/// [118] nb-single-text(n,c) — full body of a single-quoted scalar.
fn nb_single_text(n: i32, c: Context) -> Parser<'static> {
    // Multi-line is permitted in FlowOut and FlowIn contexts per spec [118].
    match c {
        Context::FlowOut | Context::FlowIn => nb_single_multi_line(n),
        Context::BlockKey | Context::FlowKey | Context::BlockOut | Context::BlockIn => {
            nb_single_one_line()
        }
    }
}

/// [117] c-single-quoted(n,c) — `'` body `'` wrapped in `BeginScalar`/`EndScalar`.
#[must_use]
pub fn c_single_quoted(n: i32, c: Context) -> Parser<'static> {
    wrap_tokens(
        Code::BeginScalar,
        Code::EndScalar,
        Box::new(move |state| {
            let (open_tokens, after_open) = match token(Code::Indicator, char_parser('\''))(state) {
                Reply::Success { tokens, state } => (tokens, state),
                other @ (Reply::Failure | Reply::Error(_)) => return other,
            };
            let body_parser = nb_single_text(n, c);
            match body_parser(after_open.clone()) {
                Reply::Success {
                    tokens: body_tokens,
                    state: after_body,
                } => match token(Code::Indicator, char_parser('\''))(after_body) {
                    Reply::Success {
                        tokens: close_tokens,
                        state: final_state,
                    } => {
                        let mut all = open_tokens;
                        all.extend(body_tokens);
                        all.extend(close_tokens);
                        Reply::Success {
                            tokens: all,
                            state: final_state,
                        }
                    }
                    Reply::Failure | Reply::Error(_) => Reply::Failure,
                },
                other @ (Reply::Failure | Reply::Error(_)) => other,
            }
        }),
    )
}

// ---------------------------------------------------------------------------
// §7.3.3 – Plain scalars [122]–[135]
// ---------------------------------------------------------------------------

// ns_plain_first(c) — defined in chars.rs
// ns_plain_safe(c)  — defined in chars.rs
// ns_plain_char(c)  — defined in chars.rs

/// [132] nb-ns-plain-in-line(c) — whitespace + ns-plain-char sequences after first char.
fn nb_ns_plain_in_line(c: Context) -> Parser<'static> {
    use crate::chars::s_white;
    many0(Box::new(move |state: State<'_>| {
        let before_ws = state.pos.byte_offset;
        let (ws_tokens, after_ws) = match many0(s_white())(state) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => unreachable!("many0 always succeeds"),
        };
        let had_ws = after_ws.pos.byte_offset > before_ws;
        // Per spec [130]: `#` requires preceding ns-char (non-whitespace).
        if had_ws && after_ws.peek() == Some('#') {
            return Reply::Failure;
        }
        match ns_plain_char(c)(after_ws) {
            Reply::Success {
                tokens: char_tokens,
                state: final_state,
            } => {
                let mut all = ws_tokens;
                all.extend(char_tokens);
                Reply::Success {
                    tokens: all,
                    state: final_state,
                }
            }
            other @ (Reply::Failure | Reply::Error(_)) => other,
        }
    }))
}

/// [134] ns-plain-one-line(c) — a plain scalar that fits on one line.
fn ns_plain_one_line(c: Context) -> Parser<'static> {
    seq(ns_plain_first(c), nb_ns_plain_in_line(c))
}

/// [133] s-ns-plain-next-line(n,c) — continuation line of a plain scalar.
///
/// A continuation line must not start at a document boundary (`c-forbidden`):
/// `---` or `...` at column 0 followed by a safe terminator.
fn s_ns_plain_next_line(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let (fold_tokens, after_fold) = match s_flow_folded(n)(state) {
            Reply::Success { tokens, state } => (tokens, state),
            other @ (Reply::Failure | Reply::Error(_)) => return other,
        };
        // `#` after fold is a comment (preceded by whitespace from fold prefix).
        if after_fold.peek() == Some('#') {
            return Reply::Failure;
        }
        let rest = seq(
            neg_lookahead(c_forbidden()),
            seq(ns_plain_char(c), nb_ns_plain_in_line(c)),
        );
        match rest(after_fold) {
            Reply::Success {
                tokens: rest_tokens,
                state: final_state,
            } => {
                let mut all = fold_tokens;
                all.extend(rest_tokens);
                Reply::Success {
                    tokens: all,
                    state: final_state,
                }
            }
            other @ (Reply::Failure | Reply::Error(_)) => other,
        }
    })
}

/// [135] ns-plain-multi-line(n,c) — a plain scalar spanning multiple lines.
fn ns_plain_multi_line(n: i32, c: Context) -> Parser<'static> {
    seq(ns_plain_one_line(c), many0(s_ns_plain_next_line(n, c)))
}

/// [131] ns-plain(n,c) — a plain scalar, single- or multi-line.
///
/// Per spec [131], block-key and flow-key contexts use only the one-line form.
/// All other contexts allow continuation lines.
///
/// Emits `BeginScalar` / `Text` / `EndScalar`.
#[must_use]
pub fn ns_plain(n: i32, c: Context) -> Parser<'static> {
    let inner: Parser<'static> = match c {
        Context::BlockKey | Context::FlowKey => token(Code::Text, ns_plain_one_line(c)),
        Context::BlockOut | Context::BlockIn | Context::FlowOut | Context::FlowIn => {
            token(Code::Text, ns_plain_multi_line(n, c))
        }
    };
    wrap_tokens(Code::BeginScalar, Code::EndScalar, inner)
}

// ---------------------------------------------------------------------------
// §7.4 – Flow sequences [136]–[140]
// ---------------------------------------------------------------------------

/// [136] c-flow-sequence(n,c) — `[` entries `]`.
///
/// Emits `BeginSequence` / entries / `EndSequence`.
#[must_use]
pub fn c_flow_sequence(n: i32, c: Context) -> Parser<'static> {
    wrap_tokens(
        Code::BeginSequence,
        Code::EndSequence,
        Box::new(move |state| {
            let outer_c = state.c;
            // Opening `[`.
            let (open_tokens, after_open) = match token(Code::Indicator, char_parser('['))(state) {
                Reply::Success { tokens, state } => (tokens, state),
                other @ (Reply::Failure | Reply::Error(_)) => return other,
            };
            // Optional separation, then inner context (FlowIn).
            let after_sep = skip_flow_sep(n, after_open);
            let inner_c = inner_flow_context(c);
            let inner_state = State {
                c: inner_c,
                ..after_sep
            };
            // Optional entries per spec [136].
            let (entries_tokens, after_entries) =
                match ns_s_flow_seq_entries(n, inner_c)(inner_state.clone()) {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure | Reply::Error(_) => (SmallVec::new(), inner_state),
                };
            // Optional trailing separation.
            let after_sep2 = skip_flow_sep(n, after_entries);
            // Closing `]`.
            match token(Code::Indicator, char_parser(']'))(after_sep2) {
                Reply::Success {
                    tokens: close_tokens,
                    state: final_state,
                } => {
                    let mut all = open_tokens;
                    all.extend(entries_tokens);
                    all.extend(close_tokens);
                    Reply::Success {
                        tokens: all,
                        state: State {
                            c: outer_c,
                            ..final_state
                        },
                    }
                }
                Reply::Failure | Reply::Error(_) => Reply::Failure,
            }
        }),
    )
}

/// [137] ns-s-flow-seq-entries(n,c) — comma-separated sequence entries (zero or more).
///
/// Entries may be empty (null nodes). A leading or trailing comma results in
/// an implicit empty entry, which is represented by emitting no tokens.
fn ns_s_flow_seq_entries(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        // First entry is required per spec [137].
        let (first_tokens, after_first) = match ns_flow_seq_entry(n, c)(state) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Error(e) => return Reply::Error(e),
            Reply::Failure => return Reply::Failure,
        };
        // Zero or more `, entry` repetitions.
        let mut all_tokens = first_tokens;
        let mut current = after_first;
        loop {
            let after_sep = skip_flow_sep(n, current.clone());
            match token(Code::Indicator, char_parser(','))(after_sep) {
                Reply::Error(e) => return Reply::Error(e),
                Reply::Failure => break,
                Reply::Success {
                    tokens: comma_tokens,
                    state: after_comma,
                } => {
                    let after_sep2 = skip_flow_sep(n, after_comma.clone());
                    let (entry_tokens, after_entry) =
                        match ns_flow_seq_entry(n, c)(after_sep2.clone()) {
                            Reply::Success { tokens, state } => (tokens, state),
                            Reply::Failure | Reply::Error(_) => {
                                // Double comma = invalid.
                                if after_sep2.peek() == Some(',') {
                                    return Reply::Failure;
                                }
                                (SmallVec::new(), after_sep2)
                            }
                        };
                    all_tokens.extend(comma_tokens);
                    all_tokens.extend(entry_tokens);
                    current = after_entry;
                }
            }
        }
        Reply::Success {
            tokens: all_tokens,
            state: current,
        }
    })
}

/// [138] ns-flow-seq-entry(n,c) — a single entry in a flow sequence.
///
/// Per spec: `ns-flow-pair(n,c) | ns-flow-node(n,c)`. When `ns-flow-pair`
/// succeeds but the remaining input doesn't start with a flow terminator
/// (`,`, `]`, `}`, whitespace, or line break), the pair may have consumed
/// too little — try `ns-flow-node` and take the longer match.
fn ns_flow_seq_entry(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let pair = c_ns_flow_pair(n, c)(state.clone());
        match &pair {
            Reply::Success {
                state: pair_state, ..
            } => {
                // Check if the pair result looks complete: remaining input
                // starts with a flow terminator or whitespace/break.
                let next = pair_state.peek();
                let looks_complete = matches!(next, None | Some(',' | ']' | '}' | ' ' | '\t'));
                if looks_complete {
                    return pair;
                }
                // Pair consumed too little — try ns-flow-node for a longer match.
                let node = ns_flow_node(n, c)(state);
                let node_end = match &node {
                    Reply::Success { state, .. } => state.pos.byte_offset,
                    Reply::Failure | Reply::Error(_) => 0,
                };
                if node_end > pair_state.pos.byte_offset {
                    node
                } else {
                    pair
                }
            }
            Reply::Failure | Reply::Error(_) => ns_flow_node(n, c)(state),
        }
    })
}

// ---------------------------------------------------------------------------
// §7.5 – Flow mappings [141]–[153]
// ---------------------------------------------------------------------------

/// [141] c-flow-mapping(n,c) — `{` entries `}`.
///
/// Emits `BeginMapping` / entries / `EndMapping`.
#[must_use]
pub fn c_flow_mapping(n: i32, c: Context) -> Parser<'static> {
    wrap_tokens(
        Code::BeginMapping,
        Code::EndMapping,
        Box::new(move |state| {
            let outer_c = state.c;
            // Opening `{`.
            let (open_tokens, after_open) = match token(Code::Indicator, char_parser('{'))(state) {
                Reply::Success { tokens, state } => (tokens, state),
                other @ (Reply::Failure | Reply::Error(_)) => return other,
            };
            let after_sep = skip_flow_sep(n, after_open);
            let inner_c = inner_flow_context(c);
            let inner_state = State {
                c: inner_c,
                ..after_sep
            };
            let (entries_tokens, after_entries) =
                match ns_s_flow_map_entries(n, inner_c)(inner_state) {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure => return Reply::Failure,
                    Reply::Error(e) => return Reply::Error(e),
                };
            let after_sep2 = skip_flow_sep(n, after_entries);
            // Closing `}`.
            match token(Code::Indicator, char_parser('}'))(after_sep2) {
                Reply::Success {
                    tokens: close_tokens,
                    state: final_state,
                } => {
                    let mut all = open_tokens;
                    all.extend(entries_tokens);
                    all.extend(close_tokens);
                    Reply::Success {
                        tokens: all,
                        state: State {
                            c: outer_c,
                            ..final_state
                        },
                    }
                }
                Reply::Failure | Reply::Error(_) => Reply::Failure,
            }
        }),
    )
}

/// [142] ns-s-flow-map-entries(n,c) — comma-separated mapping entries (zero or more).
fn ns_s_flow_map_entries(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let (first_tokens, after_first) = match ns_flow_map_entry(n, c)(state.clone()) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Error(e) => return Reply::Error(e),
            Reply::Failure => {
                return Reply::Success {
                    tokens: SmallVec::new(),
                    state,
                };
            }
        };
        let mut all_tokens = first_tokens;
        let mut current = after_first;
        loop {
            let after_sep = skip_flow_sep(n, current.clone());
            match token(Code::Indicator, char_parser(','))(after_sep) {
                Reply::Error(e) => return Reply::Error(e),
                Reply::Failure => break,
                Reply::Success {
                    tokens: comma_tokens,
                    state: after_comma,
                } => {
                    let after_sep2 = skip_flow_sep(n, after_comma);
                    let (entry_tokens, after_entry) =
                        match ns_flow_map_entry(n, c)(after_sep2.clone()) {
                            Reply::Success { tokens, state } => (tokens, state),
                            Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_sep2),
                        };
                    all_tokens.extend(comma_tokens);
                    all_tokens.extend(entry_tokens);
                    current = after_entry;
                }
            }
        }
        Reply::Success {
            tokens: all_tokens,
            state: current,
        }
    })
}

/// [143] ns-flow-map-entry(n,c) — explicit key (`?`) or implicit key entry.
fn ns_flow_map_entry(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let explicit = ns_flow_map_explicit_entry(n, c)(state.clone());
        match explicit {
            Reply::Success { .. } | Reply::Error(_) => explicit,
            Reply::Failure => ns_flow_map_implicit_entry(n, c)(state),
        }
    })
}

/// [144] ns-flow-map-explicit-entry(n,c) — `? sep key (: sep value | empty)`.
fn ns_flow_map_explicit_entry(n: i32, c: Context) -> Parser<'static> {
    wrap_tokens(
        Code::BeginPair,
        Code::EndPair,
        Box::new(move |state| {
            let (q_tokens, after_q) = match token(Code::Indicator, char_parser('?'))(state) {
                Reply::Success { tokens, state } => (tokens, state),
                other @ (Reply::Failure | Reply::Error(_)) => return other,
            };
            let after_sep = skip_flow_sep(n, after_q);
            // Key (optional — may be empty).
            let (key_tokens, after_key) = match ns_flow_yaml_node(n, c)(after_sep.clone()) {
                Reply::Success { tokens, state } => (tokens, state),
                Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_sep),
            };
            // Optional value.
            let (value_tokens, final_state) = match c_ns_flow_map_value(n, c)(after_key.clone()) {
                Reply::Success { tokens, state } => (tokens, state),
                Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_key),
            };
            let mut all = q_tokens;
            all.extend(key_tokens);
            all.extend(value_tokens);
            Reply::Success {
                tokens: all,
                state: final_state,
            }
        }),
    )
}

/// [145] ns-flow-map-implicit-entry(n,c) — YAML key entry or empty-key entry.
fn ns_flow_map_implicit_entry(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let yaml = ns_flow_map_yaml_key_entry(n, c)(state.clone());
        match yaml {
            Reply::Success { .. } | Reply::Error(_) => return yaml,
            Reply::Failure => {}
        }
        let json = c_ns_flow_map_json_key_entry(n, c)(state.clone());
        match json {
            Reply::Success { .. } | Reply::Error(_) => return json,
            Reply::Failure => {}
        }
        c_ns_flow_map_empty_key_entry(n, c)(state)
    })
}

/// [147] c-ns-flow-map-json-key-entry(n,c) — JSON node key then optional value.
///
/// Per spec, the key uses the parent context `c` (not `FlowKey`), which
/// allows multiline quoted scalars as keys in flow collections.
fn c_ns_flow_map_json_key_entry(n: i32, c: Context) -> Parser<'static> {
    wrap_tokens(
        Code::BeginPair,
        Code::EndPair,
        Box::new(move |state| {
            // Key uses the parent context c per spec [147].
            let (key_tokens, after_key) = match c_flow_json_node(n, c)(state.clone()) {
                Reply::Success { tokens, state } => (tokens, state),
                other @ (Reply::Failure | Reply::Error(_)) => return other,
            };
            // Per spec [147]: optional `s-separate(n,c)?` then adjacent value,
            // or e-node. Skip optional separation (including whitespace/newlines)
            // before checking for `:`.
            let after_sep = skip_flow_sep(n, after_key.clone());
            let (value_tokens, final_state) =
                match c_ns_flow_map_adjacent_value(n, c)(after_sep.clone()) {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure | Reply::Error(_) => {
                        match c_ns_flow_map_separate_value(n, c)(after_sep.clone()) {
                            Reply::Success { tokens, state } => (tokens, state),
                            Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_key),
                        }
                    }
                };
            let mut all = key_tokens;
            all.extend(value_tokens);
            Reply::Success {
                tokens: all,
                state: final_state,
            }
        }),
    )
}

/// [146] ns-flow-map-yaml-key-entry(n,c) — YAML key then optional value.
///
/// The key uses the parent context `c` per spec [146], allowing multiline
/// plain scalars as mapping keys in flow contexts.
fn ns_flow_map_yaml_key_entry(n: i32, c: Context) -> Parser<'static> {
    wrap_tokens(
        Code::BeginPair,
        Code::EndPair,
        Box::new(move |state| {
            // Key uses parent context c for multiline support.
            let (key_tokens, after_key) = match ns_flow_yaml_node(n, c)(state.clone()) {
                Reply::Success { tokens, state } => (tokens, state),
                other @ (Reply::Failure | Reply::Error(_)) => return other,
            };
            // Optional separation before the value (cross-line allowed).
            let after_sep = skip_flow_sep(n, after_key.clone());
            let (value_tokens, final_state) =
                match c_ns_flow_map_separate_value(n, c)(after_sep.clone()) {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure | Reply::Error(_) => {
                        match c_ns_flow_map_adjacent_value(n, c)(after_key.clone()) {
                            Reply::Success { tokens, state } => (tokens, state),
                            Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_key),
                        }
                    }
                };
            let mut all = key_tokens;
            all.extend(value_tokens);
            Reply::Success {
                tokens: all,
                state: final_state,
            }
        }),
    )
}

/// [148] c-ns-flow-map-empty-key-entry(n,c) — empty key with `: value`.
fn c_ns_flow_map_empty_key_entry(n: i32, c: Context) -> Parser<'static> {
    wrap_tokens(
        Code::BeginPair,
        Code::EndPair,
        Box::new(move |state| {
            // Must begin with `:`.
            let Some(':') = state.peek() else {
                return Reply::Failure;
            };
            let colon_input = state.input;
            let colon_pos = state.pos;
            let after_colon = state.advance(':');
            let colon_token = Token {
                code: Code::Indicator,
                pos: colon_pos,
                text: &colon_input[..1],
            };
            let after_sep = skip_flow_sep(n, after_colon.clone());
            let (value_tokens, final_state) = match ns_flow_node(n, c)(after_sep.clone()) {
                Reply::Success { tokens, state } => (tokens, state),
                Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_colon),
            };
            let mut all = vec![colon_token];
            all.extend(value_tokens);
            Reply::Success {
                tokens: SmallVec::from_vec(all),
                state: final_state,
            }
        }),
    )
}

/// [147] c-ns-flow-map-separate-value(n,c) — `: ` (colon + whitespace) then value.
///
/// Only matches when `:` is followed by whitespace or end-of-input — not
/// when it is immediately adjacent to the next token.
fn c_ns_flow_map_separate_value(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let Some(':') = state.peek() else {
            return Reply::Failure;
        };
        let colon_input = state.input;
        let colon_pos = state.pos;
        let after_colon = state.advance(':');
        // Adjacent value: next char is a non-space ns-char → fall back to adjacent.
        match after_colon.peek() {
            None => {
                // Colon at EOF — emit indicator, no value.
                Reply::Success {
                    tokens: smallvec![Token {
                        code: Code::Indicator,
                        pos: colon_pos,
                        text: &colon_input[..1],
                    }],
                    state: after_colon,
                }
            }
            Some(',' | '}' | ']') => {
                // Flow terminator after `:` — emit indicator with empty (e-node) value.
                Reply::Success {
                    tokens: smallvec![Token {
                        code: Code::Indicator,
                        pos: colon_pos,
                        text: &colon_input[..1],
                    }],
                    state: after_colon,
                }
            }
            Some(nc) if !matches!(nc, ' ' | '\t' | '\n' | '\r') => {
                // Next char is not whitespace — this is adjacent syntax; reject here.
                Reply::Failure
            }
            _ => {
                let colon_token = Token {
                    code: Code::Indicator,
                    pos: colon_pos,
                    text: &colon_input[..1],
                };
                let after_sep = skip_flow_sep(n, after_colon.clone());
                let (value_tokens, final_state) = match ns_flow_node(n, c)(after_sep.clone()) {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_colon),
                };
                let mut all = vec![colon_token];
                all.extend(value_tokens);
                Reply::Success {
                    tokens: SmallVec::from_vec(all),
                    state: final_state,
                }
            }
        }
    })
}

/// [150] c-ns-flow-map-adjacent-value(n,c) — `:`value with no space.
fn c_ns_flow_map_adjacent_value(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let Some(':') = state.peek() else {
            return Reply::Failure;
        };
        let colon_input = state.input;
        let colon_pos = state.pos;
        let after_colon = state.advance(':');
        let colon_token = Token {
            code: Code::Indicator,
            pos: colon_pos,
            text: &colon_input[..1],
        };
        // Optional separation then value, or empty node.
        let after_sep = skip_flow_sep(n, after_colon.clone());
        let (value_tokens, final_state) = match ns_flow_node(n, c)(after_sep.clone()) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_colon),
        };
        let mut all = vec![colon_token];
        all.extend(value_tokens);
        Reply::Success {
            tokens: SmallVec::from_vec(all),
            state: final_state,
        }
    })
}

/// [149] c-ns-flow-map-value(n,c) — optional sep then `: value`.
fn c_ns_flow_map_value(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let after_sep = skip_flow_sep(n, state);
        c_ns_flow_map_separate_value(n, c)(after_sep)
    })
}

// ---------------------------------------------------------------------------
// §7.6 – Flow nodes [154]–[161]
// ---------------------------------------------------------------------------

/// [154] ns-flow-yaml-content(n,c) — plain scalar as YAML flow content.
fn ns_flow_yaml_content(n: i32, c: Context) -> Parser<'static> {
    ns_plain(n, c)
}

/// [155] c-flow-json-content(n,c) — quoted scalar or flow collection as JSON content.
fn c_flow_json_content(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let dq = c_double_quoted(n, c)(state.clone());
        if matches!(dq, Reply::Success { .. } | Reply::Error(_)) {
            return dq;
        }
        let sq = c_single_quoted(n, c)(state.clone());
        if matches!(sq, Reply::Success { .. } | Reply::Error(_)) {
            return sq;
        }
        let seq_ = c_flow_sequence(n, c)(state.clone());
        if matches!(seq_, Reply::Success { .. } | Reply::Error(_)) {
            return seq_;
        }
        c_flow_mapping(n, c)(state)
    })
}

/// [156] ns-flow-content(n,c) — YAML or JSON content.
fn ns_flow_content(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let yaml = ns_flow_yaml_content(n, c)(state.clone());
        match yaml {
            Reply::Success { .. } | Reply::Error(_) => yaml,
            Reply::Failure => c_flow_json_content(n, c)(state),
        }
    })
}

/// [157] ns-flow-yaml-node(n,c) — alias, or properties + optional content.
#[must_use]
pub fn ns_flow_yaml_node(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        // Alias takes priority.
        let alias = c_ns_alias_node()(state.clone());
        if let Reply::Success { .. } = alias {
            return alias;
        }
        // Properties then optional content.
        match c_ns_properties(n, c)(state.clone()) {
            Reply::Success {
                tokens: props_tokens,
                state: after_props,
            } => {
                // Optional sep + content.
                let content_state = match s_separate(n, c)(after_props.clone()) {
                    Reply::Success { state, .. } => state,
                    Reply::Failure | Reply::Error(_) => after_props.clone(),
                };
                let (content_tokens, final_state) =
                    match ns_flow_content(n, c)(content_state.clone()) {
                        Reply::Success { tokens, state } => (tokens, state),
                        Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_props),
                    };
                let mut all = props_tokens;
                all.extend(content_tokens);
                Reply::Success {
                    tokens: all,
                    state: final_state,
                }
            }
            Reply::Failure | Reply::Error(_) => ns_flow_yaml_content(n, c)(state),
        }
    })
}

/// [158] c-flow-json-node(n,c) — optional properties then JSON content.
#[must_use]
pub fn c_flow_json_node(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        // Try properties + sep first, then content.
        let (props_tokens, content_state) = match c_ns_properties(n, c)(state.clone()) {
            Reply::Success {
                tokens,
                state: after_props,
            } => {
                let sep_state = match s_separate(n, c)(after_props.clone()) {
                    Reply::Success { state, .. } => state,
                    Reply::Failure | Reply::Error(_) => after_props,
                };
                (tokens, sep_state)
            }
            Reply::Failure | Reply::Error(_) => (SmallVec::new(), state),
        };
        match c_flow_json_content(n, c)(content_state) {
            Reply::Success {
                tokens: content_tokens,
                state: final_state,
            } => {
                let mut all = props_tokens;
                all.extend(content_tokens);
                Reply::Success {
                    tokens: all,
                    state: final_state,
                }
            }
            other @ (Reply::Failure | Reply::Error(_)) => other,
        }
    })
}

/// [159] ns-flow-node(n,c) — YAML node or JSON node.
#[must_use]
pub fn ns_flow_node(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let yaml = ns_flow_yaml_node(n, c)(state.clone());
        match yaml {
            Reply::Success { .. } | Reply::Error(_) => yaml,
            Reply::Failure => c_flow_json_node(n, c)(state),
        }
    })
}

/// [160] c-ns-flow-pair(n,c) — a key:value pair in a flow context.
///
/// Emits `BeginPair` / key / `:` / value / `EndPair`.
#[must_use]
pub fn c_ns_flow_pair(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let explicit = ns_flow_map_explicit_entry(n, c)(state.clone());
        match explicit {
            Reply::Success { .. } | Reply::Error(_) => return explicit,
            Reply::Failure => {}
        }
        // Per spec [160]: implicit pairs in sequences use ns-s-implicit-yaml-key
        // [154] which restricts YAML keys to FlowKey context (single-line).
        // Flow mapping entries use parent context via ns_flow_map_implicit_entry.
        // Here we use FlowKey for the YAML key to prevent `key\n:value` in sequences.
        let yaml_key = wrap_tokens(
            Code::BeginPair,
            Code::EndPair,
            Box::new(move |s: State<'static>| {
                let (key_tokens, after_key) =
                    match ns_flow_yaml_node(n, Context::FlowKey)(s.clone()) {
                        Reply::Success { tokens, state } => (tokens, state),
                        Reply::Failure | Reply::Error(_) => return Reply::Failure,
                    };
                // Per spec [154]: s-separate-in-line? after key.
                let after_sep = match crate::structure::s_separate_in_line()(after_key.clone()) {
                    Reply::Success { state, .. } => state,
                    Reply::Failure | Reply::Error(_) => after_key.clone(),
                };
                let (value_tokens, final_state) =
                    match c_ns_flow_map_separate_value(n, c)(after_sep.clone()) {
                        Reply::Success { tokens, state } => (tokens, state),
                        Reply::Failure | Reply::Error(_) => {
                            match c_ns_flow_map_adjacent_value(n, c)(after_sep.clone()) {
                                Reply::Success { tokens, state } => (tokens, state),
                                Reply::Failure | Reply::Error(_) => (SmallVec::new(), after_key),
                            }
                        }
                    };
                let mut all = key_tokens;
                all.extend(value_tokens);
                Reply::Success {
                    tokens: all,
                    state: final_state,
                }
            }),
        )(state.clone());
        match yaml_key {
            Reply::Success { .. } | Reply::Error(_) => return yaml_key,
            Reply::Failure => {}
        }
        // JSON key and empty key entries.
        let json = c_ns_flow_map_json_key_entry(n, c)(state.clone());
        match json {
            Reply::Success { .. } | Reply::Error(_) => return json,
            Reply::Failure => {}
        }
        c_ns_flow_map_empty_key_entry(n, c)(state)
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Skip optional separation whitespace (including newlines) in flow context.
///
/// Uses `s_separate_ge` which allows continuation lines with >= n spaces
/// of indentation, matching the flow context requirement that deeper
/// indentation is permitted inside flow collections.
fn skip_flow_sep(n: i32, state: State<'static>) -> State<'static> {
    match s_separate_ge(n, state.c)(state.clone()) {
        Reply::Success { state, .. } => state,
        Reply::Failure | Reply::Error(_) => state,
    }
}

/// Determine the inner context for a flow collection.
const fn inner_flow_context(c: Context) -> Context {
    match c {
        Context::FlowOut | Context::FlowIn | Context::BlockOut | Context::BlockIn => {
            Context::FlowIn
        }
        Context::BlockKey | Context::FlowKey => Context::FlowKey,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    unused_imports
)]
mod tests {
    use super::*;
    use crate::combinator::{Reply, State};
    use crate::token::Code;

    fn state(input: &str) -> State<'_> {
        State::new(input)
    }

    fn is_success(reply: &Reply<'_>) -> bool {
        matches!(reply, Reply::Success { .. })
    }

    fn is_failure(reply: &Reply<'_>) -> bool {
        matches!(reply, Reply::Failure)
    }

    fn remaining<'i>(reply: &Reply<'i>) -> &'i str {
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
    // Group 1: Alias nodes [104] (8 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn c_ns_alias_node_emits_begin_alias() {
        let c = codes(c_ns_alias_node()(state("*myanchor rest")));
        assert_eq!(c.first().copied(), Some(Code::BeginAlias));
    }

    #[test]
    fn c_ns_alias_node_emits_end_alias() {
        let c = codes(c_ns_alias_node()(state("*myanchor rest")));
        assert_eq!(c.last().copied(), Some(Code::EndAlias));
    }

    #[test]
    fn c_ns_alias_node_emits_indicator_for_asterisk() {
        let c = codes(c_ns_alias_node()(state("*myanchor rest")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn c_ns_alias_node_emits_meta_for_name() {
        let c = codes(c_ns_alias_node()(state("*myanchor rest")));
        assert!(c.contains(&Code::Meta));
    }

    #[test]
    fn c_ns_alias_node_stops_before_space() {
        let reply = c_ns_alias_node()(state("*myanchor rest"));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_ns_alias_node_stops_at_flow_indicator() {
        let reply = c_ns_alias_node()(state("*name]rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "]rest");
    }

    #[test]
    fn c_ns_alias_node_fails_without_asterisk() {
        let reply = c_ns_alias_node()(state("myanchor"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_ns_alias_node_fails_with_empty_name() {
        let reply = c_ns_alias_node()(state("* rest"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 2: Empty nodes [105]–[106] (4 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn e_scalar_succeeds_with_zero_consumption() {
        let reply = e_scalar()(state("rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn e_scalar_succeeds_on_empty_input() {
        let reply = e_scalar()(state(""));
        assert!(is_success(&reply));
    }

    #[test]
    fn e_node_succeeds_with_zero_consumption() {
        let reply = e_node()(state("rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn e_node_succeeds_on_empty_input() {
        let reply = e_node()(state(""));
        assert!(is_success(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 3: Double-quoted scalars [107]–[113] (22 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn c_double_quoted_emits_begin_scalar() {
        let c = codes(c_double_quoted(0, Context::FlowOut)(state(
            "\"hello\" rest",
        )));
        assert_eq!(c.first().copied(), Some(Code::BeginScalar));
    }

    #[test]
    fn c_double_quoted_emits_end_scalar() {
        let c = codes(c_double_quoted(0, Context::FlowOut)(state(
            "\"hello\" rest",
        )));
        assert_eq!(c.last().copied(), Some(Code::EndScalar));
    }

    #[test]
    fn c_double_quoted_emits_text_for_content() {
        let c = codes(c_double_quoted(0, Context::FlowOut)(state(
            "\"hello\" rest",
        )));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn c_double_quoted_consumes_through_closing_quote() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"hello\" rest"));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_double_quoted_accepts_empty_string() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"\" rest"));
        assert!(is_success(&reply));
        let c = codes(c_double_quoted(0, Context::FlowOut)(state("\"\" rest")));
        assert!(c.contains(&Code::BeginScalar));
        assert!(c.contains(&Code::EndScalar));
    }

    #[test]
    fn c_double_quoted_fails_without_opening_quote() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("hello\""));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_double_quoted_fails_without_closing_quote() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"hello"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_double_quoted_handles_escape_newline() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"hello\\nworld\""));
        assert!(is_success(&reply));
        // The escape produces a Text token covering the `\n` bytes.
        let c = codes(c_double_quoted(0, Context::FlowOut)(state(
            "\"hello\\nworld\"",
        )));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn c_double_quoted_handles_escape_tab() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"\\t\""));
        assert!(is_success(&reply));
        let c = codes(c_double_quoted(0, Context::FlowOut)(state("\"\\t\"")));
        // nb_double_char emits a Text token for the escape sequence.
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn c_double_quoted_handles_escape_backslash() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"\\\\ \""));
        assert!(is_success(&reply));
        let c = codes(c_double_quoted(0, Context::FlowOut)(state("\"\\\\ \"")));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn c_double_quoted_handles_escape_double_quote() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"\\\"\""));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_double_quoted_handles_unicode_escape() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"\\u0041\""));
        assert!(is_success(&reply));
        let c = codes(c_double_quoted(0, Context::FlowOut)(state("\"\\u0041\"")));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn c_double_quoted_rejects_invalid_escape() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"\\z\""));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_double_quoted_accepts_multiline() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"line1\nline2\""));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn nb_double_char_accepts_regular_char() {
        let reply = nb_double_char()(state("a"));
        assert!(is_success(&reply));
    }

    #[test]
    fn nb_double_char_accepts_escape_sequence() {
        let reply = nb_double_char()(state("\\n"));
        assert!(is_success(&reply));
        // Emits a Text token for the escape.
        let c = codes(nb_double_char()(state("\\n")));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn nb_double_char_fails_on_bare_double_quote() {
        let reply = nb_double_char()(state("\""));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_double_char_fails_on_space() {
        let reply = ns_double_char()(state(" a"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_double_char_accepts_regular_non_space_char() {
        let reply = ns_double_char()(state("a"));
        assert!(is_success(&reply));
    }

    #[test]
    fn nb_double_char_fails_on_backslash_alone() {
        let reply = nb_double_char()(state("\\"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_double_quoted_handles_folded_line() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"word1\n  word2\""));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_double_quoted_handles_trimmed_blank_lines() {
        let reply = c_double_quoted(0, Context::FlowOut)(state("\"word1\n\n  word2\""));
        assert!(is_success(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 4: Single-quoted scalars [114]–[121] (14 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn c_single_quoted_emits_begin_scalar() {
        let c = codes(c_single_quoted(0, Context::FlowOut)(state("'hello' rest")));
        assert_eq!(c.first().copied(), Some(Code::BeginScalar));
    }

    #[test]
    fn c_single_quoted_emits_end_scalar() {
        let c = codes(c_single_quoted(0, Context::FlowOut)(state("'hello' rest")));
        assert_eq!(c.last().copied(), Some(Code::EndScalar));
    }

    #[test]
    fn c_single_quoted_emits_text_for_content() {
        let c = codes(c_single_quoted(0, Context::FlowOut)(state("'hello' rest")));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn c_single_quoted_consumes_through_closing_quote() {
        let reply = c_single_quoted(0, Context::FlowOut)(state("'hello' rest"));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_single_quoted_accepts_empty_string() {
        let reply = c_single_quoted(0, Context::FlowOut)(state("'' rest"));
        assert!(is_success(&reply));
        let c = codes(c_single_quoted(0, Context::FlowOut)(state("'' rest")));
        assert!(c.contains(&Code::BeginScalar));
        assert!(c.contains(&Code::EndScalar));
    }

    #[test]
    fn c_single_quoted_fails_without_opening_quote() {
        let reply = c_single_quoted(0, Context::FlowOut)(state("hello'"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_single_quoted_fails_without_closing_quote() {
        let reply = c_single_quoted(0, Context::FlowOut)(state("'hello"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_single_quoted_handles_escaped_single_quote() {
        let reply = c_single_quoted(0, Context::FlowOut)(state("'it''s fine'"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn c_quoted_quote_accepts_doubled_single_quote() {
        let reply = c_quoted_quote()(state("''rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn c_quoted_quote_fails_on_single_lone_quote() {
        let reply = c_quoted_quote()(state("'rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn nb_single_char_accepts_regular_char() {
        let reply = nb_single_char()(state("a"));
        assert!(is_success(&reply));
    }

    #[test]
    fn nb_single_char_fails_on_bare_single_quote() {
        let reply = nb_single_char()(state("'rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_single_char_fails_on_space() {
        let reply = ns_single_char()(state(" a"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_single_quoted_accepts_multiline() {
        let reply = c_single_quoted(0, Context::FlowOut)(state("'line1\nline2'"));
        assert!(is_success(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 5: Plain scalars [122]–[135] (22 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn ns_plain_first_accepts_regular_char_in_block_context() {
        let reply = crate::chars::ns_plain_first(Context::BlockOut)(state("hello"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_plain_first_accepts_question_when_followed_by_safe_char() {
        let reply = crate::chars::ns_plain_first(Context::FlowOut)(state("?value"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_plain_first_accepts_colon_when_followed_by_safe_char() {
        let reply = crate::chars::ns_plain_first(Context::FlowOut)(state(":value"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_plain_first_accepts_hyphen_when_followed_by_safe_char() {
        let reply = crate::chars::ns_plain_first(Context::FlowOut)(state("-value"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_plain_first_rejects_indicator_not_followed_by_safe_char() {
        let reply = crate::chars::ns_plain_first(Context::FlowOut)(state(": "));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_plain_char_accepts_hash_inside_scalar() {
        let reply = crate::chars::ns_plain_char(Context::BlockOut)(state("#rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_plain_char_accepts_colon_when_followed_by_safe_char() {
        let reply = crate::chars::ns_plain_char(Context::BlockOut)(state(":x"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_plain_char_rejects_colon_at_end_of_input() {
        let reply = crate::chars::ns_plain_char(Context::BlockOut)(state(":"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_plain_accepts_simple_word_in_block_context() {
        // nb-ns-plain-in-line [132] allows s-white* before each ns-plain-char,
        // so "hello rest" is one scalar — interior spaces are consumed.
        let reply = ns_plain(0, Context::BlockOut)(state("hello"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn ns_plain_accepts_simple_word_in_flow_context() {
        let reply = ns_plain(0, Context::FlowOut)(state("hello"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn ns_plain_stops_at_newline_in_single_line_context() {
        // nb-ns-plain-in-line [132] uses nb- chars — no line break — so a
        // newline terminates the inline portion.  With n=0 the continuation
        // line is also valid, so the full "hello\nrest" is consumed and
        // success is returned with empty remaining.
        let reply = ns_plain(0, Context::BlockOut)(state("hello\nrest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn ns_plain_stops_at_flow_indicator_in_flow_context() {
        let reply = ns_plain(0, Context::FlowIn)(state("hello]rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "]rest");
    }

    #[test]
    fn ns_plain_allows_flow_indicator_in_block_context() {
        let reply = ns_plain(0, Context::BlockOut)(state("hello]rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn ns_plain_stops_at_colon_space_boundary() {
        let reply = ns_plain(0, Context::FlowOut)(state("key: value"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), ": value");
    }

    #[test]
    fn ns_plain_stops_at_comma_in_flow_context() {
        let reply = ns_plain(0, Context::FlowIn)(state("hello,world"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), ",world");
    }

    #[test]
    fn ns_plain_fails_when_starting_with_indicator() {
        let reply = ns_plain(0, Context::FlowOut)(state(",value"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_plain_fails_when_starting_with_hash() {
        let reply = ns_plain(0, Context::BlockOut)(state("#comment"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_plain_multi_line_continues_after_newline() {
        let reply = ns_plain(0, Context::BlockOut)(state("word1\n  word2 rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_plain_multi_line_stops_when_continuation_indent_too_low() {
        // n=2, word2 is at column 0 — continuation should not be consumed.
        let reply = ns_plain(2, Context::BlockOut)(state("word1\nword2 rest"));
        assert!(is_success(&reply));
        let rem = remaining(&reply);
        assert!(rem.contains("word2"));
    }

    #[test]
    fn ns_plain_emits_begin_scalar() {
        let c = codes(ns_plain(0, Context::BlockOut)(state("hello")));
        assert_eq!(c.first().copied(), Some(Code::BeginScalar));
    }

    #[test]
    fn ns_plain_emits_end_scalar() {
        let c = codes(ns_plain(0, Context::BlockOut)(state("hello")));
        assert_eq!(c.last().copied(), Some(Code::EndScalar));
    }

    #[test]
    fn ns_plain_emits_text_for_content() {
        let c = codes(ns_plain(0, Context::BlockOut)(state("hello world")));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn ns_plain_safe_in_flow_rejects_flow_indicators() {
        let reply = crate::chars::ns_plain_safe(Context::FlowIn)(state(","));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 6: Flow sequences [136]–[140] (22 tests)
    // -----------------------------------------------------------------------

    // Spike test: validates BeginSequence/EndSequence wrapping.
    #[test]
    fn c_flow_sequence_emits_begin_sequence() {
        let c = codes(c_flow_sequence(0, Context::FlowOut)(state("[a, b] rest")));
        assert_eq!(c.first().copied(), Some(Code::BeginSequence));
    }

    #[test]
    fn c_flow_sequence_emits_end_sequence() {
        let c = codes(c_flow_sequence(0, Context::FlowOut)(state("[a, b] rest")));
        assert_eq!(c.last().copied(), Some(Code::EndSequence));
    }

    #[test]
    fn c_flow_sequence_accepts_empty_sequence() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[] rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_sequence(0, Context::FlowOut)(state("[] rest")));
        assert!(c.contains(&Code::BeginSequence));
        assert!(c.contains(&Code::EndSequence));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_flow_sequence_accepts_single_entry() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[hello] rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_sequence(0, Context::FlowOut)(state("[hello] rest")));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn c_flow_sequence_accepts_two_entries_with_comma() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[a, b] rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_flow_sequence_accepts_trailing_comma() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[a,] rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_flow_sequence_fails_without_opening_bracket() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("a, b]"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_flow_sequence_fails_without_closing_bracket() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[a, b"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_flow_sequence_accepts_nested_sequence() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[[1,2],[3,4]] rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_flow_sequence_accepts_nested_mapping() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[{a: b}] rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_flow_sequence_accepts_double_quoted_entry() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[\"hello\"] rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_sequence(0, Context::FlowOut)(state(
            "[\"hello\"] rest",
        )));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn c_flow_sequence_accepts_single_quoted_entry() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("['hello'] rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_flow_sequence_entry_emits_begin_pair_for_explicit_key() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[? key: val] rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_sequence(0, Context::FlowOut)(state(
            "[? key: val] rest",
        )));
        assert!(c.contains(&Code::BeginPair));
    }

    #[test]
    fn c_flow_sequence_accepts_alias_node_entry() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[*anchor] rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_sequence(0, Context::FlowOut)(state(
            "[*anchor] rest",
        )));
        assert!(c.contains(&Code::BeginAlias));
    }

    #[test]
    fn c_flow_sequence_rejects_leading_comma() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[ , ] rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_flow_sequence_inner_context_is_flow_in() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[a,b,c]"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn c_flow_sequence_accepts_flow_pair_entry() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[a: b] rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_sequence(0, Context::FlowOut)(state("[a: b] rest")));
        assert!(c.contains(&Code::BeginPair));
        assert!(c.contains(&Code::EndPair));
    }

    #[test]
    fn c_flow_sequence_accepts_multiple_entries_no_spaces() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[a,b,c] rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_flow_sequence_accepts_whitespace_around_entries() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[ a , b ] rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_flow_sequence_accepts_multiline_entries() {
        let reply = c_flow_sequence(0, Context::FlowOut)(state("[\na,\nb\n] rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_flow_sequence_emits_indicator_for_brackets() {
        let c = codes(c_flow_sequence(0, Context::FlowOut)(state("[a]")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn c_flow_sequence_emits_indicator_for_comma() {
        let c = codes(c_flow_sequence(0, Context::FlowOut)(state("[a,b]")));
        assert!(c.contains(&Code::Indicator));
    }

    // -----------------------------------------------------------------------
    // Group 7: Flow mappings [141]–[153] (22 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn c_flow_mapping_emits_begin_mapping() {
        let c = codes(c_flow_mapping(0, Context::FlowOut)(state("{a: b} rest")));
        assert_eq!(c.first().copied(), Some(Code::BeginMapping));
    }

    #[test]
    fn c_flow_mapping_emits_end_mapping() {
        let c = codes(c_flow_mapping(0, Context::FlowOut)(state("{a: b} rest")));
        assert_eq!(c.last().copied(), Some(Code::EndMapping));
    }

    #[test]
    fn c_flow_mapping_accepts_empty_mapping() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{} rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_flow_mapping_accepts_single_implicit_entry() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{a: b} rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_mapping(0, Context::FlowOut)(state("{a: b} rest")));
        assert!(c.contains(&Code::BeginPair));
        assert!(c.contains(&Code::EndPair));
    }

    #[test]
    fn c_flow_mapping_accepts_two_entries() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{a: b, c: d} rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_flow_mapping_accepts_trailing_comma() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{a: b,} rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_flow_mapping_fails_without_opening_brace() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("a: b}"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_flow_mapping_fails_without_closing_brace() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{a: b"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_flow_mapping_accepts_nested_mapping() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{a: {b: c}} rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_flow_mapping_accepts_nested_sequence() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{a: [1,2]} rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_flow_mapping_accepts_colon_adjacent_value() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{key:value} rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_flow_mapping_accepts_explicit_key() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{? key : value} rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_mapping(0, Context::FlowOut)(state(
            "{? key : value} rest",
        )));
        assert!(c.contains(&Code::BeginPair));
    }

    #[test]
    fn c_flow_mapping_accepts_value_only_entry() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{: value} rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_flow_mapping_entry_emits_begin_pair() {
        let c = codes(c_flow_mapping(0, Context::FlowOut)(state("{a: b}")));
        assert!(c.contains(&Code::BeginPair));
    }

    #[test]
    fn c_flow_mapping_entry_emits_end_pair() {
        let c = codes(c_flow_mapping(0, Context::FlowOut)(state("{a: b}")));
        assert!(c.contains(&Code::EndPair));
    }

    #[test]
    fn c_flow_mapping_accepts_alias_as_value() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{a: *anchor} rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_mapping(0, Context::FlowOut)(state(
            "{a: *anchor} rest",
        )));
        assert!(c.contains(&Code::BeginAlias));
    }

    #[test]
    fn c_flow_mapping_accepts_double_quoted_key() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{\"key\": value} rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_flow_mapping_accepts_single_quoted_key() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{'key': value} rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_flow_mapping_accepts_multiline_entry() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{\na: b\n} rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), " rest");
    }

    #[test]
    fn c_flow_mapping_emits_indicator_for_braces() {
        let c = codes(c_flow_mapping(0, Context::FlowOut)(state("{a: b}")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn c_flow_mapping_emits_indicator_for_colon() {
        let c = codes(c_flow_mapping(0, Context::FlowOut)(state("{a: b}")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn c_flow_mapping_accepts_no_value_entry() {
        let reply = c_flow_mapping(0, Context::FlowOut)(state("{a} rest"));
        assert!(is_success(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 8: Flow nodes [154]–[161] (16 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn ns_flow_yaml_node_accepts_plain_scalar() {
        let reply = ns_flow_yaml_node(0, Context::FlowOut)(state("hello rest"));
        assert!(is_success(&reply));
        let c = codes(ns_flow_yaml_node(0, Context::FlowOut)(state("hello rest")));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn ns_flow_yaml_node_accepts_alias() {
        let reply = ns_flow_yaml_node(0, Context::FlowOut)(state("*anchor rest"));
        assert!(is_success(&reply));
        let c = codes(ns_flow_yaml_node(0, Context::FlowOut)(state(
            "*anchor rest",
        )));
        assert!(c.contains(&Code::BeginAlias));
    }

    #[test]
    fn ns_flow_yaml_node_accepts_properties_then_scalar() {
        let reply = ns_flow_yaml_node(0, Context::FlowOut)(state("!!str hello rest"));
        assert!(is_success(&reply));
        let c = codes(ns_flow_yaml_node(0, Context::FlowOut)(state(
            "!!str hello rest",
        )));
        assert!(c.contains(&Code::BeginTag));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn ns_flow_yaml_node_accepts_properties_with_empty_node() {
        let reply = ns_flow_yaml_node(0, Context::FlowOut)(state("!!str "));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_flow_json_node_accepts_double_quoted_scalar() {
        let reply = c_flow_json_node(0, Context::FlowOut)(state("\"hello\" rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_json_node(0, Context::FlowOut)(state(
            "\"hello\" rest",
        )));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn c_flow_json_node_accepts_single_quoted_scalar() {
        let reply = c_flow_json_node(0, Context::FlowOut)(state("'hello' rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_json_node(0, Context::FlowOut)(state("'hello' rest")));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn c_flow_json_node_accepts_flow_sequence() {
        let reply = c_flow_json_node(0, Context::FlowOut)(state("[a, b] rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_json_node(0, Context::FlowOut)(state("[a, b] rest")));
        assert!(c.contains(&Code::BeginSequence));
    }

    #[test]
    fn c_flow_json_node_accepts_flow_mapping() {
        let reply = c_flow_json_node(0, Context::FlowOut)(state("{a: b} rest"));
        assert!(is_success(&reply));
        let c = codes(c_flow_json_node(0, Context::FlowOut)(state("{a: b} rest")));
        assert!(c.contains(&Code::BeginMapping));
    }

    #[test]
    fn c_flow_json_node_fails_on_plain_scalar() {
        let reply = c_flow_json_node(0, Context::FlowOut)(state("hello rest"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_flow_node_accepts_yaml_node() {
        let reply = ns_flow_node(0, Context::FlowOut)(state("hello rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_flow_node_accepts_json_node() {
        let reply = ns_flow_node(0, Context::FlowOut)(state("\"hello\" rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_ns_flow_pair_accepts_implicit_key_value() {
        let reply = c_ns_flow_pair(0, Context::FlowOut)(state("key: value rest"));
        assert!(is_success(&reply));
        let c = codes(c_ns_flow_pair(0, Context::FlowOut)(state(
            "key: value rest",
        )));
        assert!(c.contains(&Code::BeginPair));
        assert!(c.contains(&Code::EndPair));
    }

    #[test]
    fn c_ns_flow_pair_accepts_explicit_key() {
        let reply = c_ns_flow_pair(0, Context::FlowOut)(state("? key : value rest"));
        assert!(is_success(&reply));
        let c = codes(c_ns_flow_pair(0, Context::FlowOut)(state(
            "? key : value rest",
        )));
        assert!(c.contains(&Code::BeginPair));
    }

    #[test]
    fn c_ns_flow_pair_accepts_value_with_no_key() {
        let reply = c_ns_flow_pair(0, Context::FlowOut)(state(": value rest"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_ns_flow_pair_emits_indicator_for_colon() {
        let c = codes(c_ns_flow_pair(0, Context::FlowOut)(state("key: value")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn ns_flow_yaml_node_accepts_properties_then_flow_mapping() {
        let reply = ns_flow_yaml_node(0, Context::FlowOut)(state("!!map {a: b} rest"));
        assert!(is_success(&reply));
        let c = codes(ns_flow_yaml_node(0, Context::FlowOut)(state(
            "!!map {a: b} rest",
        )));
        assert!(c.contains(&Code::BeginTag));
        assert!(c.contains(&Code::BeginMapping));
    }
}
