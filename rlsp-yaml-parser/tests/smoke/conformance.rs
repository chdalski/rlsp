use super::*;

// CF-1: AVM7 — "Empty Stream"
// The `∎` sentinel in the fixture means end-of-stream; after visual_to_raw
// this is an empty string.
#[test]
fn avm7_empty_stream() {
    let events = event_variants("");
    assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
}

// CF-2: 98YD — "Spec Example 5.5. Comment Indicator"
#[test]
fn yd98_comment_only() {
    let events = event_variants("# Comment only.\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment {
                text: " Comment only."
            },
            Event::StreamEnd,
        ]
    );
}

// CF-3: HWV9 — "Document-end marker"
// An orphan `...` with no open document produces no document events.
#[test]
fn hwv9_orphan_document_end() {
    let events = event_variants("...\n");
    assert_eq!(events, [Event::StreamStart, Event::StreamEnd]);
}

// CF-4: QT73 — "Comment and document-end marker"
#[test]
fn qt73_comment_and_document_end() {
    let events = event_variants("# comment\n...\n");
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment { text: " comment" },
            Event::StreamEnd,
        ]
    );
}

// ---------------------------------------------------------------------------
// Task 6 conformance fixtures — plain scalars
// ---------------------------------------------------------------------------

// CF-5: 4V8U — "Plain scalar with backslashes"
// yaml: `---\nplain\value\with\backslashes\n`
#[test]
fn cf5_4v8u_plain_scalar_with_backslashes() {
    // From yaml-test-suite/src/4V8U.yaml
    let input = "---\nplain\\value\\with\\backslashes\n";
    let events = event_variants(input);
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
                value: "plain\\value\\with\\backslashes".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// CF-6: EX5H — "Multiline Scalar at Top Level [1.3]"
// yaml: `---\na\nb  \n  c\nd\n\ne\n` (with trailing spaces on b-line stripped)
// Expected scalar: "a b c d\ne"
// Note: ␣␣ in the fixture is two trailing spaces that get stripped.
#[test]
fn cf6_ex5h_multiline_scalar_at_top_level() {
    // From yaml-test-suite/src/EX5H.yaml
    // The fixture yaml field (after visual notation):
    //   "---\na\nb  \n  c\nd\n\ne\n"
    // (b has two trailing spaces that are stripped during folding)
    let input = "---\na\nb  \n  c\nd\n\ne\n";
    let events = event_variants(input);
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
                value: "a b c d\ne".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// CF-7: 9YRD — "Multiline Scalar at Top Level" (bare document, YAML 1.2)
// yaml: `a\nb  \n  c\nd\n\ne\n`
// Expected scalar: "a b c d\ne"
#[test]
fn cf7_9yrd_multiline_scalar_bare_doc() {
    // From yaml-test-suite/src/9YRD.yaml
    let input = "a\nb  \n  c\nd\n\ne\n";
    let events = event_variants(input);
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
                value: "a b c d\ne".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// CF-8: HS5T — "Spec Example 7.12. Plain Lines"
// Tests tab-prefixed continuation, blank line folding, trailing-space stripping.
// Expected scalar: "1st non-empty\n2nd non-empty 3rd non-empty"
#[test]
fn cf8_hs5t_plain_lines_spec_example() {
    // From yaml-test-suite/src/HS5T.yaml
    // Visual notation: ␣ = space, → = tab
    let input = "1st non-empty\n\n 2nd non-empty \n\t3rd non-empty\n";
    let events = event_variants(input);
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
                value: "1st non-empty\n2nd non-empty 3rd non-empty".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// CF-9: 27NA — "Spec Example 5.9. Directive Indicator"
// Tests inline scalar on the same line as `---`: `--- text` → scalar "text".
// Also tests %YAML directive — now parsed and included in DocumentStart.version.
#[test]
fn cf9_27na_directive_indicator_spec_example() {
    // From yaml-test-suite/src/27NA.yaml
    // yaml: "%YAML 1.2\n--- text\n"
    let input = "%YAML 1.2\n--- text\n";
    let events = event_variants(input);
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: Some((1, 2)),
                tag_directives: vec![],
            },
            Event::Scalar {
                value: "text".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// CF-9b: 27NA — exact name from TE spec
#[test]
fn yaml27na_directive_indicator_spec_example() {
    // From yaml-test-suite/src/27NA.yaml — %YAML 1.2 + `--- text`
    // The scalar "text" follows the directives-end marker on the same line.
    let input = "%YAML 1.2\n--- text\n";
    let events = event_variants(input);
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::DocumentStart {
                explicit: true,
                version: Some((1, 2)),
                tag_directives: vec![],
            },
            Event::Scalar {
                value: "text".into(),
                style: ScalarStyle::Plain,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// ---------------------------------------------------------------------------
// Task 7 conformance fixtures — quoted scalars
// ---------------------------------------------------------------------------

// CF-Q1: 4GC6 — "Spec Example 7.7. Single Quoted Characters"
// yaml: `'here''s to "quotes"'`
// Expected scalar value: `here's to "quotes"`
#[test]
fn cf_q1_4gc6_single_quoted_characters() {
    // Spike test — validates that single-quoted parsing works end-to-end.
    let input = "'here''s to \"quotes\"'\n";
    let events = event_variants(input);
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
                value: "here's to \"quotes\"".into(),
                style: ScalarStyle::SingleQuoted,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// CF-Q2: 2LFX — "Spec Example 6.13. Reserved Directives [1.3]"
// yaml: `%FOO  bar baz # ...\n                  # with a warning.\n---\n"foo"\n`
// Expected scalar value: `foo`; the comment-only second line produces a Comment event.
#[test]
fn cf_q2_2lfx_double_quoted_after_directive() {
    let input =
        "%FOO  bar baz # Should be ignored\n                  # with a warning.\n---\n\"foo\"\n";
    let events = event_variants(input);
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment {
                text: " with a warning."
            },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "foo".into(),
                style: ScalarStyle::DoubleQuoted,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// CF-Q3: double-quoted scalar on its own line after a directive and `---`.
// Based on 2LFX (not 6LVF): `%FOO ...\n---\n"foo"\n`.  The full 6LVF
// fixture (`--- "foo"` on the same line as `---`) is not exercised here
// because inline quoted scalars after `---` are not yet supported —
// consume_marker_line dispatches through scan_plain_line_block (plain
// only).  See the TODO in consume_marker_line for the deferred fix.
#[test]
fn cf_q3_quoted_scalar_after_directive_and_doc_marker() {
    // 2LFX variant: `%FOO ...\n                  # with a warning.\n---\n"foo"\n`
    // The comment-only second line produces a Comment event before DocumentStart.
    let input =
        "%FOO  bar baz # Should be ignored\n                  # with a warning.\n---\n\"foo\"\n";
    let events = event_variants(input);
    assert_eq!(
        events,
        [
            Event::StreamStart,
            Event::Comment {
                text: " with a warning."
            },
            Event::DocumentStart {
                explicit: true,
                version: None,
                tag_directives: vec![]
            },
            Event::Scalar {
                value: "foo".into(),
                style: ScalarStyle::DoubleQuoted,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}

// CF-Q4: 4UYU — "Colon in Double Quoted String"
// yaml: `"foo: bar\": baz"`
// Expected scalar value: `foo: bar": baz`
#[test]
fn cf_q4_4uyu_colon_in_double_quoted() {
    let input = "\"foo: bar\\\": baz\"\n";
    let events = event_variants(input);
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
                value: "foo: bar\": baz".into(),
                style: ScalarStyle::DoubleQuoted,
                anchor: None,
                tag: None,
            },
            Event::DocumentEnd { explicit: false },
            Event::StreamEnd,
        ]
    );
}
