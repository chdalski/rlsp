// SPDX-License-Identifier: MIT

//! YAML 1.2 §9 document stream productions [202]–[211].
//!
//! Covers document markers, forbidden positions, document prefixes, bare
//! documents, explicit documents, directive documents, and the full YAML
//! stream.  Each function is named after the spec production and
//! cross-referenced by its production number in a `// [N]` comment.
//!
//! The public entry point is [`tokenize`], which takes a YAML string slice
//! and returns all tokens produced by parsing the entire stream.

use crate::block::s_l_block_node;
use crate::chars::b_break;
use crate::combinator::{
    Context, Parser, Reply, State, alt, char_parser, many0, many1, neg_lookahead, opt, seq, token,
    wrap_tokens,
};
use crate::structure::{c_forbidden, l_directive, s_l_comments};
use crate::token::{Code, Token};

// ---------------------------------------------------------------------------
// §9.1 – Document boundary markers [202]–[203]
// ---------------------------------------------------------------------------

/// [202] c-directives-end — `---` at the start of a line.
///
/// Emits a single `DirectivesEnd` token.
#[must_use]
pub fn c_directives_end() -> Parser<'static> {
    token(
        Code::DirectivesEnd,
        seq(char_parser('-'), seq(char_parser('-'), char_parser('-'))),
    )
}

/// [203] c-document-end — `...` at the start of a line.
///
/// Emits a single `DocumentEnd` token.
#[must_use]
pub fn c_document_end() -> Parser<'static> {
    token(
        Code::DocumentEnd,
        seq(char_parser('.'), seq(char_parser('.'), char_parser('.'))),
    )
}

// ---------------------------------------------------------------------------
// §9.2 – Document prefix [204]
// ---------------------------------------------------------------------------

/// [204] l-document-prefix — optional BOM then zero or more comment lines.
///
/// The BOM (`\u{FEFF}`) may appear at the start of the stream to indicate
/// encoding.  After the optional BOM, any number of comment-or-blank lines
/// may precede the first document.
#[must_use]
pub fn l_document_prefix() -> Parser<'static> {
    seq(opt(char_parser('\u{FEFF}')), many0(l_comment_line()))
}

/// A single blank-or-comment line: optional `# …` then a line break.
fn l_comment_line() -> Parser<'static> {
    seq(opt(c_nb_comment_text()), b_break())
}

/// `# <non-break-chars>*` — comment text without the leading whitespace.
fn c_nb_comment_text() -> Parser<'static> {
    seq(char_parser('#'), many0(crate::chars::nb_char()))
}

// ---------------------------------------------------------------------------
// §9.2 – Documents [206]–[208]
// ---------------------------------------------------------------------------

/// [206] l-bare-document — a document with no `---` prefix.
///
/// The document content is a block node at indentation −1 (top level).
/// The parser stops before any `c-forbidden` position so that document
/// boundaries inside a stream are respected.
///
/// Emits `BeginDocument` / content tokens / `EndDocument`.
#[must_use]
pub fn l_bare_document() -> Parser<'static> {
    wrap_tokens(
        Code::BeginDocument,
        Code::EndDocument,
        seq(
            neg_lookahead(c_forbidden()),
            s_l_block_node(-1, Context::BlockIn),
        ),
    )
}

/// [207] l-explicit-document — a document introduced by `---`.
///
/// The `---` marker is followed by optional block content, then an optional
/// `...` end marker with trailing comments.
///
/// Emits `BeginDocument` / `DirectivesEnd` token / optional content /
/// optional `DocumentEnd` token / `EndDocument`.
#[must_use]
pub fn l_explicit_document() -> Parser<'static> {
    wrap_tokens(
        Code::BeginDocument,
        Code::EndDocument,
        seq(
            c_directives_end(),
            seq(
                opt(l_document_content()),
                opt(seq(c_document_end(), s_l_comments())),
            ),
        ),
    )
}

/// Content of an explicit document: block node or empty node with comments.
fn l_document_content() -> Parser<'static> {
    alt(s_l_block_node(-1, Context::BlockIn), s_l_comments())
}

