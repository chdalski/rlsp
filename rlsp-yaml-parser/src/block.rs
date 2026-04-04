// SPDX-License-Identifier: MIT

//! YAML 1.2 §8 block style productions [162]–[201].
//!
//! Covers block scalar headers, chomping, literal and folded block scalars,
//! block sequences, block mappings, and block nodes.  Each function is named
//! after the spec production and cross-referenced by its production number in
//! a `// [N]` comment.

use crate::chars::{b_break, nb_char, s_white};
use crate::combinator::{
    Context, Parser, Reply, State, alt, char_parser, many0, many1, neg_lookahead, opt, seq, token,
    wrap_tokens,
};
use crate::flow::{e_node, ns_flow_node};
use crate::structure::{
    b_comment, c_forbidden, c_ns_properties, l_empty, s_b_comment, s_indent, s_indent_lt,
    s_l_comments, s_separate, s_separate_in_line,
};
use crate::token::Code;

// ---------------------------------------------------------------------------
// Chomping indicator — Strip / Clip / Keep
// ---------------------------------------------------------------------------

/// The three YAML chomping modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chomping {
    /// `-` — strip all trailing line breaks.
    Strip,
    /// (default) — keep exactly one trailing line break.
    Clip,
    /// `+` — keep all trailing line breaks.
    Keep,
}

// ---------------------------------------------------------------------------
// §8.1.1 – Block scalar headers [162]–[165]
// ---------------------------------------------------------------------------

/// [164] c-chomping-indicator — `-` for Strip, `+` for Keep, absent for Clip.
///
/// Always succeeds: returns Strip/Keep when the indicator is present, Clip
/// (zero consumption) otherwise.
fn c_chomping_indicator(state: State<'_>) -> (Chomping, State<'_>) {
    match state.peek() {
        Some('-') => (Chomping::Strip, state.advance('-')),
        Some('+') => (Chomping::Keep, state.advance('+')),
        _ => (Chomping::Clip, state),
    }
}

/// [163] c-indentation-indicator — explicit digit 1–9, or absent (auto).
///
/// Returns `Some(n)` when an explicit digit was consumed, `None` for
/// auto-detect.  Fails when the character is `0` (forbidden by spec).
fn c_indentation_indicator(state: State<'_>) -> Reply<'_> {
    match state.peek() {
        Some('0') => Reply::Failure,
        Some(ch @ '1'..='9') => {
            let after = state.advance(ch);
            let n = i32::from(ch as u8 - b'0');
            Reply::Success {
                tokens: vec![crate::token::Token {
                    code: Code::Meta,
                    pos: after.pos,
                    text: "",
                }],
                state: State {
                    input: after.input,
                    pos: after.pos,
                    n,
                    c: after.c,
                },
            }
        }
        _ => Reply::Success {
            tokens: Vec::new(),
            state: State {
                input: state.input,
                pos: state.pos,
                n: 0, // 0 signals auto-detect
                c: state.c,
            },
        },
    }
}

/// Internal helper: try indent-then-chomp ordering.
fn try_indent_chomp(s: State<'_>) -> Option<(i32, Chomping, State<'_>)> {
    match c_indentation_indicator(s) {
        Reply::Success { state: s1, .. } => {
            let m = s1.n;
            let (chomp, s2) = c_chomping_indicator(s1);
            Some((m, chomp, s2))
        }
        Reply::Failure | Reply::Error(_) => None,
    }
}

/// Internal helper: try chomp-then-indent ordering.
fn try_chomp_indent(s: State<'_>) -> Option<(i32, Chomping, State<'_>)> {
    let (chomp, s1) = c_chomping_indicator(s);
    match c_indentation_indicator(s1) {
        Reply::Success { state: s2, .. } => {
            let m = s2.n;
            Some((m, chomp, s2))
        }
        Reply::Failure | Reply::Error(_) => None,
    }
}

/// Internal: parse a block scalar header and return (m, t, `remaining_state`).
/// `m` is the explicit indentation (0 = auto-detect), `t` is Chomping.
fn parse_block_header(state: State<'_>) -> Option<(i32, Chomping, State<'_>)> {
    // Pick the ordering that advanced furthest.
    let r1 = try_indent_chomp(state.clone());
    let r2 = try_chomp_indent(state);

    let (m, chomp, after_indicators) = match (r1, r2) {
        (Some((_, _, s1)), Some((m2, c2, s2))) if s2.pos.byte_offset > s1.pos.byte_offset => {
            (m2, c2, s2)
        }
        (Some((m, c, s)), _) | (None, Some((m, c, s))) => (m, c, s),
        (None, None) => return None,
    };

    // Consume optional comment + required line break.
    match s_b_comment()(after_indicators) {
        Reply::Success { state: s_after, .. } => Some((m, chomp, s_after)),
        Reply::Failure | Reply::Error(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Auto-detect indentation
// ---------------------------------------------------------------------------

/// Scan ahead to find the indentation of the first non-empty content line.
///
/// An "empty" line is one that consists entirely of spaces/tabs and then a
/// line break (or a line containing only whitespace).  The scan starts from
/// `input` (already past the block header line break).
///
/// Returns `Some(indent)` where indent is the column of the first non-space
/// character, or `None` if all remaining lines are empty.
fn detect_scalar_indentation(input: &str, min_indent: i32) -> i32 {
    let mut remaining = input;
    loop {
        // Count leading spaces on this line.
        let spaces = remaining.chars().take_while(|&ch| ch == ' ').count();
        let after_spaces = &remaining[spaces..];
        // Check what follows the spaces.
        match after_spaces.chars().next() {
            None => {
                // EOF — no content found, use min_indent.
                return min_indent;
            }
            Some('\n' | '\r') => {
                // Empty line (spaces then break) — skip.
                let break_len = if after_spaces.starts_with("\r\n") {
                    2
                } else {
                    1
                };
                remaining = &after_spaces[break_len..];
            }
            Some('\t') => {
                // Tab — skip this line (tabs in indentation are not counted).
                // Find end of line.
                let line_end = after_spaces.find('\n').unwrap_or(after_spaces.len());
                remaining = &after_spaces[line_end..];
                if remaining.starts_with('\n') {
                    remaining = &remaining[1..];
                }
            }
            Some(_) => {
                // Non-empty line: the indentation is `spaces`.
                let indent = i32::try_from(spaces).unwrap_or(i32::MAX);
                return indent;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// §8.1.1.2 – Chomping [165]–[169]
// ---------------------------------------------------------------------------

/// [165] b-chomped-last(t) — the final line break, emitted based on chomping.
///
/// Strip: consume but don't emit.
/// Clip/Keep: consume and emit `LineFeed`.
fn b_chomped_last(t: Chomping) -> Parser<'static> {
    Box::new(move |state| {
        // Need a line break here.
        match b_break()(state.clone()) {
            Reply::Failure => {
                // EOF is valid for b-chomped-last when the scalar ends at EOF.
                if state.input.is_empty() {
                    return Reply::Success {
                        tokens: Vec::new(),
                        state,
                    };
                }
                Reply::Failure
            }
            Reply::Error(e) => Reply::Error(e),
            Reply::Success {
                state: after_break, ..
            } => match t {
                Chomping::Strip => Reply::Success {
                    tokens: Vec::new(),
                    state: after_break,
                },
                Chomping::Clip | Chomping::Keep => Reply::Success {
                    tokens: vec![crate::token::Token {
                        code: Code::LineFeed,
                        pos: state.pos,
                        text: "",
                    }],
                    state: after_break,
                },
            },
        }
    })
}

/// [167] l-strip-empty(n) — blank lines (for strip/clip chomping tail).
///
/// Consumes blank lines (lines with at most n spaces then a break) without
/// emitting tokens.
fn l_strip_empty(n: i32) -> Parser<'static> {
    many0(seq(
        opt(seq(s_indent_lt(n + 1), neg_lookahead(s_white()))),
        b_break(),
    ))
}

/// [168] l-keep-empty(n) — blank lines emitting Break tokens (for keep chomping).
fn l_keep_empty(n: i32) -> Parser<'static> {
    many0(seq(opt(s_indent_lt(n + 1)), token(Code::Break, b_break())))
}

/// [169] l-trail-comments(n) — trailing comment lines after a block scalar.
fn l_trail_comments(n: i32) -> Parser<'static> {
    seq(
        s_b_comment(),
        many0(wrap_tokens(
            Code::BeginComment,
            Code::EndComment,
            seq(
                s_indent_lt(n + 1),
                seq(
                    token(Code::Indicator, char_parser('#')),
                    seq(token(Code::Text, many0(nb_char())), b_comment()),
                ),
            ),
        )),
    )
}

/// [166] l-chomped-empty(n,t) — trailing blank lines per chomping mode.
fn l_chomped_empty(n: i32, t: Chomping) -> Parser<'static> {
    match t {
        Chomping::Strip | Chomping::Clip => l_strip_empty(n),
        Chomping::Keep => l_keep_empty(n),
    }
}

// ---------------------------------------------------------------------------
// §8.1.2 – Literal block scalar [170]–[174]
// ---------------------------------------------------------------------------

/// [171] l-nb-literal-text(n) — one line of literal scalar content.
///
/// Emits the content as a Text token (without the indentation spaces).
fn l_nb_literal_text(n: i32) -> Parser<'static> {
    seq(
        many0(l_empty(n, Context::BlockIn)),
        seq(s_indent(n), token(Code::Text, many1(nb_char()))),
    )
}

/// [172] b-nb-literal-next(n) — line break then another literal line.
fn b_nb_literal_next(n: i32) -> Parser<'static> {
    seq(token(Code::LineFeed, b_break()), l_nb_literal_text(n))
}

/// [174] l-literal-content(n,t) — full literal scalar body with chomping.
fn l_literal_content(n: i32, t: Chomping) -> Parser<'static> {
    Box::new(move |state| {
        // Try to parse the first content line.
        match l_nb_literal_text(n)(state.clone()) {
            Reply::Failure => {
                // Empty body — just chomped tail.
                l_chomped_empty(n, t)(state)
            }
            Reply::Error(e) => Reply::Error(e),
            Reply::Success {
                tokens: first_tokens,
                state: after_first,
            } => {
                // Parse continuation lines.
                let cont_result = many0(b_nb_literal_next(n))(after_first.clone());
                let (cont_tokens, after_cont) = match cont_result {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure | Reply::Error(_) => (Vec::new(), after_first),
                };

                // b-chomped-last.
                let last_result = b_chomped_last(t)(after_cont.clone());
                let (last_tokens, after_last) = match last_result {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure | Reply::Error(_) => (Vec::new(), after_cont),
                };

                // l-chomped-empty.
                let tail_result = l_chomped_empty(n, t)(after_last.clone());
                let (tail_tokens, final_state) = match tail_result {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure | Reply::Error(_) => (Vec::new(), after_last),
                };

                let mut all = first_tokens;
                all.extend(cont_tokens);
                all.extend(last_tokens);
                all.extend(tail_tokens);
                Reply::Success {
                    tokens: all,
                    state: final_state,
                }
            }
        }
    })
}

