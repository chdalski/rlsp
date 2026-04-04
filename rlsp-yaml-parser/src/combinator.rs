// SPDX-License-Identifier: MIT

use crate::pos::Pos;
use crate::token::{Code, Token};

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// YAML 1.2 context modes (spec §6, §7, §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    BlockOut,
    BlockIn,
    FlowOut,
    FlowIn,
    BlockKey,
    FlowKey,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Immutable parser state threaded through every combinator.
///
/// `State` borrows the input slice for the lifetime `'i`.  Positions in the
/// slice are tracked via `pos`; `n` and `c` carry the YAML context parameters
/// (indentation level and context mode) through combinator composition.
#[derive(Debug, Clone)]
pub struct State<'i> {
    /// Remaining (unconsumed) input.
    pub input: &'i str,
    /// Position of the first byte of `input` within the original document.
    pub pos: Pos,
    /// Indentation level `n` (YAML spec parameter).
    pub n: i32,
    /// Context mode `c` (YAML spec parameter).
    pub c: Context,
}

impl<'i> State<'i> {
    /// Construct a fresh state at the beginning of `input`.
    #[must_use]
    pub const fn new(input: &'i str) -> Self {
        Self {
            input,
            pos: Pos::ORIGIN,
            n: 0,
            c: Context::BlockOut,
        }
    }

    /// Construct a state with explicit context parameters.
    #[must_use]
    pub const fn with_context(input: &'i str, n: i32, c: Context) -> Self {
        Self {
            input,
            pos: Pos::ORIGIN,
            n,
            c,
        }
    }

    /// Peek at the next `char` without advancing.
    #[must_use]
    pub fn peek(&self) -> Option<char> {
        self.input.chars().next()
    }

    /// Advance the state past `ch`, returning the updated state.
    ///
    /// The `ch` must equal the first character of `self.input`.
    fn advance(self, ch: char) -> Self {
        let byte_len = ch.len_utf8();
        let new_input = &self.input[byte_len..];
        let new_pos = if ch == '\n' {
            Pos {
                byte_offset: self.pos.byte_offset + byte_len,
                char_offset: self.pos.char_offset + 1,
                line: self.pos.line + 1,
                column: 0,
            }
        } else {
            Pos {
                byte_offset: self.pos.byte_offset + byte_len,
                char_offset: self.pos.char_offset + 1,
                line: self.pos.line,
                column: self.pos.column + 1,
            }
        };
        Self {
            input: new_input,
            pos: new_pos,
            n: self.n,
            c: self.c,
        }
    }
}

// ---------------------------------------------------------------------------
// Reply
// ---------------------------------------------------------------------------

/// The outcome of applying a parser to a `State`.
#[derive(Debug)]
pub enum Reply<'i> {
    /// The parser matched; tokens are accumulated; state is the updated state.
    Success {
        tokens: Vec<Token<'i>>,
        state: State<'i>,
    },
    /// The parser did not match; no input was consumed; the state is
    /// unchanged.  The caller may try an alternative.
    Failure,
    /// The parser encountered an unrecoverable error after committing to a
    /// branch.  Alternatives are not tried — this propagates up the call
    /// stack unchanged.
    Error(ParseError),
}

impl Reply<'_> {
    const fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
}

// ---------------------------------------------------------------------------
// ParseError
// ---------------------------------------------------------------------------

/// A non-recoverable parse error produced after `commit`.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub pos: Pos,
    pub label: &'static str,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Parser type
// ---------------------------------------------------------------------------

/// A parser is a function from `State` to `Reply`.
///
/// Using `Box<dyn Fn>` keeps the API simple and avoids pervasive generic
/// parameters on every combinator.  The hot-path cost is a single
/// indirection per combinator call; this is acceptable for the scaffold phase
/// and can be revisited after profiling.
pub type Parser<'i> = Box<dyn Fn(State<'i>) -> Reply<'i> + 'i>;

// ---------------------------------------------------------------------------
// Primitive parser builders
// ---------------------------------------------------------------------------