/// [208] l-directive-document — one or more directives followed by `---`.
///
/// Emits `BeginDocument` / directive tokens / `DirectivesEnd` token /
/// optional content / optional `DocumentEnd` token / `EndDocument`.
#[must_use]
pub fn l_directive_document() -> Parser<'static> {
    wrap_tokens(
        Code::BeginDocument,
        Code::EndDocument,
        seq(many1(l_directive()), l_explicit_document_body()),
    )
}

/// The body of an explicit document (everything after the directives).
///
/// This mirrors `l-explicit-document` but without the outer `BeginDocument`/
/// `EndDocument` wrap, since `l-directive-document` owns that wrapper.
fn l_explicit_document_body() -> Parser<'static> {
    seq(
        c_directives_end(),
        seq(
            opt(l_document_content()),
            opt(seq(c_document_end(), s_l_comments())),
        ),
    )
}

// ---------------------------------------------------------------------------
// §9.2 – Document stream [209]–[211]
// ---------------------------------------------------------------------------

/// [209] l-any-document — directive document, explicit document, or bare document.
#[must_use]
pub fn l_any_document() -> Parser<'static> {
    alt(
        l_directive_document(),
        alt(l_explicit_document(), l_bare_document()),
    )
}

/// [211] l-yaml-stream — a YAML byte stream: zero or more documents.
///
/// A stream consists of an optional leading prefix (BOM + comments), then
/// zero or more documents each optionally followed by separating prefixes.
///
/// Note: `l-document-prefix` can succeed consuming zero bytes (it is
/// entirely optional).  To prevent infinite loops in `many0`, we use a
/// progress-guarded repetition for prefixes: only repeat when at least one
/// byte was consumed.
#[must_use]
pub fn l_yaml_stream() -> Parser<'static> {
    seq(
        many0_progressing(l_document_prefix()),
        many0(seq(
            l_any_document(),
            many0_progressing(l_document_prefix()),
        )),
    )
}