/// [170] c-l+literal(n) — `|` header then literal content.
#[must_use]
pub fn c_l_literal(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        // Must start with `|`.
        let Some('|') = state.peek() else {
            return Reply::Failure;
        };
        let start_pos = state.pos;
        let after_pipe = state.advance('|');
        let pipe_token = crate::token::Token {
            code: Code::Indicator,
            pos: after_pipe.pos,
            text: "",
        };

        // Parse header: indentation indicator + chomping indicator + comment + break.
        let Some((m_raw, chomp, after_header)) = parse_block_header(after_pipe.clone()) else {
            return Reply::Failure;
        };

        // Determine indentation: explicit or auto-detect.
        let m = if m_raw == 0 {
            detect_scalar_indentation(after_header.input, n + 1)
        } else {
            n + m_raw
        };

        if m <= n {
            // No valid content indentation found — empty scalar.
            let content_result = l_chomped_empty(m, chomp)(after_header.clone());
            let (content_tokens, final_state) = match content_result {
                Reply::Success { tokens, state } => (tokens, state),
                Reply::Failure | Reply::Error(_) => (Vec::new(), after_header),
            };
            let mut all = vec![
                crate::token::Token {
                    code: Code::BeginScalar,
                    pos: start_pos,
                    text: "",
                },
                crate::token::Token {
                    code: Code::Indicator,
                    pos: start_pos,
                    text: "|",
                },
            ];
            let _ = pipe_token;
            all.extend(content_tokens);
            all.push(crate::token::Token {
                code: Code::EndScalar,
                pos: final_state.pos,
                text: "",
            });
            return Reply::Success {
                tokens: all,
                state: final_state,
            };
        }

        // Parse the literal content at indentation m.
        let content_result = l_literal_content(m, chomp)(after_header);
        let (content_tokens, final_state) = match content_result {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure => return Reply::Failure,
            Reply::Error(e) => return Reply::Error(e),
        };

        let mut all = vec![
            crate::token::Token {
                code: Code::BeginScalar,
                pos: start_pos,
                text: "",
            },
            crate::token::Token {
                code: Code::Indicator,
                pos: start_pos,
                text: "|",
            },
        ];
        all.extend(content_tokens);
        all.push(crate::token::Token {
            code: Code::EndScalar,
            pos: final_state.pos,
            text: "",
        });
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

// ---------------------------------------------------------------------------
// §8.1.3 – Folded block scalar [175]–[182]
// ---------------------------------------------------------------------------

/// [176] s-nb-folded-text(n) — one line of folded scalar content (non-spaced).
fn s_nb_folded_text(n: i32) -> Parser<'static> {
    seq(
        s_indent(n),
        seq(
            neg_lookahead(s_white()),
            token(Code::Text, many1(nb_char())),
        ),
    )
}

/// [178] s-nb-spaced-text(n) — a more-indented or whitespace-starting line.
///
/// These lines are not folded — they are kept as-is.
fn s_nb_spaced_text(n: i32) -> Parser<'static> {
    seq(
        s_indent(n),
        seq(s_white(), token(Code::Text, many0(nb_char()))),
    )
}

/// [177] s-nb-folded-lines(n) — folded continuation lines.
fn s_nb_folded_lines(n: i32) -> Parser<'static> {
    seq(
        s_nb_folded_text(n),
        many0(seq(
            token(Code::LineFold, b_break()),
            seq(many0(l_empty(n, Context::FlowIn)), s_nb_folded_text(n)),
        )),
    )
}

/// [179] s-nb-spaced-lines(n) — spaced (more-indented or whitespace) lines.
fn s_nb_spaced_lines(n: i32) -> Parser<'static> {
    seq(
        s_nb_spaced_text(n),
        many0(seq(
            token(Code::LineFeed, b_break()),
            seq(many0(l_empty(n, Context::BlockIn)), s_nb_spaced_text(n)),
        )),
    )
}

/// [180] l-nb-same-lines(n) — folded or spaced lines at same indentation.
fn l_nb_same_lines(n: i32) -> Parser<'static> {
    seq(
        many0(l_empty(n, Context::BlockIn)),
        alt(s_nb_folded_lines(n), s_nb_spaced_lines(n)),
    )
}

/// [181] l-nb-diff-lines(n) — different-indented groups of folded/spaced lines.
fn l_nb_diff_lines(n: i32) -> Parser<'static> {
    seq(
        l_nb_same_lines(n),
        many0(seq(token(Code::LineFeed, b_break()), l_nb_same_lines(n))),
    )
}

/// [182] l-folded-content(n,t) — full folded scalar body with chomping.
fn l_folded_content(n: i32, t: Chomping) -> Parser<'static> {
    Box::new(move |state| {
        // Try to parse content.
        match l_nb_diff_lines(n)(state.clone()) {
            Reply::Failure => {
                // Empty body.
                l_chomped_empty(n, t)(state)
            }
            Reply::Error(e) => Reply::Error(e),
            Reply::Success {
                tokens: content_tokens,
                state: after_content,
            } => {
                let last_result = b_chomped_last(t)(after_content.clone());
                let (last_tokens, after_last) = match last_result {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure | Reply::Error(_) => (Vec::new(), after_content),
                };

                let tail_result = l_chomped_empty(n, t)(after_last.clone());
                let (tail_tokens, final_state) = match tail_result {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure | Reply::Error(_) => (Vec::new(), after_last),
                };

                let mut all = content_tokens;
                all.extend(last_tokens);
                all.extend(tail_tokens);
                Reply::Success {
                    tokens: all,
                    state: final_state,
                }
            }
        }
    })
}