/// Match a single character that satisfies `predicate`.
///
/// On success the matched text is the UTF-8 encoding of the character and no
/// token is emitted (use `token()` around this to produce tokens).
#[must_use]
pub fn satisfy<'i, F>(predicate: F) -> Parser<'i>
where
    F: Fn(char) -> bool + 'i,
{
    Box::new(move |state: State<'i>| {
        let Some(ch) = state.peek() else {
            return Reply::Failure;
        };
        if !predicate(ch) {
            return Reply::Failure;
        }
        let new_state = state.advance(ch);
        Reply::Success {
            tokens: Vec::new(),
            state: new_state,
        }
    })
}

/// Match a specific character.
#[must_use]
pub fn char_parser<'i>(expected: char) -> Parser<'i> {
    satisfy(move |ch| ch == expected)
}

/// Always fail without consuming input.
#[must_use]
pub fn fail<'i>() -> Parser<'i> {
    Box::new(|_state: State<'i>| Reply::Failure)
}

// ---------------------------------------------------------------------------
// Core combinators
// ---------------------------------------------------------------------------

/// Sequence: match `a` then `b`, accumulating tokens from both.
///
/// If `a` fails, the whole `seq` fails with no input consumed.
/// If `a` succeeds but `b` fails (or errors), the whole `seq` backtracks to
/// the state before `a`.
#[must_use]
pub fn seq<'i>(a: Parser<'i>, b: Parser<'i>) -> Parser<'i> {
    Box::new(move |state: State<'i>| match a(state) {
        Reply::Failure => Reply::Failure,
        Reply::Error(e) => Reply::Error(e),
        Reply::Success {
            tokens: mut tokens_a,
            state: state_after_a,
        } => match b(state_after_a) {
            Reply::Failure => Reply::Failure,
            Reply::Error(e) => Reply::Error(e),
            Reply::Success {
                tokens: tokens_b,
                state: final_state,
            } => {
                tokens_a.extend(tokens_b);
                Reply::Success {
                    tokens: tokens_a,
                    state: final_state,
                }
            }
        },
    })
}

/// Ordered alternative: try `a`; if it fails (not errors), try `b`.
#[must_use]
pub fn alt<'i>(a: Parser<'i>, b: Parser<'i>) -> Parser<'i> {
    Box::new(move |state: State<'i>| {
        // We must clone here so we can pass the same state to `b` on failure.
        match a(state.clone()) {
            Reply::Failure => b(state),
            other @ (Reply::Success { .. } | Reply::Error(_)) => other,
        }
    })
}

/// Zero-or-more repetition: always succeeds, consuming as many matches as
/// possible.
#[must_use]
pub fn many0<'i>(p: Parser<'i>) -> Parser<'i> {
    Box::new(move |mut state: State<'i>| {
        let mut all_tokens: Vec<Token<'i>> = Vec::new();
        loop {
            match p(state.clone()) {
                Reply::Failure => {
                    return Reply::Success {
                        tokens: all_tokens,
                        state,
                    };
                }
                Reply::Error(e) => return Reply::Error(e),
                Reply::Success { tokens, state: s } => {
                    all_tokens.extend(tokens);
                    state = s;
                }
            }
        }
    })
}

/// One-or-more repetition: fails if there is not at least one match.
#[must_use]
pub fn many1<'i>(p: Parser<'i>) -> Parser<'i> {
    Box::new(move |state: State<'i>| match p(state) {
        Reply::Failure => Reply::Failure,
        Reply::Error(e) => Reply::Error(e),
        Reply::Success {
            tokens: mut first_tokens,
            state: mut current_state,
        } => loop {
            match p(current_state.clone()) {
                Reply::Failure => {
                    return Reply::Success {
                        tokens: first_tokens,
                        state: current_state,
                    };
                }
                Reply::Error(e) => return Reply::Error(e),
                Reply::Success { tokens, state: s } => {
                    first_tokens.extend(tokens);
                    current_state = s;
                }
            }
        },
    })
}

/// Optional: always succeeds; produces an empty result if `p` fails.
#[must_use]
pub fn opt<'i>(p: Parser<'i>) -> Parser<'i> {
    Box::new(move |state: State<'i>| match p(state.clone()) {
        Reply::Failure => Reply::Success {
            tokens: Vec::new(),
            state,
        },
        other @ (Reply::Success { .. } | Reply::Error(_)) => other,
    })
}