/// Like `many0` but only repeats when the parser consumed at least one byte.
///
/// Prevents infinite loops when the inner parser succeeds without consuming
/// any input (e.g. `l-document-prefix` on empty input).
fn many0_progressing(p: Parser<'static>) -> Parser<'static> {
    Box::new(move |mut state: State<'static>| {
        let mut all_tokens: Vec<crate::token::Token<'static>> = Vec::new();
        loop {
            let before = state.pos.byte_offset;
            match p(state.clone()) {
                Reply::Failure | Reply::Error(_) => break,
                Reply::Success {
                    tokens,
                    state: next,
                } => {
                    if next.pos.byte_offset == before {
                        // No progress — stop to avoid infinite loop.
                        break;
                    }
                    all_tokens.extend(tokens);
                    state = next;
                }
            }
        }
        Reply::Success {
            tokens: all_tokens,
            state,
        }
    })
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Tokenize a YAML string, returning all tokens produced by parsing the
/// complete document stream.
///
/// Returns an empty `Vec` when the input is empty.  Tokens include all
/// structural markers (`BeginDocument`, `EndDocument`, `DirectivesEnd`, etc.)
/// as well as content tokens (`Text`, `Indicator`, etc.).
#[must_use]
pub fn tokenize(input: &str) -> Vec<Token<'_>> {
    // `Parser<'static>` (the type returned by combinators in this module)
    // only accepts `State<'static>` at call sites, because `'static` appears
    // in both the closure's capture set and its argument type and Rust cannot
    // prove the argument `'i` outlives `'static` for an arbitrary `&'i str`.
    //
    // This transmute is safe because:
    //   (a) The extended `'static` reference is not stored anywhere — it is
    //       consumed within the `l_yaml_stream()` call and never escapes.
    //   (b) All token `text` fields point into `input`; transmuting
    //       `Vec<Token<'static>>` back to `Vec<Token<'i>>` is sound because
    //       the tokens' provenance is exactly `input`.
    //   (c) The original `input` remains live for the duration of this call,
    //       so the extended reference is never dangling.
    let extended: &'static str = unsafe { &*std::ptr::from_ref::<str>(input) };
    let state: State<'static> = State::new(extended);
    match l_yaml_stream()(state) {
        Reply::Success { tokens, .. } => unsafe {
            // Shrink token lifetime back to `'i` (= lifetime of `input`).
            core::mem::transmute::<Vec<Token<'static>>, Vec<Token<'_>>>(tokens)
        },
        Reply::Failure | Reply::Error(_) => Vec::new(),
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
    use crate::combinator::{Context, Reply, State};
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

    fn remaining<'a>(reply: &'a Reply<'a>) -> &'a str {
        match reply {
            Reply::Success { state, .. } => state.input,
            Reply::Failure | Reply::Error(_) => panic!("expected success, got failure/error"),
        }
    }

    fn codes(reply: Reply<'_>) -> Vec<Code> {
        match reply {
            Reply::Success { tokens, .. } => tokens.into_iter().map(|t| t.code).collect(),
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    fn has_code(reply: &Reply<'_>, code: Code) -> bool {
        match reply {
            Reply::Success { tokens, .. } => tokens.iter().any(|t| t.code == code),
            Reply::Failure | Reply::Error(_) => false,
        }
    }

    // -----------------------------------------------------------------------
    // Group 1: c_directives_end [202] and c_document_end [203]
    // -----------------------------------------------------------------------

    #[test]
    fn c_directives_end_matches_triple_dash() {
        let reply = c_directives_end()(state("---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "\n");
    }

    #[test]
    fn c_directives_end_emits_directives_end_token() {
        let reply = c_directives_end()(state("---\n"));
        assert!(has_code(&reply, Code::DirectivesEnd));
    }

    #[test]
    fn c_directives_end_fails_on_non_dash() {
        let reply = c_directives_end()(state("abc"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_directives_end_fails_on_partial_dash() {
        let reply = c_directives_end()(state("--a"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_document_end_matches_triple_dot() {
        let reply = c_document_end()(state("...\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "\n");
    }

    #[test]
    fn c_document_end_emits_document_end_token() {
        let reply = c_document_end()(state("...\n"));
        assert!(has_code(&reply, Code::DocumentEnd));
    }

    // -----------------------------------------------------------------------
    // Group 2: c_forbidden [205]
    // -----------------------------------------------------------------------

    #[test]
    fn c_forbidden_triggers_on_triple_dash_followed_by_space() {
        let reply = c_forbidden()(state("--- "));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_triggers_on_triple_dash_followed_by_newline() {
        let reply = c_forbidden()(state("---\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_triggers_on_triple_dash_at_eof() {
        let reply = c_forbidden()(state("---"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_triggers_on_triple_dot_followed_by_space() {
        let reply = c_forbidden()(state("... "));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_triggers_on_triple_dot_followed_by_newline() {
        let reply = c_forbidden()(state("...\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_triggers_on_triple_dot_at_eof() {
        let reply = c_forbidden()(state("..."));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_fails_on_triple_dash_followed_by_letter() {
        let reply = c_forbidden()(state("---a"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_forbidden_fails_when_not_at_column_zero() {
        // Simulate being at column 1 by using state_with.
        let s = State {
            input: "---\n",
            pos: crate::pos::Pos {
                byte_offset: 1,
                char_offset: 1,
                line: 1,
                column: 1,
            },
            n: 0,
            c: Context::BlockOut,
        };
        let reply = c_forbidden()(s);
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 3: l_document_prefix [204]
    // -----------------------------------------------------------------------

    #[test]
    fn l_document_prefix_succeeds_on_empty_input() {
        let reply = l_document_prefix()(state(""));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn l_document_prefix_consumes_bom() {
        let reply = l_document_prefix()(state("\u{FEFF}---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "---\n");
    }

    #[test]
    fn l_document_prefix_consumes_comment_lines() {
        let reply = l_document_prefix()(state("# comment\n---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "---\n");
    }

    #[test]
    fn l_document_prefix_consumes_blank_lines() {
        let reply = l_document_prefix()(state("\n\n---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "---\n");
    }

    #[test]
    fn l_document_prefix_consumes_bom_then_comment() {
        let reply = l_document_prefix()(state("\u{FEFF}# comment\n---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "---\n");
    }

    // -----------------------------------------------------------------------
    // Group 4: l_bare_document [206]
    // -----------------------------------------------------------------------

    #[test]
    fn l_bare_document_parses_scalar() {
        let reply = l_bare_document()(state("hello\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_bare_document_emits_begin_and_end_document() {
        let reply = l_bare_document()(state("hello\n"));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginDocument));
        assert!(cs.contains(&Code::EndDocument));
    }

    #[test]
    fn l_bare_document_stops_before_directives_end() {
        let reply = l_bare_document()(state("hello\n---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "---\n");
    }

    #[test]
    fn l_bare_document_stops_before_document_end() {
        let reply = l_bare_document()(state("hello\n...\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "...\n");
    }

    #[test]
    fn l_bare_document_parses_mapping() {
        let reply = l_bare_document()(state("key: value\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginMapping));
    }

    #[test]
    fn l_bare_document_parses_sequence() {
        let reply = l_bare_document()(state("- item\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginSequence));
    }

    #[test]
    fn l_bare_document_fails_when_starts_with_directives_end() {
        let reply = l_bare_document()(state("---\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn l_bare_document_fails_when_starts_with_document_end() {
        let reply = l_bare_document()(state("...\n"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 5: l_explicit_document [207]
    // -----------------------------------------------------------------------

    #[test]
    fn l_explicit_document_parses_empty_after_marker() {
        // `---\n` with no content — empty explicit document.
        let reply = l_explicit_document()(state("---\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_explicit_document_emits_begin_and_end_document() {
        let reply = l_explicit_document()(state("---\n"));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginDocument));
        assert!(cs.contains(&Code::EndDocument));
    }

    #[test]
    fn l_explicit_document_emits_directives_end_token() {
        let reply = l_explicit_document()(state("---\n"));
        assert!(has_code(&reply, Code::DirectivesEnd));
    }

    #[test]
    fn l_explicit_document_parses_scalar_content() {
        let reply = l_explicit_document()(state("--- hello\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_explicit_document_parses_mapping_content() {
        let reply = l_explicit_document()(state("---\nkey: value\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginMapping));
    }

    #[test]
    fn l_explicit_document_with_end_marker() {
        let reply = l_explicit_document()(state("---\nhello\n...\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::DocumentEnd));
    }

    #[test]
    fn l_explicit_document_fails_without_triple_dash() {
        let reply = l_explicit_document()(state("hello\n"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 6: l_directive_document [208]
    // -----------------------------------------------------------------------

    #[test]
    fn l_directive_document_parses_yaml_directive() {
        let reply = l_directive_document()(state("%YAML 1.2\n---\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_directive_document_emits_begin_and_end_document() {
        let reply = l_directive_document()(state("%YAML 1.2\n---\n"));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginDocument));
        assert!(cs.contains(&Code::EndDocument));
    }

    #[test]
    fn l_directive_document_emits_directives_end_token() {
        let reply = l_directive_document()(state("%YAML 1.2\n---\n"));
        assert!(has_code(&reply, Code::DirectivesEnd));
    }

    #[test]
    fn l_directive_document_parses_tag_directive() {
        let reply = l_directive_document()(state("%TAG ! tag:example.com,2024:\n---\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_directive_document_requires_directives_end_after_directive() {
        // A directive without `---` must fail.
        let reply = l_directive_document()(state("%YAML 1.2\nhello\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn l_directive_document_parses_content_after_directive() {
        let reply = l_directive_document()(state("%YAML 1.2\n---\nhello\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_directive_document_fails_without_any_directive() {
        let reply = l_directive_document()(state("---\nhello\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn l_directive_document_parses_multiple_directives() {
        let reply = l_directive_document()(state("%YAML 1.2\n%TAG ! tag:example.com,2024:\n---\n"));
        assert!(is_success(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 7: l_yaml_stream [211]
    // -----------------------------------------------------------------------

    #[test]
    fn l_yaml_stream_accepts_empty_input() {
        let reply = l_yaml_stream()(state(""));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn l_yaml_stream_accepts_single_bare_document() {
        let reply = l_yaml_stream()(state("hello\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginDocument));
    }

    #[test]
    fn l_yaml_stream_accepts_comment_only() {
        let reply = l_yaml_stream()(state("# just a comment\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_yaml_stream_accepts_explicit_document() {
        let reply = l_yaml_stream()(state("---\nhello\n...\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginDocument));
    }

    #[test]
    fn l_yaml_stream_accepts_two_bare_documents() {
        let reply = l_yaml_stream()(state("first\n---\nsecond\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 2);
    }

    #[test]
    fn l_yaml_stream_accepts_two_explicit_documents() {
        let reply = l_yaml_stream()(state("---\nfirst\n...\n---\nsecond\n...\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 2);
    }

    #[test]
    fn l_yaml_stream_accepts_bare_then_explicit_document() {
        let reply = l_yaml_stream()(state("first\n---\nsecond\n...\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 2);
    }

    #[test]
    fn l_yaml_stream_accepts_directive_document() {
        let reply = l_yaml_stream()(state("%YAML 1.2\n---\nhello\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginDocument));
    }

    #[test]
    fn l_yaml_stream_accepts_leading_comments_then_document() {
        let reply = l_yaml_stream()(state("# comment\n---\nhello\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginDocument));
    }

    #[test]
    fn l_yaml_stream_accepts_bom_prefix() {
        let reply = l_yaml_stream()(state("\u{FEFF}---\nhello\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginDocument));
    }

    #[test]
    fn l_yaml_stream_accepts_mapping_document() {
        let reply = l_yaml_stream()(state("key: value\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginMapping));
    }

    #[test]
    fn l_yaml_stream_accepts_sequence_document() {
        let reply = l_yaml_stream()(state("- item\n- item2\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginSequence));
    }

    #[test]
    fn l_yaml_stream_explicit_document_end_tokens_count() {
        let reply = l_yaml_stream()(state("---\nfirst\n...\n---\nsecond\n...\n"));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::EndDocument).count(), 2);
    }

    #[test]
    fn l_yaml_stream_document_end_markers_count() {
        let reply = l_yaml_stream()(state("---\nfirst\n...\n---\nsecond\n...\n"));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::DocumentEnd).count(), 2);
    }

    #[test]
    fn l_yaml_stream_consumes_all_input_for_multi_document() {
        let reply = l_yaml_stream()(state("---\nfirst\n...\n---\nsecond\n...\n"));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn l_yaml_stream_empty_explicit_documents() {
        let reply = l_yaml_stream()(state("---\n---\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 2);
    }

    #[test]
    fn l_yaml_stream_three_documents() {
        let reply = l_yaml_stream()(state("a\n---\nb\n---\nc\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 3);
    }

    #[test]
    fn l_yaml_stream_blank_lines_between_documents() {
        let reply = l_yaml_stream()(state("---\nfirst\n...\n\n---\nsecond\n...\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 2);
    }

    // -----------------------------------------------------------------------
    // Group 8: tokenize public entry point
    // -----------------------------------------------------------------------

    #[test]
    fn tokenize_empty_input_returns_empty_vec() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn tokenize_simple_mapping_returns_tokens() {
        let tokens = tokenize("key: value\n");
        assert!(!tokens.is_empty());
    }

    #[test]
    fn tokenize_returns_begin_document_token() {
        let tokens = tokenize("key: value\n");
        assert!(tokens.iter().any(|t| t.code == Code::BeginDocument));
    }

    #[test]
    fn tokenize_returns_end_document_token() {
        let tokens = tokenize("key: value\n");
        assert!(tokens.iter().any(|t| t.code == Code::EndDocument));
    }

    #[test]
    fn tokenize_explicit_stream_has_directives_end_token() {
        let tokens = tokenize("---\nhello\n");
        assert!(tokens.iter().any(|t| t.code == Code::DirectivesEnd));
    }

    #[test]
    fn tokenize_multi_doc_stream_has_two_begin_document_tokens() {
        let tokens = tokenize("---\nfirst\n...\n---\nsecond\n...\n");
        assert_eq!(
            tokens
                .iter()
                .filter(|t| t.code == Code::BeginDocument)
                .count(),
            2
        );
    }

    #[test]
    fn tokenize_all_tokens_have_non_empty_or_marker_codes() {
        // Verify no token has Code::Error.
        let tokens = tokenize("key: value\n");
        assert!(!tokens.iter().any(|t| t.code == Code::Error));
    }
}