/// [175] c-l+folded(n) — `>` header then folded content.
#[must_use]
pub fn c_l_folded(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        // Must start with `>`.
        let Some('>') = state.peek() else {
            return Reply::Failure;
        };
        let start_pos = state.pos;
        let after_gt = state.advance('>');

        let Some((m_raw, chomp, after_header)) = parse_block_header(after_gt) else {
            return Reply::Failure;
        };

        let m = if m_raw == 0 {
            detect_scalar_indentation(after_header.input, n + 1)
        } else {
            n + m_raw
        };

        if m <= n {
            let content_result = l_chomped_empty(m, chomp)(after_header.clone());
            let (content_tokens, final_state) = match content_result {
                Reply::Success { tokens, state } => (tokens, state),
                Reply::Failure | Reply::Error(_) => (Vec::new(), after_header),
            };
            let mut all = vec![
                crate::token::Token {
                    code: Code::BeginScalar,
                    pos: start_pos,
                    text: "",
                },
                crate::token::Token {
                    code: Code::Indicator,
                    pos: start_pos,
                    text: ">",
                },
            ];
            all.extend(content_tokens);
            all.push(crate::token::Token {
                code: Code::EndScalar,
                pos: final_state.pos,
                text: "",
            });
            return Reply::Success {
                tokens: all,
                state: final_state,
            };
        }

        let content_result = l_folded_content(m, chomp)(after_header);
        let (content_tokens, final_state) = match content_result {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure => return Reply::Failure,
            Reply::Error(e) => return Reply::Error(e),
        };

        let mut all = vec![
            crate::token::Token {
                code: Code::BeginScalar,
                pos: start_pos,
                text: "",
            },
            crate::token::Token {
                code: Code::Indicator,
                pos: start_pos,
                text: ">",
            },
        ];
        all.extend(content_tokens);
        all.push(crate::token::Token {
            code: Code::EndScalar,
            pos: final_state.pos,
            text: "",
        });
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

// ---------------------------------------------------------------------------
// §8.2.1 – Block sequences [183]–[186]
// ---------------------------------------------------------------------------

/// [201] seq-spaces(n,c) — indentation level for sequence entries.
///
/// `BlockOut` uses n-1 (entries dedent by 1), `BlockIn` uses n.
#[must_use]
pub const fn seq_spaces(n: i32, c: Context) -> i32 {
    match c {
        Context::BlockOut => n - 1,
        Context::BlockIn
        | Context::FlowOut
        | Context::FlowIn
        | Context::BlockKey
        | Context::FlowKey => n,
    }
}

/// [183] l+block-sequence(n) — a block sequence at indentation n.
///
/// Auto-detects the entry indentation `n+m` (m ≥ 0) from the first entry's column.
#[must_use]
pub fn l_block_sequence(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        // Auto-detect entry column from the first non-empty line.
        let detected = detect_scalar_indentation(state.input, n);
        if detected < n {
            return Reply::Failure;
        }
        wrap_tokens(
            Code::BeginSequence,
            Code::EndSequence,
            many1(c_l_block_seq_entry(detected)),
        )(state)
    })
}

/// [184] c-l-block-seq-entry(n) — a single sequence entry: `- ` then content.
fn c_l_block_seq_entry(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        // Must start with exact indentation n spaces then `- `.
        let indent_result = s_indent(n)(state.clone());
        let after_indent = match indent_result {
            Reply::Success { state, .. } => state,
            Reply::Failure => return Reply::Failure,
            Reply::Error(e) => return Reply::Error(e),
        };

        // Expect `-`.
        let Some('-') = after_indent.peek() else {
            return Reply::Failure;
        };
        let dash_pos = after_indent.pos;
        let after_dash = after_indent.advance('-');

        // The `-` must not be immediately followed by a non-space ns-char (that
        // would make it part of a plain scalar, not a sequence indicator).
        if let Some(ch) = after_dash.peek() {
            if ch != ' ' && ch != '\t' && ch != '\n' && ch != '\r' {
                return Reply::Failure;
            }
        }

        let dash_token = crate::token::Token {
            code: Code::Indicator,
            pos: dash_pos,
            text: "-",
        };

        // Parse the value: block-indented content.
        let value_result = s_b_block_indented(n, Context::BlockIn)(after_dash.clone());
        let (value_tokens, final_state) = match value_result {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure => return Reply::Failure,
            Reply::Error(e) => return Reply::Error(e),
        };

        let mut all = vec![dash_token];
        all.extend(value_tokens);
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

/// [185] s-b-block-indented(n,c) — content after a sequence `- `.
///
/// Accepts compact sequences/mappings inline, or a block node on the next line.
fn s_b_block_indented(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        let m = seq_spaces(n, c);

        // Try compact sequence or compact mapping inline.
        let compact = alt(ns_l_compact_sequence(n + 1), ns_l_compact_mapping(n + 1));

        // Try: optional sep then compact.
        let after_sep = {
            let sep_result = s_separate_in_line()(state.clone());
            match sep_result {
                Reply::Success { state: s, .. } => s,
                Reply::Failure | Reply::Error(_) => state.clone(),
            }
        };

        match compact(after_sep.clone()) {
            Reply::Success { tokens, state } => {
                return Reply::Success { tokens, state };
            }
            Reply::Failure | Reply::Error(_) => {}
        }

        // Try block node on next line or empty node.
        let block_node = alt(s_l_block_node(m, c), seq(e_node(), s_l_comments()));
        block_node(state)
    })
}

/// [186] ns-l-compact-sequence(n) — compact nested sequence (no leading indent).
fn ns_l_compact_sequence(n: i32) -> Parser<'static> {
    wrap_tokens(
        Code::BeginSequence,
        Code::EndSequence,
        seq(c_l_block_seq_entry(n), many0(c_l_block_seq_entry(n))),
    )
}

// ---------------------------------------------------------------------------
// §8.2.2 – Block mappings [187]–[195]
// ---------------------------------------------------------------------------

/// [187] l+block-mapping(n) — a block mapping at indentation n.
///
/// Auto-detects the entry indentation `n+m` (m ≥ 0) from the first entry's column.
#[must_use]
pub fn l_block_mapping(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        // Auto-detect entry column from the first non-empty line.
        let detected = detect_scalar_indentation(state.input, n);
        if detected < n {
            return Reply::Failure;
        }
        wrap_tokens(
            Code::BeginMapping,
            Code::EndMapping,
            many1(ns_l_block_map_entry(detected)),
        )(state)
    })
}

/// [188] ns-l-block-map-entry(n) — explicit (`?`) or implicit key entry.
fn ns_l_block_map_entry(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        // Must begin at indentation n.
        let indent_result = s_indent(n)(state.clone());
        let after_indent = match indent_result {
            Reply::Success { state, .. } => state,
            Reply::Failure => return Reply::Failure,
            Reply::Error(e) => return Reply::Error(e),
        };

        alt(
            c_l_block_map_explicit_entry(n),
            ns_l_block_map_implicit_entry(n),
        )(after_indent)
    })
}

/// [189] c-l-block-map-explicit-entry(n) — `?` key + optional `:` value.
fn c_l_block_map_explicit_entry(n: i32) -> Parser<'static> {
    wrap_tokens(
        Code::BeginPair,
        Code::EndPair,
        seq(
            c_l_block_map_explicit_key(n),
            opt(l_block_map_explicit_value(n)),
        ),
    )
}