/// Exclusion: match `p` only if `q` does not also match at the same position.
///
/// Neither `p` nor `q` consume input when `q` is checked — this is a
/// positive `p` with a negative lookahead for `q`.
#[must_use]
pub fn exclude<'i>(p: Parser<'i>, q: Parser<'i>) -> Parser<'i> {
    Box::new(move |state: State<'i>| {
        // Check q first (lookahead — no input consumed by q).
        if q(state.clone()).is_success() {
            return Reply::Failure;
        }
        p(state)
    })
}

/// Positive lookahead: succeeds if `p` would succeed, but consumes no input
/// and emits no tokens.
#[must_use]
pub fn lookahead<'i>(p: Parser<'i>) -> Parser<'i> {
    Box::new(move |state: State<'i>| match p(state.clone()) {
        Reply::Success { .. } => Reply::Success {
            tokens: Vec::new(),
            state,
        },
        Reply::Failure => Reply::Failure,
        Reply::Error(e) => Reply::Error(e),
    })
}

/// Negative lookahead: succeeds if `p` would *fail*, consumes no input, and
/// emits no tokens.
#[must_use]
pub fn neg_lookahead<'i>(p: Parser<'i>) -> Parser<'i> {
    Box::new(move |state: State<'i>| match p(state.clone()) {
        Reply::Success { .. } => Reply::Failure,
        Reply::Failure => Reply::Success {
            tokens: Vec::new(),
            state,
        },
        // Propagate errors unchanged even in negative lookahead.
        Reply::Error(e) => Reply::Error(e),
    })
}

/// Commit (cut): run `p` and convert any `Failure` into an `Error`,
/// preventing backtracking past this point.
#[must_use]
pub fn commit<'i>(label: &'static str, p: Parser<'i>) -> Parser<'i> {
    Box::new(move |state: State<'i>| {
        let pos = state.pos;
        match p(state) {
            Reply::Failure => Reply::Error(ParseError {
                pos,
                label,
                message: format!("expected {label}"),
            }),
            other @ (Reply::Success { .. } | Reply::Error(_)) => other,
        }
    })
}

/// Emit a `Begin`/`End` token pair around the tokens produced by `p`.
///
/// If `p` fails or errors, no tokens are emitted (no orphaned Begin token).
#[must_use]
pub fn wrap_tokens<'i>(begin: Code, end: Code, p: Parser<'i>) -> Parser<'i> {
    Box::new(move |state: State<'i>| {
        let begin_pos = state.pos;
        match p(state) {
            Reply::Failure => Reply::Failure,
            Reply::Error(e) => Reply::Error(e),
            Reply::Success {
                tokens: inner,
                state: final_state,
            } => {
                let end_pos = final_state.pos;
                let mut tokens = Vec::with_capacity(inner.len() + 2);
                tokens.push(Token {
                    code: begin,
                    pos: begin_pos,
                    text: "",
                });
                tokens.extend(inner);
                tokens.push(Token {
                    code: end,
                    pos: end_pos,
                    text: "",
                });
                Reply::Success {
                    tokens,
                    state: final_state,
                }
            }
        }
    })
}

