use super::*;
use rstest::rstest;

// -----------------------------------------------------------------------
// Group A — Basic explicit-start documents
// -----------------------------------------------------------------------

#[test]
fn bare_dash_no_newline_yields_doc_start_and_implicit_end() {
    let events = event_variants("---");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn bare_dash_with_lf_yields_same_sequence_as_no_newline() {
    let events = event_variants("---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn explicit_end_marker_yields_explicit_doc_end() {
    let events = event_variants("---\n...");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn explicit_end_with_lf_yields_explicit_doc_end() {
    let events = event_variants("---\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group B — Multiple documents
// -----------------------------------------------------------------------

#[test]
fn two_docs_adjacent_markers_both_have_implicit_end() {
    let events = event_variants("---\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn two_docs_explicit_ends() {
    let events = event_variants("---\n...\n---\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: true },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn two_docs_blank_lines_between_markers() {
    let events = event_variants("---\n\n\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group C — Blank/comment-only stream (regressions from Task 4)
// -----------------------------------------------------------------------

#[test]
fn empty_input_produces_stream_only() {
    let events = event_variants("");
    assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
}

#[test]
fn whitespace_only_produces_stream_only() {
    let events = event_variants("   \n");
    assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
}

#[test]
fn comment_only_produces_stream_only() {
    let events = event_variants("# comment\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment { text: " comment" },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group D — Comments and blank lines around markers
// -----------------------------------------------------------------------

#[test]
fn comment_before_marker_is_emitted() {
    let events = event_variants("# comment\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment { text: " comment" },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn blank_lines_before_marker_are_skipped() {
    let events = event_variants("\n\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn comment_between_start_and_end_marker() {
    let events = event_variants("---\n# comment\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Comment { text: " comment" },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn blank_lines_between_start_and_end_marker() {
    let events = event_variants("---\n\n\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group E — Orphan `...` (document-end before any document start)
// -----------------------------------------------------------------------

#[test]
fn orphan_document_end_before_any_start_is_skipped() {
    let events = event_variants("...\n");
    assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
}

#[test]
fn orphan_document_end_then_real_document() {
    let events = event_variants("...\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group F — Line ending variants
// -----------------------------------------------------------------------

#[test]
fn crlf_terminated_marker_is_recognised() {
    let events = event_variants("---\r\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn bare_cr_terminated_marker_is_recognised() {
    let events = event_variants("---\r");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group G — BOM handling
// -----------------------------------------------------------------------

#[test]
fn bom_before_directives_end_marker_is_stripped_correctly() {
    let events = event_variants("\u{FEFF}---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group H — Content after marker on same line
// -----------------------------------------------------------------------

#[test]
fn content_after_dash_marker_space_separated_starts_document() {
    // Space after `---` qualifies as a marker (4th byte is space).
    // The content "value" is the root plain scalar.
    let events = event_variants("--- value\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "value".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                meta: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group I — Indented `---` is NOT a marker
// -----------------------------------------------------------------------

#[test]
fn indented_dash_is_not_a_directives_end_marker() {
    // "  ---" has indent=2; it is a plain scalar (not a marker).
    // `---` is allowed as a plain scalar when it is indented — ns-plain-first
    // allows `-` when followed by a safe ns-char, and the next two `-` chars
    // are ns-chars.
    let events = event_variants("  ---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "---".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                meta: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn indented_dots_is_not_a_document_end_marker() {
    // "  ..." has indent=2; it is a plain scalar (not a doc-end marker).
    // Verifies that `peeked_indent == 0` guard in Change B does not suppress
    // the line — it reaches `is_document_end()` (which also returns false for
    // indented content) and then falls through to scalar parsing.
    let events = event_variants("---\n  ...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "...".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                meta: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group J — Span assertions
// -----------------------------------------------------------------------

#[test]
fn doc_start_explicit_span_covers_three_bytes() {
    let results = parse_to_vec("---\n");
    let Some(Ok((Event::DocumentStart { .. }, span))) = results.get(1) else {
        unreachable!("expected DocumentStart as second event");
    };
    assert_eq!(
        span.end - span.start,
        3,
        "DocumentStart span must cover exactly 3 bytes"
    );
}

#[test]
fn doc_start_explicit_span_start_byte_offset_is_zero() {
    let results = parse_to_vec("---\n");
    let Some(Ok((Event::DocumentStart { .. }, span))) = results.get(1) else {
        unreachable!("expected DocumentStart as second event");
    };
    assert_eq!(span.start, 0);
}

#[test]
fn doc_end_explicit_span_covers_three_bytes() {
    let results = parse_to_vec("---\n...\n");
    let Some(Ok((Event::DocumentEnd { explicit: true }, span))) = results.get(2) else {
        unreachable!("expected explicit DocumentEnd as third event");
    };
    assert_eq!(
        span.end - span.start,
        3,
        "DocumentEnd span must cover exactly 3 bytes"
    );
}

#[test]
fn doc_end_explicit_span_start_byte_offset_is_four() {
    // "---\n" = 4 bytes, so `...` starts at byte offset 4.
    let results = parse_to_vec("---\n...\n");
    let Some(Ok((Event::DocumentEnd { explicit: true }, span))) = results.get(2) else {
        unreachable!("expected explicit DocumentEnd as third event");
    };
    assert_eq!(span.start, 4);
}

#[test]
fn doc_end_implicit_span_is_zero_width() {
    let results = parse_to_vec("---\n");
    let Some(Ok((Event::DocumentEnd { explicit: false }, span))) = results.get(2) else {
        unreachable!("expected implicit DocumentEnd as third event");
    };
    assert_eq!(
        span.start, span.end,
        "implicit DocumentEnd span must be zero-width"
    );
}

#[test]
fn doc_start_explicit_span_start_after_blank_lines() {
    // "\n\n---\n": two newlines (2 bytes) then `---` at byte offset 2.
    let results = parse_to_vec("\n\n---\n");
    let Some(Ok((Event::DocumentStart { .. }, span))) = results.get(1) else {
        unreachable!("expected DocumentStart as second event");
    };
    assert_eq!(span.start, 2);
}

// -----------------------------------------------------------------------
// Group K — Bare document boundaries (IT-28 through IT-37)
// -----------------------------------------------------------------------

#[test]
fn single_content_line_yields_bare_doc() {
    let events = event_variants("foo\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "foo".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                meta: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn multi_line_content_yields_single_bare_doc() {
    // Both lines fold into a single plain scalar ("foo bar").
    let events = event_variants("foo\nbar\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "foo bar".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                meta: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn leading_blank_before_bare_content() {
    // Blank line skipped in BetweenDocs; `foo` triggers the bare-doc path.
    let events = event_variants("\nfoo\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "foo".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                meta: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn bare_doc_with_explicit_end_marker() {
    // InDocument sees scalar, then `...` → DocumentEnd{explicit:true}.
    let events = event_variants("foo\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "foo".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                meta: None,
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn bare_doc_followed_by_explicit_doc() {
    // InDocument emits scalar, sees `---` → implicit DocumentEnd, then
    // DocumentStart{explicit:true} for the new one.
    let events = event_variants("foo\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "foo".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                meta: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn two_bare_docs_separated_by_explicit_end() {
    let events = event_variants("foo\n...\nbar\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "foo".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                meta: None,
            },
            Event::DocumentEnd { explicit: true },
            Event::DocumentStart {
                explicit: false,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "bar".into(),
                style: rlsp_yaml_parser::ScalarStyle::Plain,
                meta: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn bare_doc_start_span_is_zero_width_at_first_content_byte() {
    // "foo\n": DocumentStart{false} span at byte 0, zero-width.
    let results = parse_to_vec("foo\n");
    let Some(Ok((
        Event::DocumentStart {
            explicit: false, ..
        },
        span,
    ))) = results.get(1)
    else {
        unreachable!("expected bare DocumentStart as second event");
    };
    assert_eq!(
        span.start, span.end,
        "bare DocumentStart span must be zero-width"
    );
    assert_eq!(span.start, 0);
}

#[test]
fn bare_doc_end_at_eof_span_is_zero_width_after_last_content() {
    // "foo\n" = 4 bytes; sequence: StreamStart, DocStart, Scalar, DocEnd.
    // DocEnd is at index 3 now (Scalar is at index 2).
    let results = parse_to_vec("foo\n");
    let Some(Ok((Event::DocumentEnd { explicit: false }, span))) = results.get(3) else {
        unreachable!("expected bare DocumentEnd as fourth event");
    };
    assert_eq!(
        span.start, span.end,
        "bare DocumentEnd span must be zero-width"
    );
    assert_eq!(span.start, 4);
}

#[test]
fn bare_doc_end_before_explicit_doc_span_is_zero_width_at_marker_pos() {
    // "foo\n---\n": StreamStart, DocStart, Scalar, DocEnd(implicit), DocStart, DocEnd.
    // Implicit DocEnd is at index 3.
    let results = parse_to_vec("foo\n---\n");
    let Some(Ok((Event::DocumentEnd { explicit: false }, span))) = results.get(3) else {
        unreachable!("expected implicit DocumentEnd at index 3");
    };
    assert_eq!(
        span.start, span.end,
        "implicit DocumentEnd span must be zero-width"
    );
    assert_eq!(span.start, 4);
}

#[test]
fn bare_doc_start_span_zero_width_after_leading_blank() {
    // "\nfoo\n": `f` is at byte offset 1 (after the leading `\n`).
    let results = parse_to_vec("\nfoo\n");
    let Some(Ok((
        Event::DocumentStart {
            explicit: false, ..
        },
        span,
    ))) = results.get(1)
    else {
        unreachable!("expected bare DocumentStart as second event");
    };
    assert_eq!(span.start, 1);
}

// -----------------------------------------------------------------------
// Group L — Directive line skipping (IT-38 through IT-40)
// -----------------------------------------------------------------------

#[test]
fn yaml_directive_before_explicit_doc_carries_version() {
    let events = event_variants("%YAML 1.2\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: Some((1, 2)),
                tag_directives: vec![],
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn yaml_directive_with_explicit_end_carries_version() {
    let events = event_variants("%YAML 1.2\n---\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: Some((1, 2)),
                tag_directives: vec![],
            },
            Event::DocumentEnd { explicit: true },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn unknown_directive_before_explicit_doc_is_skipped() {
    // Any `%`-prefixed line is silently skipped (not limited to %YAML).
    let events = event_variants("%FOO bar\n---\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// -----------------------------------------------------------------------
// Group M — directive-split regression test (IT-41)
// -----------------------------------------------------------------------
// A directive is valid only before `---`.  A `%`-prefixed line inside an open
// document (InDocument context) is neither a directive nor content, so it is
// an error rather than a line the parser drops without a trace.

#[test]
fn percent_prefixed_line_inside_explicit_doc_is_an_error() {
    // IT-41: `---\n%foo: bar\n` once produced an empty document with no sign of
    // the `%foo: bar` line.
    let events = parse_to_vec("---\n%foo: bar\n");
    let error = events
        .iter()
        .find_map(|r| r.as_ref().err())
        .expect("an error");
    assert_eq!((error.pos.line, error.pos.column), (2, 0));
    assert!(
        !events
            .iter()
            .any(|r| matches!(r, Ok((Event::DocumentEnd { .. }, _)))),
        "the document must not end before the error"
    );
}

// -----------------------------------------------------------------------
// Group N: Coverage gap test — root-node guard
// -----------------------------------------------------------------------
// A document contains exactly one root node; once it is fully emitted, any
// further non-comment, non-blank content without an intervening document
// marker (`---`/`...`) is invalid.

#[test]
fn content_after_root_node_without_marker_errors() {
    // "[a]\nb\n" — a flow sequence closes as the (only) root node at the
    // end of line 1; `b` on line 2 is unexpected content with no
    // intervening `---`/`...` marker. (A bare plain scalar like "a\nb\n"
    // is NOT an error — it folds into a single multi-line scalar "a b",
    // so it does not exercise this guard.)
    let result: Vec<_> = parse_events("[a]\nb\n").collect();
    let has_error = result.iter().any(Result::is_err);
    assert!(
        has_error,
        "content after the document root node without a marker must return an error"
    );
    if let Some(err) = result.iter().find_map(|r| r.as_ref().err()) {
        assert!(
            err.message
                .contains("unexpected content after document root node"),
            "error message should mention unexpected content after root node; got: {}",
            err.message
        );
    }
}

// -----------------------------------------------------------------------
// Group O: A flow collection or an alias on the `---` line
// -----------------------------------------------------------------------
// YAML 1.2.2 [207] l-bare-document ::= s-l+block-node(-1,block-in), whose
// s-l+flow-in-block [197] takes `s-separate ns-flow-node s-l-comments` — a
// flow collection ([137]/[140]) or an alias ([104] c-ns-alias-node) after the
// `---` marker is the root node; a block mapping cannot start on the marker
// line, so a node followed by more content there is an error.  libfyaml 0.9.6
// gives the same events.

const fn explicit_start() -> Event<'static> {
    Event::DocumentStart {
        explicit: true,
        version: None,
        tag_directives: vec![],
    }
}

fn flow_plain(v: &'static str) -> Event<'static> {
    Event::Scalar {
        value: v.into(),
        style: ScalarStyle::Plain,
        meta: None,
    }
}

const fn flow_start(is_sequence: bool) -> Event<'static> {
    if is_sequence {
        Event::SequenceStart {
            style: CollectionStyle::Flow,
            meta: None,
        }
    } else {
        Event::MappingStart {
            style: CollectionStyle::Flow,
            meta: None,
        }
    }
}

#[test]
fn flow_sequence_on_the_marker_line_is_the_root_sequence() {
    assert_eq!(
        event_variants("--- [alpha, bravo]\n"),
        [
            Event::StreamStart,
            explicit_start(),
            flow_start(true),
            flow_plain("alpha"),
            flow_plain("bravo"),
            Event::SequenceEnd,
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn flow_mapping_on_the_marker_line_is_the_root_mapping() {
    assert_eq!(
        event_variants("--- {a: 1}\n"),
        [
            Event::StreamStart,
            explicit_start(),
            flow_start(false),
            flow_plain("a"),
            flow_plain("1"),
            Event::MappingEnd,
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[test]
fn alias_on_the_marker_line_is_an_alias_node() {
    assert_eq!(
        event_variants("--- *a\n"),
        [
            Event::StreamStart,
            explicit_start(),
            Event::Alias { name: "a" },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[rstest]
#[case::space_run("---   [x]\n")]
#[case::tab("---\t[x]\n")]
#[case::comment_after("--- [x] # c\n")]
#[case::collection_spans_lines("--- [x,\n  y]\n")]
#[case::closing_bracket_on_its_own_line("--- [\n  x\n]\n")]
#[case::comment_inside("--- [ # c\n  x]\n")]
#[case::nested("--- [[x], {y: z}]\n")]
#[case::alias_inside("--- [*a]\n")]
#[case::anchor("--- &a [x]\n")]
#[case::tag("--- !t [x]\n")]
#[case::followed_by_document_end("--- [x]\n...\n")]
#[case::followed_by_next_document("--- [x]\n--- [y]\n")]
#[case::alias_with_comment("--- *a # c\n")]
fn collection_or_alias_on_the_marker_line_parses(#[case] input: &str) {
    assert!(!has_error(input), "input: {input:?}");
}

#[rstest]
#[case::second_flow_collection("--- [x] [y]\n")]
#[case::flow_collection_then_plain("--- [x] y\n")]
#[case::flow_sequence_as_a_key("--- [x]: 1\n")]
#[case::pair_sequence_as_a_key("--- [a: b]: c\n")]
#[case::multi_line_sequence_as_a_key("--- [a,\n b]: v\n")]
#[case::flow_mapping_as_a_key("--- {a: 1}: 2\n")]
#[case::alias_then_plain("--- *a b\n")]
#[case::alias_as_a_key("--- *a : v\n")]
#[case::alias_named_with_a_colon_then_plain("--- *a: v\n")]
#[case::properties_then_alias("--- &x *a\n")]
#[case::tag_then_alias("--- !t *a\n")]
#[case::closing_bracket("--- ]\n")]
#[case::closing_brace("--- }\n")]
#[case::closing_bracket_then_plain("--- ]x\n")]
#[case::closing_brace_then_plain("--- }x\n")]
#[case::two_closing_brackets("--- ]]\n")]
#[case::comment_without_space("--- [x]#c\n")]
#[case::extra_closing_bracket("--- [a]]\n")]
#[case::unclosed("--- [a\n")]
#[case::unclosed_after_comma("--- [a,\n")]
#[case::unclosed_mapping("--- {\n")]
#[case::alias_without_name("--- *\n")]
#[case::alias_on_the_next_line_then_plain("--- *a\nz\n")]
#[case::collection_then_plain_on_the_next_line("--- [x]\nz\n")]
#[case::collection_then_sequence_entry("--- [x]\n- y\n")]
#[case::collection_then_mapping_entry("--- [x]\nk: v\n")]
fn content_after_the_node_on_the_marker_line_is_an_error(#[case] input: &str) {
    assert!(has_error(input), "input: {input:?}");
}

// -----------------------------------------------------------------------
// Group P: The marker line and the line below give the same events
// -----------------------------------------------------------------------
// Every node that starts on the `---` line is read as it is read when it starts
// on the next line; where the next line is an error, so is the marker line.

/// The events of `input` as text, or `Err` when the stream has an error.  What
/// comes before an error is left out: the marker line is rejected before it
/// emits the start of the document, and its message differs from the one the
/// line below gets.
fn event_trace(input: &str) -> Result<Vec<String>, ()> {
    parse_events(input)
        .map(|item| item.map(|(event, _)| format!("{event:?}")).map_err(|_| ()))
        .collect()
}

#[rstest]
#[case::flow_sequence("[alpha, bravo]")]
#[case::empty_sequence("[]")]
#[case::padded("[ a ]")]
#[case::nested("[a, [b, c]]")]
#[case::over_two_lines("[a,\n b]")]
#[case::bracket_on_its_own_line("[\n a,\n b]")]
#[case::quoted_items("[\"q\" , 'r']")]
#[case::trailing_comma("[a, ]")]
#[case::plain_item_with_a_space("[a b]")]
#[case::alias_item("[*a]")]
#[case::anchored_and_aliased_items("[&a x, *a]")]
#[case::tagged_item("[!t x]")]
#[case::single_pair("[a: 1]")]
#[case::explicit_single_pair("[? a: 1]")]
#[case::comment_inside("[ # c\n 1]")]
#[case::comment_after("[a] # c")]
#[case::tab_comment_after("[a]\t# c")]
#[case::trailing_spaces("[a]  ")]
#[case::comment_on_the_next_line("[a]\n# c")]
#[case::indented_comment_on_the_next_line("[a]\n  # c")]
#[case::document_end("[a, b]\n...")]
#[case::next_document("[a]\n---\n[b]")]
#[case::alias("*a")]
#[case::alias_with_comment("*a # c")]
#[case::alias_comment_on_the_next_line("*a\n# c")]
#[case::alias_named_with_a_colon("*a:")]
#[case::alias_named_with_a_colon_inside("*a:b")]
#[case::flow_mapping("{a: 1}")]
#[case::empty_mapping("{}")]
#[case::mapping_with_comment_after("{a: 1} # c")]
#[case::mapping_with_comment_inside("{ # c\n a: 1}")]
#[case::bracket_in_a_quoted_item("[\"]\", a]")]
#[case::apostrophe_in_a_quoted_item("[a, 'it''s']")]
#[case::hash_after_a_comment_start("[a] # c: d")]
#[case::colon_in_a_quoted_value("{a: \"x: y\"}")]
#[case::comment_with_brackets("[a] # [b] [c]")]
#[case::alias_comment_with_brackets("*a # [x] [y]")]
#[case::multibyte_items("[ä, \"日本\"]")]
#[case::multibyte_alias("*é")]
#[case::pair_in_a_mapping("{a: 1, b: [c: d]}")]
#[case::pairs_in_a_sequence("[a: 1, b: 2]")]
#[case::comment_with_a_key("[ # k: v\n a]")]
#[case::second_collection("[x] [y]")]
#[case::collection_then_plain("[x] y")]
#[case::collection_then_plain_without_space("[x]y")]
#[case::closing_bracket_twice("[x]]")]
#[case::comment_without_space("[x]#c")]
#[case::collection_then_tab_then_collection("[x]\t[y]")]
#[case::multi_line_collection_as_a_key("[a,\n b]: v")]
#[case::multi_line_collection_then_collection("[a,\n b] [c]")]
#[case::multi_line_collection_then_plain("[a,\n b] x")]
#[case::alias_then_alias("*a *b")]
#[case::alias_then_plain("*a b")]
#[case::properties_then_alias("&x *a")]
#[case::tag_then_alias("!t *a")]
#[case::document_marker_inside("[a,\n---\n b]")]
#[case::document_end_inside("[a,\n...\n]")]
#[case::unclosed("[a")]
#[case::unclosed_mapping("{")]
#[case::lone_closing_bracket("]")]
#[case::lone_closing_brace("}")]
#[case::closing_bracket_then_plain("]x")]
#[case::closing_brace_then_plain("}x")]
#[case::two_closing_brackets("]]")]
#[case::alias_named_with_a_colon_then_plain("*a: v")]
fn marker_line_node_reads_as_the_node_on_the_next_line(#[case] node: &str) {
    let same_line = event_trace(&format!("--- {node}\n"));
    let next_line = event_trace(&format!("---\n{node}\n"));
    assert_eq!(same_line, next_line, "node: {node:?}");
}

#[rstest]
#[case::lf("--- [alpha, bravo]\n")]
#[case::crlf("--- [alpha, bravo]\r\n")]
#[case::lone_cr("--- [alpha, bravo]\r")]
#[case::bom("\u{feff}--- [alpha, bravo]\n")]
#[case::tab("---\t[alpha, bravo]\n")]
#[case::spaces("---   [alpha, bravo]\n")]
#[case::multibyte("--- [äb, \"日本\", bravo]\n")]
fn marker_line_flow_sequence_has_the_spans_of_its_text(#[case] input: &str) {
    let items: Vec<_> = parse_events(input)
        .map(|item| item.expect("the input parses"))
        .collect();
    let start = input.find('[').expect("a bracket");
    for (event, span) in &items {
        let text = &input[span.start as usize..span.end as usize];
        match event {
            Event::Scalar { value, style, .. } if *style == ScalarStyle::Plain => {
                assert_eq!(text, value, "scalar span in {input:?}");
            }
            Event::Alias { name } => assert_eq!(text, format!("*{name}")),
            Event::SequenceStart { .. } => assert_eq!(span.start as usize, start),
            Event::Scalar { .. }
            | Event::StreamStart
            | Event::StreamEnd
            | Event::Comment { .. }
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::SequenceEnd
            | Event::MappingStart { .. }
            | Event::MappingEnd => {}
        }
    }
    let (line, column) = line_col(input, u32::try_from(start).unwrap());
    assert_eq!(line, 1, "{input:?}");
    assert_eq!(
        column,
        u32::try_from(
            input[..start]
                .trim_start_matches('\u{feff}')
                .chars()
                .count()
        )
        .unwrap()
            + u32::from(input.starts_with('\u{feff}')),
        "{input:?}"
    );
}

#[test]
fn marker_line_alias_has_the_span_of_its_text() {
    let items: Vec<_> = parse_events("--- *a\n").map(Result::unwrap).collect();
    let (_, span) = items
        .iter()
        .find(|(event, _)| matches!(event, Event::Alias { .. }))
        .expect("an alias");
    assert_eq!((span.start, span.end), (4, 6));
}

#[test]
fn marker_line_multi_line_collection_takes_continuation_lines_at_any_indent() {
    for input in ["--- [a,\nb]\n", "--- [a,\n b]\n", "--- [a,\n    b]\n"] {
        assert!(!has_error(input), "{input:?}");
    }
}

#[test]
fn marker_line_flow_collection_nesting_is_limited_without_overflowing_the_stack() {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let deepest = MAX_COLLECTION_DEPTH - 1;
            let ok = format!("--- {}{}\n", "[".repeat(deepest), "]".repeat(deepest));
            assert!(!has_error(&ok), "the deepest nesting the parser accepts");
            for levels in [MAX_COLLECTION_DEPTH + 1, 100_000] {
                let deep = format!("--- {}{}\n", "[".repeat(levels), "]".repeat(levels));
                assert!(has_error(&deep), "{levels} levels");
                let unclosed = format!("--- {}\n", "[".repeat(levels));
                assert!(has_error(&unclosed), "{levels} levels, unclosed");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn marker_line_alias_name_is_limited() {
    let longest = format!("--- *{}\n", "a".repeat(MAX_ANCHOR_NAME_BYTES - 1));
    assert!(!has_error(&longest));
    let too_long = format!("--- *{}\n", "a".repeat(MAX_ANCHOR_NAME_BYTES + 1));
    assert!(has_error(&too_long));
}

#[test]
fn marker_line_flow_sequence_with_many_items_reads_as_on_the_next_line() {
    let items = "a, ".repeat(5_000);
    let on_marker_line = event_trace(&format!("--- [{items}b]\n"));
    let on_next_line = event_trace(&format!("---\n[{items}b]\n"));
    assert_eq!(on_marker_line, on_next_line);
}

// -----------------------------------------------------------------------
// Properties alone are the root node of the document they end
// -----------------------------------------------------------------------

/// A property line with nothing after it, before the `---` or `...` that ends
/// the document, is an empty root node with those properties (YAML 1.2.2 [96]
/// on an empty node, [105]); it is neither dropped nor given to the next
/// document.  The end of the input already behaved so.
#[rstest]
#[case::anchor_before_a_start_marker("&x\n---\ny\n", (Some("x"), None))]
#[case::tag_before_a_start_marker("!t\n---\ny\n", (None, Some("!t")))]
#[case::both_before_a_start_marker("&x !t\n---\ny\n", (Some("x"), Some("!t")))]
#[case::anchor_with_a_comment("&x # c\n---\ny\n", (Some("x"), None))]
#[case::anchor_on_a_marker_line("--- &x\n---\ny\n", (Some("x"), None))]
#[case::anchor_before_an_end_marker("&x\n...\n", (Some("x"), None))]
#[case::tag_on_a_marker_line_before_an_end_marker("--- !t\n...\n---\ny\n", (None, Some("!t")))]
#[case::anchor_at_the_end_of_the_input("&x\n", (Some("x"), None))]
fn properties_alone_before_a_document_boundary_are_an_empty_root(
    #[case] input: &str,
    #[case] properties: (Option<&str>, Option<&str>),
) {
    let events = event_variants(input);
    let first_document: Vec<&Event<'_>> = events
        .iter()
        .skip_while(|e| !matches!(e, Event::DocumentStart { .. }))
        .take_while(|e| !matches!(e, Event::DocumentEnd { .. }))
        .collect();
    let scalars: Vec<&&Event<'_>> = first_document
        .iter()
        .filter(|e| matches!(e, Event::Scalar { .. }))
        .collect();
    assert_eq!(scalars.len(), 1, "{input:?}");
    if let Some(Event::Scalar { value, .. }) = scalars.first().map(|e| **e) {
        assert!(value.is_empty(), "{input:?}");
    }
    if let Some(scalar) = scalars.first() {
        assert_eq!((scalar.anchor(), scalar.tag()), properties, "{input:?}");
    }
    // The next document, if any, has none of them.
    let next_root = events
        .iter()
        .skip_while(|e| !matches!(e, Event::DocumentEnd { .. }))
        .find(|e| matches!(e, Event::Scalar { .. }));
    if let Some(next_root) = next_root {
        assert_eq!((next_root.anchor(), next_root.tag()), (None, None));
    }
}

// -----------------------------------------------------------------------
// Group Q: A plain scalar on the `---` line
// -----------------------------------------------------------------------
// YAML 1.2.2 [207] l-bare-document ::= s-l+block-node(-1,block-in), whose
// s-l+flow-in-block [197] takes `s-separate ns-flow-node s-l-comments`.  A
// plain scalar after the `---` marker is the root node: it continues over the
// lines below ([134]/[135] ns-plain-multi-line, folded by [73]/[74]), which at
// the root may start at any column, and a block mapping cannot start on the
// marker line.  libfyaml 0.9.6 gives the same events and the same errors.

/// The scalars of `input` as `(value, start, end)`, panicking on an error.
fn scalars_with_spans(input: &str) -> Vec<(String, u32, u32)> {
    parse_events(input)
        .map(|item| match item {
            Ok(item) => item,
            Err(e) => unreachable!("unexpected parse error: {e}"),
        })
        .filter_map(|(event, span)| match event {
            Event::Scalar { value, .. } => Some((value.into_owned(), span.start, span.end)),
            Event::StreamStart
            | Event::StreamEnd
            | Event::Comment { .. }
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::SequenceStart { .. }
            | Event::SequenceEnd
            | Event::MappingStart { .. }
            | Event::MappingEnd
            | Event::Alias { .. } => None,
        })
        .collect()
}

#[rstest]
#[case::continuation_line("--- x\n  y\n", "x y", 4, 9)]
#[case::continuation_at_column_zero("--- x\ny\n", "x y", 4, 7)]
#[case::three_lines("--- x\ny\nz\n", "x y z", 4, 9)]
#[case::percent_continuation("--- x\n%y\n", "x %y", 4, 8)]
#[case::percent_word_continuation("--- x\n%FOO\n", "x %FOO", 4, 10)]
#[case::at_sign_continuation("--- x\n  @y\n", "x @y", 4, 10)]
#[case::backtick_continuation("--- x\n  `y\n", "x `y", 4, 10)]
#[case::comma_continuation("--- x\n  ,y\n", "x ,y", 4, 10)]
#[case::bracket_continuation("--- x\n  [y\n", "x [y", 4, 10)]
#[case::blank_line_between("--- x\n\n  y\n", "x\ny", 4, 10)]
#[case::blank_lines_between("--- x\n  \n\n  y\n", "x\n\ny", 4, 13)]
#[case::trailing_blank_lines("--- x\n  y\n\n\n", "x y", 4, 9)]
#[case::crlf("--- x\r\n  y\r\n", "x y", 4, 10)]
#[case::tab_continuation("--- x\n\ty\n", "x y", 4, 8)]
#[case::space_and_tab_continuation("--- x\n  \ty\n", "x y", 4, 10)]
#[case::words("--- a b c\n  d e\n", "a b c d e", 4, 15)]
#[case::dash_continuation("--- x\n- y\n", "x - y", 4, 9)]
#[case::question_mark_continuation("--- x\n? y\n", "x ? y", 4, 9)]
#[case::bare_dash_continuation("--- x\n-\n", "x -", 4, 7)]
#[case::bare_question_mark_continuation("--- x\n?\n", "x ?", 4, 7)]
#[case::colon_word_continuation("--- x\n  :y\n", "x :y", 4, 10)]
#[case::dash_word_continuation("--- x\n  -y\n", "x -y", 4, 10)]
#[case::flow_looking_continuation("--- x\n[y]\n", "x [y]", 4, 9)]
#[case::quote_looking_continuation("--- x\n\"y\"\n", "x \"y\"", 4, 9)]
#[case::anchor_looking_continuation("--- x\n&a y\n", "x &a y", 4, 10)]
#[case::tag_looking_continuation("--- x\n!t y\n", "x !t y", 4, 10)]
#[case::alias_looking_continuation("--- x\n*a\n", "x *a", 4, 8)]
#[case::block_scalar_looking_continuation("--- x\n|\n", "x |", 4, 7)]
#[case::marker_prefix_continuation("--- x\n---y\n", "x ---y", 4, 10)]
#[case::indented_marker_continuation("--- x\n  ---\n", "x ---", 4, 11)]
#[case::indented_end_marker_continuation("--- x\n  ...\n  y\n", "x ... y", 4, 15)]
#[case::deeper_continuation("--- x\n  y\n   z\n", "x y z", 4, 14)]
#[case::two_byte("--- é\n  ü\n", "é ü", 4, 11)]
#[case::three_byte("--- 日本\n  語\n", "日本 語", 4, 16)]
#[case::single_line_without_newline("--- x", "x", 4, 5)]
#[case::continuation_without_newline("--- x\n  y", "x y", 4, 9)]
#[case::wide_gap("---  x\n  y\n", "x y", 5, 10)]
#[case::tab_gap("---\tx\n  y\n", "x y", 4, 9)]
#[case::dots_in_the_value("--- x ...\n", "x ...", 4, 9)]
#[case::tab_in_the_value("--- x\ty\n", "x\ty", 4, 7)]
fn plain_scalar_on_the_marker_line_continues_over_the_lines_below(
    #[case] input: &str,
    #[case] value: &str,
    #[case] start: u32,
    #[case] end: u32,
) {
    assert_eq!(
        scalars_with_spans(input),
        [(value.to_owned(), start, end)],
        "input: {input:?}"
    );
}

#[rstest]
#[case::colon_start(":x")]
#[case::dash_start("-x")]
#[case::question_mark_start("?x")]
#[case::colon_inside("x:y")]
#[case::colon_after_a_space("x :y")]
#[case::url("http://x")]
#[case::dots("...")]
#[case::dashes("---")]
#[case::dots_then_word("... x")]
#[case::dashes_then_word("--- x")]
fn plain_scalar_on_the_marker_line_may_look_like_an_indicator(#[case] text: &str) {
    assert_eq!(
        scalars_with_spans(&format!("--- {text}\n")),
        [(text.to_owned(), 4, 4 + u32::try_from(text.len()).unwrap())]
    );
}

#[test]
fn plain_scalar_on_the_marker_line_is_one_scalar_event_in_a_plain_document() {
    assert_eq!(
        event_variants("--- x\n  y\n"),
        [
            Event::StreamStart,
            explicit_start(),
            flow_plain("x y"),
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

#[rstest]
#[case::next_marker("--- x\n---\nz\n", &["x", "z"])]
#[case::end_marker("--- x\n...\n", &["x"])]
#[case::continuation_then_end_marker("--- x\n  y\n...\n", &["x y"])]
#[case::continuation_then_next_document("--- x\n  y\n--- z\n  w\n", &["x y", "z w"])]
#[case::text_after_the_end_marker("--- x\n...\n  y\n", &["x", "y"])]
#[case::next_marker_with_text("--- x\n--- y\n", &["x", "y"])]
fn plain_scalar_on_the_marker_line_ends_at_the_next_marker(
    #[case] input: &str,
    #[case] values: &[&str],
) {
    let found: Vec<String> = scalars_with_spans(input)
        .into_iter()
        .map(|(value, _, _)| value)
        .collect();
    assert_eq!(found, values, "input: {input:?}");
}

#[test]
fn plain_scalar_after_the_end_marker_with_text_is_an_error() {
    assert!(has_error("--- x\n... y\n"));
}

#[rstest]
#[case::first_document("--- x\n  y\n", 4)]
#[case::after_a_directive("%YAML 1.2\n--- x\n  y\n", 14)]
#[case::inside_a_document("a: b\n--- x\n  y\n", 9)]
#[case::after_an_end_marker("a\n...\n--- x\n  y\n", 10)]
#[case::second_document("--- p\n--- x\n  y\n", 10)]
fn plain_scalar_after_every_kind_of_marker_line_continues(#[case] input: &str, #[case] start: u32) {
    let found = scalars_with_spans(input);
    let last = found.last().expect("a scalar");
    assert_eq!(
        (last.0.as_str(), last.1),
        ("x y", start),
        "input: {input:?}"
    );
}

#[test]
fn version_directive_still_reaches_a_marker_line_scalar_with_continuation() {
    let events = event_variants("%YAML 1.2\n--- x\n  y\n");
    assert!(matches!(
        events.get(1),
        Some(Event::DocumentStart {
            explicit: true,
            version: Some((1, 2)),
            ..
        })
    ));
}

#[rstest]
#[case::tag("--- !t x\n  y\n", None, Some("!t"), 7, 12)]
#[case::anchor("--- &a x\n  y\n", Some("a"), None, 7, 12)]
#[case::anchor_and_continuation_at_column_zero("--- &a x\ny\n", Some("a"), None, 7, 10)]
#[case::tag_then_anchor("--- !t &a x\n  y\n", Some("a"), Some("!t"), 10, 15)]
#[case::anchor_then_tag("--- &a !t x\n  y\n", Some("a"), Some("!t"), 10, 15)]
#[case::secondary_tag("--- !!str x\n  y\n", None, Some("tag:yaml.org,2002:str"), 10, 15)]
#[case::properties_alone_then_text("--- !t\n  x\n  y\n", None, Some("!t"), 9, 14)]
fn properties_and_plain_scalar_on_the_marker_line_continue(
    #[case] input: &str,
    #[case] anchor: Option<&str>,
    #[case] tag: Option<&str>,
    #[case] start: u32,
    #[case] end: u32,
) {
    let items: Vec<_> = parse_events(input).map(Result::unwrap).collect();
    let (event, span) = items
        .iter()
        .find(|(event, _)| matches!(event, Event::Scalar { .. }))
        .expect("a scalar");
    if let Event::Scalar { value, .. } = event {
        assert_eq!(value, "x y", "input: {input:?}");
    }
    assert_eq!((event.anchor(), event.tag()), (anchor, tag), "{input:?}");
    assert_eq!((span.start, span.end), (start, end), "{input:?}");
}

#[rstest]
#[case::comment_after("--- x # c\n", "x", (4, 5), " c", (6, 9))]
#[case::comment_with_colon("--- x # y: z\n", "x", (4, 5), " y: z", (6, 12))]
#[case::comment_without_space("--- x #c: d\n", "x", (4, 5), "c: d", (6, 11))]
#[case::comment_after_two_words("--- x y # c: d\n", "x y", (4, 7), " c: d", (8, 14))]
#[case::empty_comment("--- x #\n", "x", (4, 5), "", (6, 7))]
#[case::comment_after_a_continuation("--- x\n  y # c\n", "x y", (4, 9), " c", (10, 13))]
#[case::comment_on_the_next_line("--- x\n  # c\n", "x", (4, 5), " c", (8, 11))]
#[case::crlf_comment("--- x # c\r\n--- y\r\n", "x", (4, 5), " c", (6, 9))]
fn plain_scalar_on_the_marker_line_is_followed_by_its_comment(
    #[case] input: &str,
    #[case] value: &str,
    #[case] scalar_span: (u32, u32),
    #[case] comment: &str,
    #[case] comment_span: (u32, u32),
) {
    let items: Vec<_> = parse_events(input).map(Result::unwrap).collect();
    let scalar = items
        .iter()
        .position(|(event, _)| matches!(event, Event::Scalar { .. }))
        .expect("a scalar");
    let comment_at = items
        .iter()
        .position(|(event, _)| matches!(event, Event::Comment { .. }))
        .expect("a comment");
    assert_eq!(comment_at, scalar + 1, "the comment follows the scalar");
    let (scalar_event, span) = &items[scalar];
    assert!(matches!(scalar_event, Event::Scalar { value: v, .. } if v == value));
    assert_eq!((span.start, span.end), scalar_span);
    let (comment_event, span) = &items[comment_at];
    assert!(matches!(comment_event, Event::Comment { text } if *text == comment));
    assert_eq!((span.start, span.end), comment_span);
    let comments = items
        .iter()
        .filter(|(event, _)| matches!(event, Event::Comment { .. }))
        .count();
    assert_eq!(comments, 1, "the comment appears once");
}

/// A comment ends a plain scalar, so text below it is a second root node
/// ([135]: the comment cannot sit inside the multi-line scalar).
#[rstest]
#[case::comment_line_then_text("--- v\n# d\nw\n", 3)]
#[case::indented_comment_line_then_text("--- x\n  # c\n  y\n", 3)]
#[case::blank_and_comment_line_then_text("--- x\n\n# c\ny\n", 4)]
#[case::comment_after_the_scalar_then_text("--- x # c\ny\n", 2)]
#[case::comment_after_a_continuation_then_text("--- x\n  y # c\nz\n", 3)]
#[case::comment_after_the_scalar_then_continuation("--- x # c\n  y\n", 2)]
fn text_after_a_comment_below_the_marker_line_scalar_is_an_error(
    #[case] input: &str,
    #[case] line: usize,
) {
    let items = parse_to_vec(input);
    let error = items
        .iter()
        .find_map(|item| item.as_ref().err())
        .expect("an error");
    assert_eq!(error.pos.line, line, "input: {input:?}");
    let values: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            Ok((Event::Scalar { value, .. }, _)) => Some(value.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(values.len(), 1, "only the first scalar: {values:?}");
}

#[rstest]
#[case::key("--- x: 1\n")]
#[case::key_with_empty_value("--- x:\n")]
#[case::key_with_space_before_colon("--- x :\n")]
#[case::key_with_space_before_colon_and_value("--- x : y\n")]
#[case::hash_inside_a_key("--- x#y: z\n")]
#[case::two_word_key("--- x y: 1\n")]
#[case::colon_inside_a_key("--- a:b: c\n")]
#[case::double_colon("--- x::\n")]
#[case::tab_before_colon("--- x\t: 1\n")]
#[case::tab_after_colon("--- x:\t1\n")]
#[case::multibyte_key("--- é: 1\n")]
#[case::sequence_entry("--- - a\n")]
#[case::bare_dash("--- -\n")]
#[case::dash_then_tab("--- -\tx\n")]
#[case::explicit_key("--- ? a\n")]
#[case::bare_question_mark("--- ?\n")]
#[case::question_mark_then_tab("--- ?\tx\n")]
#[case::value_indicator("--- : a\n")]
#[case::bare_colon("--- :\n")]
#[case::colon_then_tab("--- :\tx\n")]
#[case::key_on_the_next_line("--- x\ny: 1\n")]
#[case::key_on_a_continuation_line("--- x\n  y: 1\n")]
#[case::key_with_empty_value_below("--- x\n  y:\n")]
#[case::key_with_space_before_colon_below("--- x\n  y :\n")]
#[case::value_indicator_below("--- x\n: y\n")]
#[case::indented_value_indicator_below("--- x\n  : y\n")]
#[case::bare_colon_below("--- x\n  :\n")]
#[case::tagged_key_below("--- !t x\n  y: 1\n")]
fn plain_scalar_on_the_marker_line_never_opens_a_collection(#[case] input: &str) {
    let items = parse_to_vec(input);
    assert!(items.iter().any(Result::is_err), "input: {input:?}");
    assert!(
        !items.iter().any(|item| matches!(
            item,
            Ok((Event::MappingStart { .. } | Event::SequenceStart { .. }, _))
        )),
        "input: {input:?}"
    );
}

#[rstest]
#[case::marker_line_text("--- x\n%YAML 1.2\n---\n", "x %YAML 1.2")]
#[case::marker_line_text_other_directive("--- x\n%FOO\n---\n", "x %FOO")]
#[case::bare_document("x\n%YAML 1.2\n---\n", "x %YAML 1.2")]
fn directive_looking_line_below_a_plain_scalar_is_part_of_it(
    #[case] input: &str,
    #[case] first: &str,
) {
    let found = scalars_with_spans(input);
    assert_eq!(found.first().map(|s| s.0.as_str()), Some(first));
    let documents = event_variants(input)
        .iter()
        .filter(|e| matches!(e, Event::DocumentStart { .. }))
        .count();
    assert_eq!(documents, 2, "the next document follows: {input:?}");
}

/// Every plain body that is valid after `---` reads the same on the marker line
/// and on the line below, and where one is an error, so is the other.
#[rstest]
#[case::word("x")]
#[case::words("x y")]
#[case::two_lines("x\ny")]
#[case::indented_continuation("x\n  y")]
#[case::blank_line("x\n\ny")]
#[case::blank_lines_and_trailing_blank_lines("x\n\n\n  y\n\n")]
#[case::comment("x # c")]
#[case::comment_after_a_continuation("x\n  y # c")]
#[case::comment_line_below("x\n  # c")]
#[case::percent_line_below("x\n%y")]
#[case::dash_line_below("x\n- y")]
#[case::question_mark_line_below("x\n? y")]
#[case::tab_line_below("x\n\ty")]
#[case::multibyte("é\n  ü")]
#[case::multibyte_with_comment("日本 # c\n")]
#[case::crlf("x\r\n  y\r\n")]
#[case::dots_inside("x ...")]
#[case::end_marker_below("x\n...\ny")]
#[case::start_marker_below("x\n---\ny")]
#[case::key_line_below("x\ny: 1")]
#[case::value_indicator_below("x\n: y")]
#[case::comment_line_then_text("x\n# c\ny")]
fn marker_line_plain_scalar_reads_as_the_scalar_on_the_next_line(#[case] body: &str) {
    let same_line = event_trace(&format!("--- {body}\n"));
    let next_line = event_trace(&format!("---\n{body}\n"));
    assert_eq!(same_line, next_line, "body: {body:?}");
}

#[rstest]
#[case::multibyte_key_cut("--- é: 1\n")]
#[case::multibyte_comment_cut("--- é\n  ü # c\r\n")]
#[case::wide_text_cut("--- 日本 # c: d\n  語\n--- x\n")]
fn marker_line_plain_scalar_does_not_panic_on_any_prefix(#[case] input: &str) {
    for end in (0..=input.len()).filter(|&end| input.is_char_boundary(end)) {
        let _ = parse_to_vec(&input[..end]);
    }
}

#[rstest]
#[case::comment_at_the_limit(MAX_COMMENT_LEN, false)]
#[case::comment_over_the_limit(MAX_COMMENT_LEN + 1, true)]
fn marker_line_scalar_comment_is_limited(#[case] length: usize, #[case] rejected: bool) {
    let input = format!("--- x #{}\n", "c".repeat(length));
    assert_eq!(has_error(&input), rejected, "comment of {length} bytes");
}

#[rstest]
#[case::control_character_in_the_comment("--- x # \u{1}\n")]
#[case::nul_after_the_scalar("--- x\0\n")]
#[case::bom_inside_the_scalar("--- x\u{feff}y\n")]
#[case::delete_character_in_the_comment("--- x\n  y # \u{7f}\n")]
fn marker_line_scalar_rejects_non_printable_characters(#[case] input: &str) {
    assert!(has_error(input), "input: {input:?}");
}