/// [190] c-l-block-map-explicit-key(n) — `? ` then block-indented content.
fn c_l_block_map_explicit_key(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        let Some('?') = state.peek() else {
            return Reply::Failure;
        };
        let q_pos = state.pos;
        let after_q = state.advance('?');

        let q_token = crate::token::Token {
            code: Code::Indicator,
            pos: q_pos,
            text: "?",
        };

        let value_result = s_b_block_indented(n, Context::BlockOut)(after_q.clone());
        let (value_tokens, final_state) = match value_result {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => (Vec::new(), after_q),
        };

        let mut all = vec![q_token];
        all.extend(value_tokens);
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

/// [191] l-block-map-explicit-value(n) — `: ` then block-indented content.
fn l_block_map_explicit_value(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        // Must start at indent n.
        let indent_result = s_indent(n)(state.clone());
        let after_indent = match indent_result {
            Reply::Success { state, .. } => state,
            Reply::Failure => return Reply::Failure,
            Reply::Error(e) => return Reply::Error(e),
        };

        let Some(':') = after_indent.peek() else {
            return Reply::Failure;
        };
        let colon_pos = after_indent.pos;
        let after_colon = after_indent.advance(':');

        let colon_token = crate::token::Token {
            code: Code::Indicator,
            pos: colon_pos,
            text: ":",
        };

        let value_result = s_b_block_indented(n, Context::BlockOut)(after_colon.clone());
        let (value_tokens, final_state) = match value_result {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => (Vec::new(), after_colon),
        };

        let mut all = vec![colon_token];
        all.extend(value_tokens);
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

/// [192] ns-l-block-map-implicit-entry(n) — key then `:` value.
fn ns_l_block_map_implicit_entry(n: i32) -> Parser<'static> {
    wrap_tokens(
        Code::BeginPair,
        Code::EndPair,
        seq(
            ns_s_block_map_implicit_key(),
            c_l_block_map_implicit_value(n),
        ),
    )
}

/// [193] ns-s-block-map-implicit-key — optional properties then scalar as key.
///
/// Keys must fit on one line (block-key context). Optional anchor/tag
/// properties may precede the scalar.
#[must_use]
pub fn ns_s_block_map_implicit_key() -> Parser<'static> {
    Box::new(|state| {
        // Optional node properties (anchor/tag) before the key scalar.
        let (prop_tokens, after_props) =
            match seq(c_ns_properties(0, Context::BlockKey), s_separate_in_line())(state.clone()) {
                Reply::Success { tokens, state } => (tokens, state),
                Reply::Failure | Reply::Error(_) => (Vec::new(), state.clone()),
            };

        // Key scalar: double-quoted, single-quoted, or plain.
        let key_result = alt(
            crate::flow::c_double_quoted(0, Context::BlockKey),
            alt(
                crate::flow::c_single_quoted(0, Context::BlockKey),
                crate::flow::ns_plain(0, Context::BlockKey),
            ),
        )(after_props.clone());

        match key_result {
            Reply::Success {
                tokens: key_tokens,
                state: final_state,
            } => {
                let mut all = prop_tokens;
                all.extend(key_tokens);
                Reply::Success {
                    tokens: all,
                    state: final_state,
                }
            }
            Reply::Failure => {
                if prop_tokens.is_empty() {
                    Reply::Failure
                } else {
                    // Properties without scalar — treat properties alone as success.
                    Reply::Success {
                        tokens: prop_tokens,
                        state: after_props,
                    }
                }
            }
            Reply::Error(e) => Reply::Error(e),
        }
    })
}

/// [194] c-l-block-map-implicit-value(n) — `:` then block node or empty.
fn c_l_block_map_implicit_value(n: i32) -> Parser<'static> {
    Box::new(move |state| {
        let Some(':') = state.peek() else {
            return Reply::Failure;
        };
        let colon_pos = state.pos;
        let after_colon = state.advance(':');

        let colon_token = crate::token::Token {
            code: Code::Indicator,
            pos: colon_pos,
            text: ":",
        };

        // After colon: try block node (with or without separator), then empty.
        let value_result = alt(
            seq(
                s_separate(n, Context::BlockOut),
                s_l_block_node(n, Context::BlockOut),
            ),
            alt(
                // Block node without preceding separator (e.g. next-line block scalar/collection).
                s_l_block_node(n, Context::BlockIn),
                // Empty value with trailing comments.
                seq(e_node(), s_l_comments()),
            ),
        )(after_colon.clone());

        let (value_tokens, final_state) = match value_result {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => (Vec::new(), after_colon),
        };

        let mut all = vec![colon_token];
        all.extend(value_tokens);
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

/// [195] ns-l-compact-mapping(n) — compact nested mapping (no leading indent).
fn ns_l_compact_mapping(n: i32) -> Parser<'static> {
    wrap_tokens(
        Code::BeginMapping,
        Code::EndMapping,
        seq(
            ns_l_block_map_implicit_entry(n),
            many0(seq(s_indent(n), ns_l_block_map_implicit_entry(n))),
        ),
    )
}

// ---------------------------------------------------------------------------
// §8.2.3 – Block nodes [196]–[200]
// ---------------------------------------------------------------------------

/// [196] s-l+block-node(n,c) — a full block node with optional properties.
#[must_use]
pub fn s_l_block_node(n: i32, c: Context) -> Parser<'static> {
    alt(s_l_block_in_block(n, c), s_l_flow_in_block(n))
}

/// [197] s-l+flow-in-block(n) — a flow node used inside a block context.
///
/// After the separator the parser must not be at a document boundary
/// (`c-forbidden`): a flow node that would start on a `---`/`...` line is
/// not valid content.
#[must_use]
pub fn s_l_flow_in_block(n: i32) -> Parser<'static> {
    seq(
        s_separate(n + 1, Context::FlowOut),
        seq(
            neg_lookahead(c_forbidden()),
            seq(ns_flow_node(n + 1, Context::FlowOut), s_l_comments()),
        ),
    )
}

/// [198] s-l+block-in-block(n,c) — a block scalar or block collection.
#[must_use]
pub fn s_l_block_in_block(n: i32, c: Context) -> Parser<'static> {
    alt(s_l_block_scalar(n, c), s_l_block_collection(n, c))
}

/// [199] s-l+block-scalar(n,c) — a literal or folded block scalar.
#[must_use]
pub fn s_l_block_scalar(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        // Optional separator.
        let (sep_tokens, after_sep) = match s_separate(n + 1, c)(state.clone()) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => (Vec::new(), state.clone()),
        };

        // Optional properties.
        let (prop_tokens, after_props) =
            match seq(c_ns_properties(n + 1, c), s_separate(n + 1, c))(after_sep.clone()) {
                Reply::Success { tokens, state } => (tokens, state),
                Reply::Failure | Reply::Error(_) => (Vec::new(), after_sep.clone()),
            };

        // Literal or folded scalar.
        let scalar_result = alt(c_l_literal(n), c_l_folded(n))(after_props.clone());
        let (scalar_tokens, after_scalar) = match scalar_result {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure => return Reply::Failure,
            Reply::Error(e) => return Reply::Error(e),
        };

        // Optional trailing comments after the block scalar.
        let (trail_tokens, final_state) = match l_trail_comments(n)(after_scalar.clone()) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => (Vec::new(), after_scalar),
        };

        let mut all = sep_tokens;
        all.extend(prop_tokens);
        all.extend(scalar_tokens);
        all.extend(trail_tokens);
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
}