/// Emit a single token with `code` for the text consumed by `p`.
///
/// The token's position is the position at the start of `p`'s match and the
/// text is the slice of input consumed.
#[must_use]
pub fn token<'i>(code: Code, p: Parser<'i>) -> Parser<'i> {
    Box::new(move |state: State<'i>| {
        let start_pos = state.pos;
        let start_input = state.input;
        match p(state) {
            Reply::Failure => Reply::Failure,
            Reply::Error(e) => Reply::Error(e),
            Reply::Success {
                state: final_state, ..
            } => {
                let consumed_bytes = final_state.pos.byte_offset - start_pos.byte_offset;
                let text = &start_input[..consumed_bytes];
                Reply::Success {
                    tokens: vec![Token {
                        code,
                        pos: start_pos,
                        text,
                    }],
                    state: final_state,
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    // Helper: build a State starting at the origin with BlockOut context.
    fn state(input: &str) -> State<'_> {
        State::new(input)
    }

    // Helper: build a State with explicit pos (for position-tracking tests).
    fn state_at(input: &str, pos: Pos) -> State<'_> {
        State {
            input,
            pos,
            n: 0,
            c: Context::BlockOut,
        }
    }

    fn remaining<'a>(reply: &'a Reply<'a>) -> &'a str {
        match reply {
            Reply::Success { state, .. } => state.input,
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    fn tokens(reply: Reply<'_>) -> Vec<Code> {
        match reply {
            Reply::Success { tokens, .. } => tokens.into_iter().map(|t| t.code).collect(),
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    fn is_failure(reply: &Reply<'_>) -> bool {
        matches!(reply, Reply::Failure)
    }

    fn is_error(reply: &Reply<'_>) -> bool {
        matches!(reply, Reply::Error(_))
    }

    // -----------------------------------------------------------------------
    // seq
    // -----------------------------------------------------------------------

    #[test]
    fn seq_matches_both_parsers_in_order() {
        let p = seq(char_parser('a'), char_parser('b'));
        let reply = p(state("ab"));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn seq_fails_when_first_parser_fails() {
        let p = seq(char_parser('x'), char_parser('b'));
        let reply = p(state("ab"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn seq_fails_when_second_parser_fails() {
        let p = seq(char_parser('a'), char_parser('x'));
        let reply = p(state("ab"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn seq_on_empty_input_fails_when_non_empty_expected() {
        let p = seq(char_parser('a'), char_parser('b'));
        let reply = p(state(""));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // alt
    // -----------------------------------------------------------------------

    #[test]
    fn alt_matches_first_alternative() {
        let p = alt(char_parser('a'), char_parser('b'));
        let reply = p(state("a"));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn alt_falls_through_to_second_when_first_fails() {
        let p = alt(char_parser('a'), char_parser('b'));
        let reply = p(state("b"));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn alt_fails_when_both_alternatives_fail() {
        let p = alt(char_parser('a'), char_parser('b'));
        let reply = p(state("c"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn alt_does_not_try_second_when_first_matches() {
        // The second parser always produces an Error (not a Failure).
        // If alt tried it after a successful first branch, we would see Error.
        let p = alt(
            char_parser('a'),
            Box::new(|_s: State<'_>| {
                Reply::Error(ParseError {
                    pos: Pos::ORIGIN,
                    label: "should not be tried",
                    message: "alt tried second branch after first succeeded".into(),
                })
            }),
        );
        let reply = p(state("a"));
        // Should be Success, not Error
        assert!(matches!(reply, Reply::Success { .. }));
    }

    // -----------------------------------------------------------------------
    // many0
    // -----------------------------------------------------------------------

    #[test]
    fn many0_matches_zero_occurrences() {
        let p = many0(char_parser('a'));
        let reply = p(state("b"));
        assert_eq!(remaining(&reply), "b");
    }

    #[test]
    fn many0_matches_multiple_occurrences() {
        let p = many0(char_parser('a'));
        let reply = p(state("aaab"));
        assert_eq!(remaining(&reply), "b");
    }

    #[test]
    fn many0_on_empty_input_succeeds_with_empty_result() {
        let p = many0(char_parser('a'));
        let reply = p(state(""));
        assert_eq!(remaining(&reply), "");
    }

    // -----------------------------------------------------------------------
    // many1
    // -----------------------------------------------------------------------

    #[test]
    fn many1_fails_when_no_occurrences() {
        let p = many1(char_parser('a'));
        let reply = p(state("b"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn many1_matches_single_occurrence() {
        let p = many1(char_parser('a'));
        let reply = p(state("ab"));
        assert_eq!(remaining(&reply), "b");
    }

    #[test]
    fn many1_matches_multiple_occurrences() {
        let p = many1(char_parser('a'));
        let reply = p(state("aaab"));
        assert_eq!(remaining(&reply), "b");
    }

    // -----------------------------------------------------------------------
    // opt
    // -----------------------------------------------------------------------

    #[test]
    fn opt_returns_success_when_parser_matches() {
        let p = opt(char_parser('a'));
        let reply = p(state("ab"));
        assert_eq!(remaining(&reply), "b");
    }

    #[test]
    fn opt_returns_success_when_parser_does_not_match() {
        let p = opt(char_parser('a'));
        let reply = p(state("b"));
        assert!(matches!(&reply, Reply::Success { .. }));
        assert_eq!(remaining(&reply), "b");
    }

    #[test]
    fn opt_always_succeeds_on_empty_input() {
        let p = opt(char_parser('a'));
        let reply = p(state(""));
        assert!(matches!(&reply, Reply::Success { .. }));
    }

    // -----------------------------------------------------------------------
    // exclude
    // -----------------------------------------------------------------------

    #[test]
    fn exclude_succeeds_when_p_matches_and_q_does_not() {
        let p = exclude(char_parser('a'), char_parser('b'));
        let reply = p(state("a"));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn exclude_fails_when_both_p_and_q_match() {
        // p = 'a', q = 'a' — both match the same input
        let p = exclude(char_parser('a'), char_parser('a'));
        let reply = p(state("a"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn exclude_fails_when_p_does_not_match() {
        let p = exclude(char_parser('a'), char_parser('b'));
        let reply = p(state("b"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // lookahead
    // -----------------------------------------------------------------------

    #[test]
    fn lookahead_succeeds_without_consuming_input() {
        let p = lookahead(char_parser('a'));
        let reply = p(state("abc"));
        assert_eq!(remaining(&reply), "abc");
    }

    #[test]
    fn lookahead_fails_when_parser_fails() {
        let p = lookahead(char_parser('x'));
        let reply = p(state("abc"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // neg_lookahead
    // -----------------------------------------------------------------------

    #[test]
    fn neg_lookahead_succeeds_when_parser_fails() {
        let p = neg_lookahead(char_parser('x'));
        let reply = p(state("abc"));
        assert!(matches!(&reply, Reply::Success { .. }));
        assert_eq!(remaining(&reply), "abc");
    }

    #[test]
    fn neg_lookahead_fails_when_parser_succeeds() {
        let p = neg_lookahead(char_parser('a'));
        let reply = p(state("abc"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // commit
    // -----------------------------------------------------------------------

    #[test]
    fn commit_succeeds_and_inner_parser_output_is_preserved() {
        let p = commit("char_a", char_parser('a'));
        let reply = p(state("a"));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn commit_failure_becomes_error_not_backtrackable_failure() {
        // alt(seq('a', commit("x", 'x')), 'a') on "ab":
        // - First branch: 'a' matches, commit("x",'x') fails → Error
        // - alt sees Error (not Failure) → does NOT try second branch
        let p = alt(
            seq(char_parser('a'), commit("after_a", char_parser('x'))),
            char_parser('a'),
        );
        let reply = p(state("ab"));
        assert!(is_error(&reply));
    }

    // -----------------------------------------------------------------------
    // wrap_tokens
    // -----------------------------------------------------------------------

    #[test]
    fn wrap_tokens_emits_begin_token_first() {
        let p = wrap_tokens(Code::BeginMapping, Code::EndMapping, char_parser('a'));
        let codes = tokens(p(state("a")));
        assert_eq!(codes.first().copied(), Some(Code::BeginMapping));
    }

    #[test]
    fn wrap_tokens_emits_end_token_last() {
        let p = wrap_tokens(Code::BeginMapping, Code::EndMapping, char_parser('a'));
        let codes = tokens(p(state("a")));
        assert_eq!(codes.last().copied(), Some(Code::EndMapping));
    }

    #[test]
    fn wrap_tokens_inner_tokens_are_between_begin_and_end() {
        // Use token() to produce inner tokens for 'h' and 'i'.
        let p = wrap_tokens(
            Code::BeginScalar,
            Code::EndScalar,
            seq(
                token(Code::Text, char_parser('h')),
                token(Code::Text, char_parser('i')),
            ),
        );
        let codes = tokens(p(state("hi")));
        assert_eq!(
            codes,
            vec![Code::BeginScalar, Code::Text, Code::Text, Code::EndScalar]
        );
    }

    #[test]
    fn wrap_tokens_on_inner_failure_emits_no_tokens() {
        let p = wrap_tokens(Code::BeginMapping, Code::EndMapping, char_parser('x'));
        let reply = p(state("a"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // token
    // -----------------------------------------------------------------------

    #[test]
    fn token_emits_token_with_correct_code() {
        let p = token(Code::Text, char_parser('a'));
        let codes = tokens(p(state("a")));
        assert_eq!(codes, vec![Code::Text]);
    }

    #[test]
    fn token_emits_token_with_correct_position() {
        let start_pos = Pos {
            byte_offset: 5,
            char_offset: 5,
            line: 3,
            column: 2,
        };
        let p = token(Code::Text, char_parser('a'));
        let reply = p(state_at("a", start_pos));
        match reply {
            Reply::Success { tokens, .. } => {
                assert_eq!(tokens.len(), 1);
                assert_eq!(tokens[0].pos, start_pos);
            }
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    // -----------------------------------------------------------------------
    // Position tracking
    // -----------------------------------------------------------------------

    #[test]
    fn position_advances_by_byte_and_char_after_ascii_match() {
        let p = char_parser('a');
        let reply = p(state("ab"));
        match reply {
            Reply::Success { state, .. } => {
                assert_eq!(state.pos.byte_offset, 1);
                assert_eq!(state.pos.char_offset, 1);
                assert_eq!(state.pos.column, 1);
                assert_eq!(state.pos.line, 1);
            }
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    #[test]
    fn position_advances_correctly_after_newline() {
        let p = char_parser('\n');
        let reply = p(state("\n"));
        match reply {
            Reply::Success { state, .. } => {
                assert_eq!(state.pos.line, 2);
                assert_eq!(state.pos.column, 0);
                assert_eq!(state.pos.byte_offset, 1);
            }
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    #[test]
    fn position_advances_by_correct_byte_count_for_multibyte_char() {
        // 'é' (U+00E9) is 2 bytes in UTF-8
        let p = char_parser('é');
        let reply = p(state("é"));
        match reply {
            Reply::Success { state, .. } => {
                assert_eq!(state.pos.byte_offset, 2);
                assert_eq!(state.pos.char_offset, 1);
            }
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    #[test]
    fn position_advances_by_correct_byte_count_for_three_byte_char() {
        // '中' (U+4E2D) is 3 bytes in UTF-8
        let p = char_parser('中');
        let reply = p(state("中"));
        match reply {
            Reply::Success { state, .. } => {
                assert_eq!(state.pos.byte_offset, 3);
                assert_eq!(state.pos.char_offset, 1);
            }
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    // -----------------------------------------------------------------------
    // Context threading
    // -----------------------------------------------------------------------

    #[test]
    fn state_carries_indentation_level() {
        let p = char_parser('a');
        let s = State::with_context("a", 4, Context::BlockOut);
        match p(s) {
            Reply::Success { state, .. } => assert_eq!(state.n, 4),
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    #[test]
    fn state_carries_context_mode() {
        let p = char_parser('a');
        let s = State::with_context("a", 0, Context::FlowIn);
        match p(s) {
            Reply::Success { state, .. } => {
                assert_eq!(state.c, Context::FlowIn);
            }
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    #[test]
    fn context_enum_has_all_six_variants() {
        let ctx = Context::BlockOut;
        let _ = match ctx {
            Context::BlockOut => 0,
            Context::BlockIn => 1,
            Context::FlowOut => 2,
            Context::FlowIn => 3,
            Context::BlockKey => 4,
            Context::FlowKey => 5,
        };
    }

    // -----------------------------------------------------------------------
    // Combinator composition
    // -----------------------------------------------------------------------

    #[test]
    fn composed_combinators_parse_simple_sequence_correctly() {
        // seq(many1('a'), seq(':', many0(' '))) on "aaa: "
        let p = seq(
            many1(char_parser('a')),
            seq(char_parser(':'), many0(char_parser(' '))),
        );
        let reply = p(state("aaa: "));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn alt_of_seq_correctly_backtracks_on_partial_match() {
        // alt(seq('a','b'), seq('a','c')) on "ac" — first branch fails after
        // matching 'a', second branch succeeds.
        let p = alt(
            seq(char_parser('a'), char_parser('b')),
            seq(char_parser('a'), char_parser('c')),
        );
        let reply = p(state("ac"));
        assert_eq!(remaining(&reply), "");
    }
}