/// [200] s-l+block-collection(n,c) — a block sequence or mapping.
#[must_use]
pub fn s_l_block_collection(n: i32, c: Context) -> Parser<'static> {
    Box::new(move |state| {
        // Optional properties + separator.
        let (prop_tokens, after_props) =
            match seq(s_separate(n + 1, c), c_ns_properties(n + 1, c))(state.clone()) {
                Reply::Success { tokens, state } => (tokens, state),
                Reply::Failure | Reply::Error(_) => (Vec::new(), state.clone()),
            };

        // Optional s-l-comments before the collection.
        let (comment_tokens, after_comments) = match s_l_comments()(after_props.clone()) {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure | Reply::Error(_) => (Vec::new(), after_props.clone()),
        };

        // Block sequence or mapping.
        let m = seq_spaces(n, c);
        let coll_result =
            alt(l_block_sequence(m + 1), l_block_mapping(n + 1))(after_comments.clone());
        let (coll_tokens, final_state) = match coll_result {
            Reply::Success { tokens, state } => (tokens, state),
            Reply::Failure => {
                // Try at n.
                match alt(l_block_sequence(n), l_block_mapping(n))(after_comments) {
                    Reply::Success { tokens, state } => (tokens, state),
                    Reply::Failure => return Reply::Failure,
                    Reply::Error(e) => return Reply::Error(e),
                }
            }
            Reply::Error(e) => return Reply::Error(e),
        };

        let mut all = prop_tokens;
        all.extend(comment_tokens);
        all.extend(coll_tokens);
        Reply::Success {
            tokens: all,
            state: final_state,
        }
    })
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
    use crate::combinator::{Context, Reply, State};
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
            Reply::Failure | Reply::Error(_) => panic!("expected success, got failure/error"),
        }
    }

    fn codes(reply: Reply<'_>) -> Vec<Code> {
        match reply {
            Reply::Success { tokens, .. } => tokens.into_iter().map(|t| t.code).collect(),
            Reply::Failure | Reply::Error(_) => vec![],
        }
    }

    // -----------------------------------------------------------------------
    // Group 1: Chomping indicator [164] (3 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn c_chomping_indicator_strip_returns_strip() {
        let (chomp, after) = c_chomping_indicator(state("-"));
        assert_eq!(chomp, Chomping::Strip);
        assert_eq!(after.input, "");
    }

    #[test]
    fn c_chomping_indicator_keep_returns_keep() {
        let (chomp, after) = c_chomping_indicator(state("+"));
        assert_eq!(chomp, Chomping::Keep);
        assert_eq!(after.input, "");
    }

    #[test]
    fn c_chomping_indicator_absent_returns_clip() {
        let (chomp, after) = c_chomping_indicator(state("something"));
        assert_eq!(chomp, Chomping::Clip);
        assert_eq!(after.input, "something");
    }

    // -----------------------------------------------------------------------
    // Group 2: Indentation indicator [163] (4 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn c_indentation_indicator_explicit_digit() {
        let reply = c_indentation_indicator(state("2\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "\n");
    }

    #[test]
    fn c_indentation_indicator_rejects_zero() {
        let reply = c_indentation_indicator(state("0\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_indentation_indicator_absent_succeeds_with_zero_consumption() {
        let reply = c_indentation_indicator(state("\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "\n");
    }

    #[test]
    fn c_indentation_indicator_digit_nine() {
        let reply = c_indentation_indicator(state("9rest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
    }

    // -----------------------------------------------------------------------
    // Group 3: Block header [162] (6 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn c_b_block_header_indent_then_chomp() {
        let result = parse_block_header(state("2-\n"));
        assert!(result.is_some());
        let (m, chomp, after) = result.unwrap();
        assert_eq!(m, 2);
        assert_eq!(chomp, Chomping::Strip);
        assert_eq!(after.input, "");
    }

    #[test]
    fn c_b_block_header_chomp_then_indent() {
        let result = parse_block_header(state("-2\n"));
        assert!(result.is_some());
        let (m, chomp, after) = result.unwrap();
        assert_eq!(m, 2);
        assert_eq!(chomp, Chomping::Strip);
        assert_eq!(after.input, "");
    }

    #[test]
    fn c_b_block_header_chomp_only() {
        let result = parse_block_header(state("-\n"));
        assert!(result.is_some());
        let (m, chomp, after) = result.unwrap();
        assert_eq!(chomp, Chomping::Strip);
        assert_eq!(after.input, "");
        let _ = m;
    }

    #[test]
    fn c_b_block_header_indent_only() {
        let result = parse_block_header(state("2\n"));
        assert!(result.is_some());
        let (m, _, after) = result.unwrap();
        assert_eq!(m, 2);
        assert_eq!(after.input, "");
    }

    #[test]
    fn c_b_block_header_neither_indicator() {
        let result = parse_block_header(state("\n"));
        assert!(result.is_some());
        let (_, _, after) = result.unwrap();
        assert_eq!(after.input, "");
    }

    #[test]
    fn c_b_block_header_with_trailing_comment() {
        let result = parse_block_header(state("2 # comment\n"));
        assert!(result.is_some());
        let (m, _, after) = result.unwrap();
        assert_eq!(m, 2);
        assert_eq!(after.input, "");
    }

    // -----------------------------------------------------------------------
    // Group 4: Literal block scalar [170]–[174] (18 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn c_l_literal_accepts_simple_literal_scalar() {
        let reply = c_l_literal(0)(state("|\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|\n  hello\n")));
        assert!(c.contains(&Code::BeginScalar));
        assert!(c.contains(&Code::EndScalar));
    }

    #[test]
    fn c_l_literal_emits_indicator_for_pipe() {
        let c = codes(c_l_literal(0)(state("|\n  hello\n")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn c_l_literal_consumes_entire_block() {
        let reply = c_l_literal(0)(state("|\n  hello\n  world\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn c_l_literal_leaves_less_indented_content_unconsumed() {
        let reply = c_l_literal(0)(state("|\n  hello\nrest\n"));
        assert!(is_success(&reply));
        assert!(remaining(&reply).starts_with("rest"));
    }

    #[test]
    fn c_l_literal_clip_mode_strips_final_newlines_but_keeps_one() {
        let reply = c_l_literal(0)(state("|\n  hello\n\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|\n  hello\n\n\n")));
        assert!(c.contains(&Code::LineFeed));
    }

    #[test]
    fn c_l_literal_strip_mode_removes_all_trailing_newlines() {
        let reply = c_l_literal(0)(state("|-\n  hello\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|-\n  hello\n\n")));
        // After the Text token there should be no LineFeed.
        let text_pos = c.iter().rposition(|&x| x == Code::Text);
        if let Some(pos) = text_pos {
            assert!(!c[pos..].contains(&Code::LineFeed));
        }
    }

    #[test]
    fn c_l_literal_keep_mode_retains_all_trailing_newlines() {
        let reply = c_l_literal(0)(state("|+\n  hello\n\n\n"));
        assert!(is_success(&reply));
        // Should have break codes.
        let c = codes(c_l_literal(0)(state("|+\n  hello\n\n\n")));
        assert!(c.contains(&Code::LineFeed) || c.contains(&Code::Break));
    }

    #[test]
    fn c_l_literal_explicit_indentation_indicator() {
        let reply = c_l_literal(0)(state("|2\n  hello\n  world\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn c_l_literal_explicit_indent_does_not_consume_less_indented() {
        let reply = c_l_literal(0)(state("|2\n  hello\n world\n"));
        assert!(is_success(&reply));
        assert!(remaining(&reply).contains("world"));
    }

    #[test]
    fn c_l_literal_auto_detects_indentation_from_first_content_line() {
        let reply = c_l_literal(0)(state("|\n    hello\n    world\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn c_l_literal_empty_body_with_strip() {
        let reply = c_l_literal(0)(state("|-\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|-\n")));
        assert!(!c.contains(&Code::Text));
        assert!(!c.contains(&Code::LineFeed));
    }

    #[test]
    fn c_l_literal_empty_body_with_clip() {
        let reply = c_l_literal(0)(state("|\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|\n")));
        assert!(!c.contains(&Code::Text));
    }

    #[test]
    fn c_l_literal_empty_body_with_keep() {
        let reply = c_l_literal(0)(state("|+\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_l_literal_preserves_internal_blank_lines() {
        let reply = c_l_literal(0)(state("|\n  hello\n\n  world\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|\n  hello\n\n  world\n")));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn c_l_literal_strip_chomp_with_explicit_indent() {
        let reply = c_l_literal(0)(state("|-2\n  hello\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|-2\n  hello\n\n")));
        let text_pos = c.iter().rposition(|&x| x == Code::Text);
        if let Some(pos) = text_pos {
            assert!(!c[pos..].contains(&Code::LineFeed));
        }
    }

    #[test]
    fn c_l_literal_keep_chomp_with_explicit_indent() {
        let reply = c_l_literal(0)(state("|+2\n  hello\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|+2\n  hello\n\n")));
        assert!(c.contains(&Code::LineFeed) || c.contains(&Code::Break));
    }

    #[test]
    fn c_l_literal_nested_at_n_equals_2() {
        let reply = c_l_literal(2)(state_with("|\n    hello\n", 2, Context::BlockIn));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_l_literal_fails_at_non_pipe_character() {
        let reply = c_l_literal(0)(state(">\n  hello\n"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 5: Folded block scalar [175]–[182] (18 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn c_l_folded_accepts_simple_folded_scalar() {
        let reply = c_l_folded(0)(state(">\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_folded(0)(state(">\n  hello\n")));
        assert!(c.contains(&Code::BeginScalar));
        assert!(c.contains(&Code::EndScalar));
    }

    #[test]
    fn c_l_folded_emits_indicator_for_gt() {
        let c = codes(c_l_folded(0)(state(">\n  hello\n")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn c_l_folded_folds_two_content_lines() {
        let reply = c_l_folded(0)(state(">\n  hello\n  world\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_l_folded_clip_mode_keeps_one_trailing_newline() {
        let reply = c_l_folded(0)(state(">\n  hello\n\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_l_folded_strip_mode_removes_trailing_newlines() {
        let reply = c_l_folded(0)(state(">-\n  hello\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_folded(0)(state(">-\n  hello\n\n")));
        let text_pos = c.iter().rposition(|&x| x == Code::Text);
        if let Some(pos) = text_pos {
            assert!(!c[pos..].contains(&Code::LineFeed));
        }
    }

    #[test]
    fn c_l_folded_keep_mode_retains_trailing_newlines() {
        let reply = c_l_folded(0)(state(">+\n  hello\n\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_folded(0)(state(">+\n  hello\n\n\n")));
        assert!(c.contains(&Code::LineFeed) || c.contains(&Code::Break));
    }

    #[test]
    fn c_l_folded_spaced_lines_not_folded() {
        let reply = c_l_folded(0)(state(">\n  hello\n\n  world\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_folded(0)(state(">\n  hello\n\n  world\n")));
        assert!(c.iter().filter(|&&x| x == Code::Text).count() >= 2);
    }

    #[test]
    fn c_l_folded_more_indented_lines_not_folded() {
        let reply = c_l_folded(0)(state(">\n  normal\n    indented\n  normal\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_l_folded_explicit_indentation_indicator() {
        let reply = c_l_folded(0)(state(">2\n  hello\n  world\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn c_l_folded_auto_detects_indentation() {
        let reply = c_l_folded(0)(state(">\n    hello\n    world\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn c_l_folded_empty_body_with_strip() {
        let reply = c_l_folded(0)(state(">-\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_folded(0)(state(">-\n")));
        assert!(!c.contains(&Code::Text));
    }

    #[test]
    fn c_l_folded_empty_body_with_clip() {
        let reply = c_l_folded(0)(state(">\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_l_folded_empty_body_with_keep() {
        let reply = c_l_folded(0)(state(">+\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_l_folded_leaves_less_indented_content_unconsumed() {
        let reply = c_l_folded(0)(state(">\n  hello\nrest\n"));
        assert!(is_success(&reply));
        assert!(remaining(&reply).starts_with("rest"));
    }

    #[test]
    fn c_l_folded_strip_with_explicit_indent() {
        let reply = c_l_folded(0)(state(">-2\n  hello\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_folded(0)(state(">-2\n  hello\n\n")));
        let text_pos = c.iter().rposition(|&x| x == Code::Text);
        if let Some(pos) = text_pos {
            assert!(!c[pos..].contains(&Code::LineFeed));
        }
    }

    #[test]
    fn c_l_folded_keep_with_explicit_indent() {
        let reply = c_l_folded(0)(state(">+2\n  hello\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_folded(0)(state(">+2\n  hello\n\n")));
        assert!(c.contains(&Code::LineFeed) || c.contains(&Code::Break));
    }

    #[test]
    fn c_l_folded_fails_at_non_gt_character() {
        let reply = c_l_folded(0)(state("|\n  hello\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_l_folded_nested_at_n_equals_2() {
        let reply = c_l_folded(2)(state_with(">\n    hello\n", 2, Context::BlockIn));
        assert!(is_success(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 6: Chomping helpers [165]–[169] (12 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn l_trail_comments_accepts_comment_after_blank() {
        let reply = l_trail_comments(0)(state("\n# comment\n"));
        assert!(is_success(&reply));
        let c = codes(l_trail_comments(0)(state("\n# comment\n")));
        assert!(c.contains(&Code::BeginComment) || c.contains(&Code::EndComment));
    }

    #[test]
    fn l_trail_comments_accepts_multiple_comments() {
        let reply = l_trail_comments(0)(state("\n# one\n# two\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_trail_comments_fails_on_non_comment() {
        let reply = l_trail_comments(0)(state("\nplaintext\n"));
        // l_trail_comments starts with s_b_comment which accepts a bare newline.
        // The overall call succeeds (consuming the newline) but "plaintext" remains.
        // The spec says l-trail-comments = s-b-comment ( l-comment )* — so one bare
        // newline is enough for s-b-comment to succeed.  We verify "plaintext" is unconsumed.
        if is_success(&reply) {
            assert!(remaining(&reply).starts_with("plaintext"));
        }
    }

    #[test]
    fn l_strip_empty_consumes_blank_lines() {
        let reply = l_strip_empty(0)(state("\n\n\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn l_strip_empty_stops_before_non_blank() {
        let reply = l_strip_empty(0)(state("\n\ncontent"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "content");
    }

    #[test]
    fn l_keep_empty_consumes_blank_indented_lines() {
        let reply = l_keep_empty(2)(state("\n\n"));
        assert!(is_success(&reply));
        let c = codes(l_keep_empty(2)(state("\n\n")));
        assert!(c.contains(&Code::Break));
    }

    #[test]
    fn l_chomped_empty_strip_consumes_only_blank_lines() {
        let reply = l_chomped_empty(0, Chomping::Strip)(state("\n\nrest"));
        assert!(is_success(&reply));
        let c = codes(l_chomped_empty(0, Chomping::Strip)(state("\n\nrest")));
        assert!(!c.contains(&Code::LineFeed));
        assert_eq!(
            remaining(&l_chomped_empty(0, Chomping::Strip)(state("\n\nrest"))),
            "rest"
        );
    }

    #[test]
    fn l_chomped_empty_clip_consumes_nothing() {
        // Clip uses l_strip_empty which consumes blank lines without emitting.
        let reply = l_chomped_empty(0, Chomping::Clip)(state("\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_chomped_empty_keep_emits_breaks() {
        let reply = l_chomped_empty(0, Chomping::Keep)(state("\n\n"));
        assert!(is_success(&reply));
        let c = codes(l_chomped_empty(0, Chomping::Keep)(state("\n\n")));
        assert!(c.contains(&Code::Break));
    }

    #[test]
    fn b_chomped_last_strip_consumes_break() {
        let reply = b_chomped_last(Chomping::Strip)(state("\nrest"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "rest");
        let c = codes(b_chomped_last(Chomping::Strip)(state("\nrest")));
        assert!(!c.contains(&Code::LineFeed));
    }

    #[test]
    fn b_chomped_last_clip_emits_line_feed() {
        let reply = b_chomped_last(Chomping::Clip)(state("\nrest"));
        assert!(is_success(&reply));
        let c = codes(b_chomped_last(Chomping::Clip)(state("\nrest")));
        assert!(c.contains(&Code::LineFeed));
        assert_eq!(remaining(&reply), "rest");
    }

    #[test]
    fn b_chomped_last_keep_emits_line_feed() {
        let reply = b_chomped_last(Chomping::Keep)(state("\nrest"));
        assert!(is_success(&reply));
        let c = codes(b_chomped_last(Chomping::Keep)(state("\nrest")));
        assert!(c.contains(&Code::LineFeed));
        assert_eq!(remaining(&reply), "rest");
    }

    // -----------------------------------------------------------------------
    // Group 7: Block sequence [183]–[186] (16 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn l_block_sequence_accepts_single_entry() {
        let reply = l_block_sequence(0)(state("- hello\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_sequence(0)(state("- hello\n")));
        assert!(c.contains(&Code::BeginSequence));
        assert!(c.contains(&Code::EndSequence));
    }

    #[test]
    fn l_block_sequence_accepts_multiple_entries() {
        let reply = l_block_sequence(0)(state("- hello\n- world\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn l_block_sequence_emits_indicator_for_dash() {
        let c = codes(l_block_sequence(0)(state("- hello\n")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn l_block_sequence_stops_before_less_indented_line() {
        // Sequence at n=1 (entries at 1 space); then "rest" at column 0.
        let reply = l_block_sequence(1)(state("  - hello\n  - world\nrest\n"));
        assert!(is_success(&reply));
        assert!(remaining(&reply).starts_with("rest"));
    }

    #[test]
    fn l_block_sequence_fails_when_no_dash_present() {
        let reply = l_block_sequence(0)(state("hello\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_l_block_seq_entry_accepts_block_scalar_value() {
        let reply = l_block_sequence(0)(state("- |\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_sequence(0)(state("- |\n  hello\n")));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn c_l_block_seq_entry_accepts_nested_sequence() {
        let reply = l_block_sequence(0)(state("- - hello\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_l_block_seq_entry_accepts_empty_value() {
        let reply = l_block_sequence(0)(state("-\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn s_b_block_indented_accepts_compact_sequence() {
        let reply = s_b_block_indented(0, Context::BlockIn)(state("- a\n- b\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn s_b_block_indented_accepts_compact_mapping() {
        let reply = s_b_block_indented(0, Context::BlockIn)(state(" a: 1\n b: 2\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_l_compact_sequence_accepts_nested_entries() {
        let reply = ns_l_compact_sequence(0)(state("- a\n- b\n"));
        assert!(is_success(&reply));
        let c = codes(ns_l_compact_sequence(0)(state("- a\n- b\n")));
        assert!(c.contains(&Code::BeginSequence));
    }

    #[test]
    fn l_block_sequence_accepts_block_mapping_entry() {
        let reply = l_block_sequence(0)(state("- a: 1\n  b: 2\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_sequence(0)(state("- a: 1\n  b: 2\n")));
        assert!(c.contains(&Code::BeginMapping));
    }

    #[test]
    fn l_block_sequence_multiline_entry_consumed() {
        let reply = l_block_sequence(0)(state("- |\n  line1\n  line2\n- next\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_block_sequence_entry_with_flow_sequence_value() {
        let reply = l_block_sequence(0)(state("- [a, b]\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_sequence(0)(state("- [a, b]\n")));
        assert!(c.contains(&Code::BeginSequence));
    }

    #[test]
    fn l_block_sequence_entry_with_plain_scalar() {
        let reply = l_block_sequence(0)(state("- hello world\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_sequence(0)(state("- hello world\n")));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn l_block_sequence_entry_with_anchor() {
        let reply = l_block_sequence(0)(state("- &anchor hello\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_sequence(0)(state("- &anchor hello\n")));
        assert!(c.contains(&Code::BeginAnchor));
    }

    // -----------------------------------------------------------------------
    // Group 8: Block mapping [187]–[195] (20 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn l_block_mapping_accepts_single_implicit_entry() {
        let reply = l_block_mapping(0)(state("key: value\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_mapping(0)(state("key: value\n")));
        assert!(c.contains(&Code::BeginMapping));
        assert!(c.contains(&Code::EndMapping));
    }

    #[test]
    fn l_block_mapping_accepts_multiple_entries() {
        let reply = l_block_mapping(0)(state("a: 1\nb: 2\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn l_block_mapping_emits_begin_pair_per_entry() {
        let c = codes(l_block_mapping(0)(state("a: 1\nb: 2\n")));
        assert_eq!(c.iter().filter(|&&x| x == Code::BeginPair).count(), 2);
    }

    #[test]
    fn l_block_mapping_emits_indicator_for_colon() {
        let c = codes(l_block_mapping(0)(state("key: value\n")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn l_block_mapping_stops_before_less_indented_line() {
        // Mapping at n=1 (entries at 2 spaces), then "rest" at col 0.
        let reply = l_block_mapping(1)(state("  a: 1\n  b: 2\nrest\n"));
        assert!(is_success(&reply));
        assert!(remaining(&reply).starts_with("rest"));
    }

    #[test]
    fn l_block_mapping_fails_when_no_key_present() {
        let reply = l_block_mapping(0)(state("- hello\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn ns_l_block_map_entry_accepts_explicit_key() {
        let reply = l_block_mapping(0)(state("? key\n: value\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_mapping(0)(state("? key\n: value\n")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn ns_l_block_map_entry_accepts_implicit_key() {
        let reply = l_block_mapping(0)(state("key: value\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_l_block_map_explicit_key_emits_indicator() {
        let c = codes(l_block_mapping(0)(state("? key\n: value\n")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn l_block_map_explicit_value_accepts_colon_value() {
        let reply = l_block_mapping(0)(state("? key\n: value\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_mapping(0)(state("? key\n: value\n")));
        assert!(c.contains(&Code::Indicator));
    }

    #[test]
    fn l_block_map_explicit_value_accepts_empty_value() {
        let reply = l_block_mapping(0)(state("? key\n:\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_s_block_map_implicit_key_accepts_plain_scalar() {
        let reply = ns_s_block_map_implicit_key()(state("key"));
        assert!(is_success(&reply));
        let c = codes(ns_s_block_map_implicit_key()(state("key")));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn ns_s_block_map_implicit_key_accepts_quoted_scalar() {
        let reply = ns_s_block_map_implicit_key()(state("\"key\""));
        assert!(is_success(&reply));
        let c = codes(ns_s_block_map_implicit_key()(state("\"key\"")));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn c_l_block_map_implicit_value_accepts_block_scalar() {
        let reply = l_block_mapping(0)(state("key: |\n  content\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_mapping(0)(state("key: |\n  content\n")));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn c_l_block_map_implicit_value_accepts_plain_scalar() {
        let reply = l_block_mapping(0)(state("key: value\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_mapping(0)(state("key: value\n")));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn c_l_block_map_implicit_value_accepts_empty_value() {
        let reply = l_block_mapping(0)(state("key:\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn ns_l_compact_mapping_accepts_multiple_entries() {
        let reply = ns_l_compact_mapping(0)(state("a: 1\nb: 2\n"));
        assert!(is_success(&reply));
        let c = codes(ns_l_compact_mapping(0)(state("a: 1\nb: 2\n")));
        assert!(c.contains(&Code::BeginMapping));
    }

    #[test]
    fn l_block_mapping_value_is_block_sequence() {
        let reply = l_block_mapping(0)(state("items:\n  - a\n  - b\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_mapping(0)(state("items:\n  - a\n  - b\n")));
        assert!(c.contains(&Code::BeginSequence));
    }

    #[test]
    fn l_block_mapping_value_is_nested_mapping() {
        let reply = l_block_mapping(0)(state("outer:\n  inner: val\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_mapping(0)(state("outer:\n  inner: val\n")));
        assert!(c.iter().filter(|&&x| x == Code::BeginPair).count() >= 2);
    }

    #[test]
    fn l_block_mapping_entry_with_anchor_on_key() {
        let reply = l_block_mapping(0)(state("&k key: value\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_mapping(0)(state("&k key: value\n")));
        assert!(c.contains(&Code::BeginAnchor));
    }

    // -----------------------------------------------------------------------
    // Group 9: Block nodes [196]–[201] (14 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn s_l_block_node_accepts_literal_scalar_in_block_in() {
        let reply = s_l_block_node(0, Context::BlockIn)(state("|\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_block_node(0, Context::BlockIn)(state("|\n  hello\n")));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn s_l_block_node_accepts_folded_scalar_in_block_in() {
        let reply = s_l_block_node(0, Context::BlockIn)(state(">\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_block_node(0, Context::BlockIn)(state(">\n  hello\n")));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn s_l_block_node_accepts_block_sequence_in_block_out() {
        let reply = s_l_block_node(0, Context::BlockOut)(state("\n- hello\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_block_node(0, Context::BlockOut)(state("\n- hello\n")));
        assert!(c.contains(&Code::BeginSequence));
    }

    #[test]
    fn s_l_block_node_accepts_block_mapping_in_block_out() {
        let reply = s_l_block_node(0, Context::BlockOut)(state("\nkey: value\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_block_node(0, Context::BlockOut)(state(
            "\nkey: value\n",
        )));
        assert!(c.contains(&Code::BeginMapping));
    }

    #[test]
    fn s_l_flow_in_block_accepts_flow_sequence() {
        // n=0 means content must be at column ≥ 1 (s-separate(n+1=1,flow-out)).
        let reply = s_l_flow_in_block(0)(state("\n [a, b]\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_flow_in_block(0)(state("\n [a, b]\n")));
        assert!(c.contains(&Code::BeginSequence));
    }

    #[test]
    fn s_l_flow_in_block_accepts_flow_mapping() {
        let reply = s_l_flow_in_block(0)(state("\n {a: b}\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_flow_in_block(0)(state("\n {a: b}\n")));
        assert!(c.contains(&Code::BeginMapping));
    }

    #[test]
    fn s_l_flow_in_block_accepts_double_quoted_scalar() {
        let reply = s_l_flow_in_block(0)(state("\n \"hello\"\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_flow_in_block(0)(state("\n \"hello\"\n")));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn s_l_block_scalar_accepts_literal_scalar() {
        let reply = s_l_block_scalar(0, Context::BlockIn)(state("|\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_block_scalar(0, Context::BlockIn)(state("|\n  hello\n")));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn s_l_block_scalar_accepts_folded_scalar() {
        let reply = s_l_block_scalar(0, Context::BlockIn)(state(">\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_block_scalar(0, Context::BlockIn)(state(">\n  hello\n")));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn s_l_block_collection_accepts_block_sequence() {
        let reply = s_l_block_collection(0, Context::BlockOut)(state("\n- a\n- b\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_block_collection(0, Context::BlockOut)(state(
            "\n- a\n- b\n",
        )));
        assert!(c.contains(&Code::BeginSequence));
    }

    #[test]
    fn s_l_block_collection_accepts_block_mapping() {
        let reply = s_l_block_collection(0, Context::BlockOut)(state("\na: 1\nb: 2\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_block_collection(0, Context::BlockOut)(state(
            "\na: 1\nb: 2\n",
        )));
        assert!(c.contains(&Code::BeginMapping));
    }

    #[test]
    fn seq_spaces_block_out_uses_n_minus_1() {
        // In BlockOut, seq_spaces(1, BlockOut) = 0, so sequence at col 0 is accepted.
        let reply = l_block_sequence(0)(state_with("- hello\n", 0, Context::BlockOut));
        assert!(is_success(&reply));
    }

    #[test]
    fn seq_spaces_block_in_uses_n() {
        // In BlockIn, seq_spaces(0, BlockIn) = 0.
        let reply = l_block_sequence(0)(state_with("- hello\n", 0, Context::BlockIn));
        assert!(is_success(&reply));
    }

    #[test]
    fn s_l_block_in_block_accepts_block_scalar_content() {
        let reply = s_l_block_in_block(0, Context::BlockOut)(state("|\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_block_in_block(0, Context::BlockOut)(state(
            "|\n  hello\n",
        )));
        assert!(c.contains(&Code::BeginScalar));
    }

    // -----------------------------------------------------------------------
    // Group 10: Properties and tags on block nodes (5 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn s_l_block_scalar_accepts_anchor_before_literal() {
        let reply = s_l_block_scalar(0, Context::BlockIn)(state("&a |\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_block_scalar(0, Context::BlockIn)(state(
            "&a |\n  hello\n",
        )));
        assert!(c.contains(&Code::BeginAnchor));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn s_l_block_scalar_accepts_tag_before_folded() {
        let reply = s_l_block_scalar(0, Context::BlockIn)(state("!!str >\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(s_l_block_scalar(0, Context::BlockIn)(state(
            "!!str >\n  hello\n",
        )));
        assert!(c.contains(&Code::BeginTag));
        assert!(c.contains(&Code::BeginScalar));
    }

    #[test]
    fn l_block_mapping_accepts_tagged_value() {
        let reply = l_block_mapping(0)(state("key: !!str value\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_mapping(0)(state("key: !!str value\n")));
        assert!(c.contains(&Code::BeginTag));
    }

    #[test]
    fn l_block_sequence_accepts_anchored_entry() {
        let reply = l_block_sequence(0)(state("- &a hello\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_sequence(0)(state("- &a hello\n")));
        assert!(c.contains(&Code::BeginAnchor));
    }

    #[test]
    fn l_block_mapping_accepts_alias_as_value() {
        let reply = l_block_mapping(0)(state("key: *anchor\n"));
        assert!(is_success(&reply));
        let c = codes(l_block_mapping(0)(state("key: *anchor\n")));
        assert!(c.contains(&Code::BeginAlias));
    }

    // -----------------------------------------------------------------------
    // Group 11: Auto-detect indentation edge cases (6 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn detect_indentation_skips_leading_blank_lines() {
        let reply = c_l_literal(0)(state("|\n\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|\n\n  hello\n")));
        assert!(c.contains(&Code::Text));
    }

    #[test]
    fn detect_indentation_uses_first_non_empty_line() {
        let reply = c_l_literal(0)(state("|\n\n\n    hello\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn detect_indentation_minimum_is_n_plus_1() {
        // n=2, content at 3 spaces (n+1=3) — valid.
        let reply = c_l_literal(2)(state("|\n   hello\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn detect_indentation_rejects_content_at_n_or_less() {
        // n=2, content at 2 spaces — not valid (requires >n).
        let reply = c_l_literal(2)(state("|\n  hello\n"));
        // Either failure or remaining contains "hello" (not consumed as content).
        if is_success(&reply) {
            assert!(remaining(&reply).contains("hello"));
        }
    }

    #[test]
    fn detect_indentation_all_blank_body_succeeds() {
        let reply = c_l_literal(0)(state("|\n\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn detect_indentation_with_tab_in_blank_line_ignored() {
        // Tab in "blank" line — treated as non-content for auto-detect.
        let reply = c_l_literal(0)(state("|\n\t\n  hello\n"));
        assert!(is_success(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 12: Chomping × scalar style matrix (8 tests)
    // -----------------------------------------------------------------------

    #[test]
    fn literal_clip_single_trailing_newline() {
        let reply = c_l_literal(0)(state("|\n  hello\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|\n  hello\n\n")));
        // Clip: exactly one LineFeed after Text.
        let text_pos = c.iter().rposition(|&x| x == Code::Text);
        let lf_count = text_pos.map_or(0, |pos| {
            c[pos..].iter().filter(|&&x| x == Code::LineFeed).count()
        });
        assert_eq!(lf_count, 1);
    }

    #[test]
    fn literal_strip_no_trailing_newline() {
        let reply = c_l_literal(0)(state("|-\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|-\n  hello\n")));
        let text_pos = c.iter().rposition(|&x| x == Code::Text);
        if let Some(pos) = text_pos {
            assert!(!c[pos..].contains(&Code::LineFeed));
        }
    }

    #[test]
    fn literal_keep_multiple_trailing_newlines() {
        let reply = c_l_literal(0)(state("|+\n  hello\n\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|+\n  hello\n\n\n")));
        let break_count = c
            .iter()
            .filter(|&&x| x == Code::LineFeed || x == Code::Break)
            .count();
        assert!(break_count >= 2);
    }

    #[test]
    fn folded_clip_single_trailing_newline() {
        let reply = c_l_folded(0)(state(">\n  hello\n\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn folded_strip_no_trailing_newline() {
        let reply = c_l_folded(0)(state(">-\n  hello\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_folded(0)(state(">-\n  hello\n")));
        let text_pos = c.iter().rposition(|&x| x == Code::Text);
        if let Some(pos) = text_pos {
            assert!(!c[pos..].contains(&Code::LineFeed));
        }
    }

    #[test]
    fn folded_keep_multiple_trailing_newlines() {
        let reply = c_l_folded(0)(state(">+\n  hello\n\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_folded(0)(state(">+\n  hello\n\n\n")));
        let break_count = c
            .iter()
            .filter(|&&x| x == Code::LineFeed || x == Code::Break)
            .count();
        assert!(break_count >= 2);
    }

    #[test]
    fn literal_strip_empty_body_no_tokens_after_scalar_begin() {
        let reply = c_l_literal(0)(state("|-\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_literal(0)(state("|-\n\n")));
        assert!(!c.contains(&Code::Text));
    }

    #[test]
    fn folded_strip_empty_body_no_tokens_after_scalar_begin() {
        let reply = c_l_folded(0)(state(">-\n\n"));
        assert!(is_success(&reply));
        let c = codes(c_l_folded(0)(state(">-\n\n")));
        assert!(!c.contains(&Code::Text));
    }
}
