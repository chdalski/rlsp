// SPDX-License-Identifier: MIT
//
// `parse_events` against libfyaml: inputs that once lost or misplaced text, with
// the outcome of `fy-tool --testsuite` from libfyaml 0.9.6 — its events, or an
// error where it reports one.  The parser reads an input like libfyaml or
// reports an error, so the formatter has no text to delete.
//
// The expected events are the output of `fy-tool --testsuite <file>` on the
// bytes of the input, recorded once; the test runs no process.  To record a
// case again, write the input to a file and run `fy-tool --testsuite` on it.
// For an error the table keeps only the verdict: libfyaml stops at another
// event than the parser does.
//
// Outside the table: the `%` and non-printable lines of the block dispatcher
// and `--- x # c\n%y\n`, which report an error here on purpose while libfyaml
// loads them (`docs/conformance/design-decisions.md`), and `[x]:2`, which keeps
// the behavior it had before (the parser loads it, libfyaml reports an error;
// open on `.ai/backlog.md`).

use std::fmt::Write as _;

use rlsp_yaml_parser::{CollectionStyle, Event, EventMeta, ScalarStyle, load, parse_events};
use rstest::rstest;

// ---- Rendering --------------------------------------------------------------

/// The properties of a node in the notation of `fy-tool --testsuite`: `&anchor`
/// then `<tag>`, each after a space.
fn properties(meta: Option<&EventMeta<'_>>) -> String {
    let mut out = String::new();
    if let Some(meta) = meta {
        if let Some(anchor) = meta.anchor {
            let _ = write!(out, " &{anchor}");
        }
        if let Some(tag) = &meta.tag {
            let _ = write!(out, " <{tag}>");
        }
    }
    out
}

/// The scalar value with the escapes of the test-suite notation.
fn escaped(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\u{8}', "\\b")
}

/// The line of `fy-tool --testsuite` for `event`; `None` for a comment, which
/// libfyaml does not report.
fn line(event: &Event<'_>) -> Option<String> {
    Some(match event {
        Event::StreamStart => "+STR".to_string(),
        Event::StreamEnd => "-STR".to_string(),
        Event::Comment { .. } => return None,
        Event::Alias { name } => format!("=ALI *{name}"),
        Event::DocumentStart { explicit, .. } => {
            if *explicit { "+DOC ---" } else { "+DOC" }.to_string()
        }
        Event::DocumentEnd { explicit } => if *explicit { "-DOC ..." } else { "-DOC" }.to_string(),
        Event::SequenceStart { style, meta } => {
            let flow = if *style == CollectionStyle::Flow {
                " []"
            } else {
                ""
            };
            format!("+SEQ{flow}{}", properties(meta.as_deref()))
        }
        Event::SequenceEnd => "-SEQ".to_string(),
        Event::MappingStart { style, meta } => {
            let flow = if *style == CollectionStyle::Flow {
                " {}"
            } else {
                ""
            };
            format!("+MAP{flow}{}", properties(meta.as_deref()))
        }
        Event::MappingEnd => "-MAP".to_string(),
        Event::Scalar { value, style, meta } => {
            let indicator = match style {
                ScalarStyle::Plain => ':',
                ScalarStyle::SingleQuoted => '\'',
                ScalarStyle::DoubleQuoted => '"',
                ScalarStyle::Literal(_) => '|',
                ScalarStyle::Folded(_) => '>',
            };
            format!(
                "=VAL{} {indicator}{}",
                properties(meta.as_deref()),
                escaped(value)
            )
        }
    })
}

/// The events of `input` as `fy-tool --testsuite` prints them, one per line; the
/// first error of the parser instead.
fn render_events(input: &str) -> Result<String, String> {
    let mut out = String::new();
    for event in parse_events(input) {
        let (event, _) = event.map_err(|e| e.to_string())?;
        if let Some(line) = line(&event) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    Ok(out)
}

// ---- The renderer -----------------------------------------------------------

#[rstest]
#[case::flow_collections_with_properties(
    "&s !t [a]: &m {b: *s}\n",
    "+STR\n+DOC\n+MAP\n+SEQ [] &s <!t>\n=VAL :a\n-SEQ\n+MAP {} &m\n=VAL :b\n=ALI *s\n-MAP\n-MAP\n-DOC\n-STR\n"
)]
#[case::markers("--- a\n...\n", "+STR\n+DOC ---\n=VAL :a\n-DOC ...\n-STR\n")]
#[case::quoted_scalars_and_escapes(
    "- 'a'\n- \"b\\t\\\\\"\n",
    "+STR\n+DOC\n+SEQ\n=VAL 'a\n=VAL \"b\\t\\\\\n-SEQ\n-DOC\n-STR\n"
)]
#[case::block_scalars(
    "- |\n a\n- >\n b\n",
    "+STR\n+DOC\n+SEQ\n=VAL |a\\n\n=VAL >b\\n\n-SEQ\n-DOC\n-STR\n"
)]
#[case::comments_are_left_out(
    "a: b # c\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :b\n-MAP\n-DOC\n-STR\n"
)]
fn render_events_prints_the_notation_of_fy_tool(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(render_events(input).as_deref(), Ok(expected));
}

// ---- The table --------------------------------------------------------------

#[rstest]
// ---- Task 2 ------------------------------------------------------
#[case::marker_scalar_continued_by_an_indented_line(
    "--- x\n  y\n",
    "+STR\n+DOC ---\n=VAL :x y\n-DOC\n-STR\n"
)]
#[case::marker_scalar_continued_by_a_flush_line(
    "--- x\ny\n",
    "+STR\n+DOC ---\n=VAL :x y\n-DOC\n-STR\n"
)]
#[case::bare_scalar_continued_by_a_flush_line("  x\ny\n", "+STR\n+DOC\n=VAL :x y\n-DOC\n-STR\n")]
#[case::marker_scalar_continued_by_a_percent_line(
    "--- x\n%y\n",
    "+STR\n+DOC ---\n=VAL :x %y\n-DOC\n-STR\n"
)]
#[case::marker_scalar_continued_by_an_at_line(
    "--- x\n  @y\n",
    "+STR\n+DOC ---\n=VAL :x @y\n-DOC\n-STR\n"
)]
#[case::marker_scalar_with_a_comment_holding_a_colon(
    "--- x # y: z\n",
    "+STR\n+DOC ---\n=VAL :x\n-DOC\n-STR\n"
)]
// ---- Task 4 ------------------------------------------------------
#[case::key_then_colon_line(
    "a:\n: c\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :\n=VAL :\n=VAL :c\n-MAP\n-DOC\n-STR\n"
)]
#[case::other_key_then_colon_line(
    "b:\n: x\n",
    "+STR\n+DOC\n+MAP\n=VAL :b\n=VAL :\n=VAL :\n=VAL :x\n-MAP\n-DOC\n-STR\n"
)]
#[case::empty_key_then_colon_line(
    ":\n: c\n",
    "+STR\n+DOC\n+MAP\n=VAL :\n=VAL :\n=VAL :\n=VAL :c\n-MAP\n-DOC\n-STR\n"
)]
#[case::bare_explicit_key_then_two_colon_lines(
    "?\n:\n: c\n",
    "+STR\n+DOC\n+MAP\n=VAL :\n=VAL :\n=VAL :\n=VAL :c\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_then_two_colon_lines(
    "? a\n:\n: c\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :\n=VAL :\n=VAL :c\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_colon_line(
    "x:\n  : v\n",
    "+STR\n+DOC\n+MAP\n=VAL :x\n+MAP\n=VAL :\n=VAL :v\n-MAP\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_colon_line_with_a_sequence(
    "x:\n  :\n    - 1\n",
    "+STR\n+DOC\n+MAP\n=VAL :x\n+MAP\n=VAL :\n+SEQ\n=VAL :1\n-SEQ\n-MAP\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_key_then_colon_line(
    "a: 1\n[x]:\n: v\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :1\n+SEQ []\n=VAL :x\n-SEQ\n=VAL :\n=VAL :\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchored_value_then_anchored_empty_key(
    "a: &y\n&k : v\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL &y :\n=VAL &k :\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::alias_key("*a : v\n", "+STR\n+DOC\n+MAP\n=ALI *a\n=VAL :v\n-MAP\n-DOC\n-STR\n")]
#[case::alias_key_without_value("*a :\n", "+STR\n+DOC\n+MAP\n=ALI *a\n=VAL :\n-MAP\n-DOC\n-STR\n")]
#[case::alias_key_with_a_comment(
    "*a : # c\n",
    "+STR\n+DOC\n+MAP\n=ALI *a\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_key(
    "[x]: v\n",
    "+STR\n+DOC\n+MAP\n+SEQ []\n=VAL :x\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_key_without_value(
    "[x]:\n",
    "+STR\n+DOC\n+MAP\n+SEQ []\n=VAL :x\n-SEQ\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_key_with_a_comment(
    "[x]: # c\n",
    "+STR\n+DOC\n+MAP\n+SEQ []\n=VAL :x\n-SEQ\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchored_flow_key(
    "&k [x]: v\n",
    "+STR\n+DOC\n+MAP\n+SEQ [] &k\n=VAL :x\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::alias_key_in_a_sequence(
    "- *a : v\n",
    "+STR\n+DOC\n+SEQ\n+MAP\n=ALI *a\n=VAL :v\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
// ---- Task 5 ------------------------------------------------------
#[case::bare_dash_comment_then_indented_value(
    "-\n# c\n  b\n",
    "+STR\n+DOC\n+SEQ\n=VAL :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::bare_dash_indented_comment("-\n  # d\n", "+STR\n+DOC\n+SEQ\n=VAL :\n-SEQ\n-DOC\n-STR\n")]
#[case::bare_dash_blank_line_then_indented_value(
    "-\n\n  b\n",
    "+STR\n+DOC\n+SEQ\n=VAL :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::bare_dash_blank_indented_line("-\n  \n", "+STR\n+DOC\n+SEQ\n=VAL :\n-SEQ\n-DOC\n-STR\n")]
#[case::bare_dash_comment_then_next_item(
    "-\n# about x\n- x\n",
    "+STR\n+DOC\n+SEQ\n=VAL :\n=VAL :x\n-SEQ\n-DOC\n-STR\n"
)]
// ---- Task 6 ------------------------------------------------------
#[case::flow_sequence_plain_scalar_over_a_blank_line(
    "[a\n\n  b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\\nb\n-SEQ\n-DOC\n-STR\n"
)]
// ---- Task 8 ------------------------------------------------------
#[case::explicit_key_with_an_anchor_then_an_anchored_entry(
    "? &x\n&zz : v\n: v\n",
    "+STR\n+DOC\n+MAP\n=VAL &x :\n=VAL :\n=VAL &zz :\n=VAL :v\n=VAL :\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_with_a_tag_then_a_tagged_entry(
    "? !!str\n!zz : v\n? &x\n",
    "+STR\n+DOC\n+MAP\n=VAL <tag:yaml.org,2002:str> :\n=VAL :\n=VAL <!zz> :\n=VAL :v\n=VAL &x :\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_in_a_sequence_then_an_anchored_entry(
    "- ? &d\n  &zz : v\n",
    "+STR\n+DOC\n+SEQ\n+MAP\n=VAL &d :\n=VAL :\n=VAL &zz :\n=VAL :v\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::explicit_key_with_an_anchor_then_a_tagged_entry(
    "? &x\n!zz : v\n",
    "+STR\n+DOC\n+MAP\n=VAL &x :\n=VAL :\n=VAL <!zz> :\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_with_a_plain_key_then_an_anchored_entry(
    "? a\n&zz : v\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :\n=VAL &zz :\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::bare_explicit_key_then_an_anchored_entry(
    "?\n&zz : v\n",
    "+STR\n+DOC\n+MAP\n=VAL :\n=VAL :\n=VAL &zz :\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchored_tag_line_then_anchored_tagged_key(
    "&a4 !!map\n&a5 !!str key5: value4\n",
    "+STR\n+DOC\n+MAP &a4 <tag:yaml.org,2002:map>\n=VAL &a5 <tag:yaml.org,2002:str> :key5\n=VAL :value4\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_line_then_anchored_tagged_key(
    "&zz\n&a4 !!str k: v\n",
    "+STR\n+DOC\n+MAP &zz\n=VAL &a4 <tag:yaml.org,2002:str> :k\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_line_then_tag_line_then_anchored_key(
    "&zz\n!!map\n&a5 k: v\n",
    "+STR\n+DOC\n+MAP &zz <tag:yaml.org,2002:map>\n=VAL &a5 :k\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_line_then_tagged_key(
    "!!map\n!!str a: 1\n",
    "+STR\n+DOC\n+MAP <tag:yaml.org,2002:map>\n=VAL <tag:yaml.org,2002:str> :a\n=VAL :1\n-MAP\n-DOC\n-STR\n"
)]
#[case::marker_tag_then_tagged_key(
    "--- !t\n!u a: b\n",
    "+STR\n+DOC ---\n+MAP <!t>\n=VAL <!u> :a\n=VAL :b\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_tag_line_then_anchor_tag_key(
    "&a !t\n&b !u k: v\n",
    "+STR\n+DOC\n+MAP &a <!t>\n=VAL &b <!u> :k\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_line_then_anchored_key(
    "!t\n&a k: v\n",
    "+STR\n+DOC\n+MAP <!t>\n=VAL &a :k\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_line_then_tagged_key(
    "&a\n!t k: v\n",
    "+STR\n+DOC\n+MAP &a\n=VAL <!t> :k\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_and_anchor_then_a_mapping(
    "a: !t &a\n  b: c\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n+MAP &a <!t>\n=VAL :b\n=VAL :c\n-MAP\n-MAP\n-DOC\n-STR\n"
)]
#[case::marker_tag_and_anchor_then_a_tagged_key(
    "---\n!t &a\n!zz a: b\n",
    "+STR\n+DOC ---\n+MAP &a <!t>\n=VAL <!zz> :a\n=VAL :b\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_and_anchor_then_a_tagged_key_in_a_value(
    "a: !t &a\n  !zz b: c\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n+MAP &a <!t>\n=VAL <!zz> :b\n=VAL :c\n-MAP\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_and_anchor_then_second_tag_and_anchor_on_a_flow_key(
    "!t &a\n!z &w [a]:\n",
    "+STR\n+DOC\n+MAP &a <!t>\n+SEQ [] &w <!z>\n=VAL :a\n-SEQ\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_and_anchor_then_anchor_and_second_tag_on_a_flow_key(
    "!t &a\n&w !z [a]:\n",
    "+STR\n+DOC\n+MAP &a <!t>\n+SEQ [] &w <!z>\n=VAL :a\n-SEQ\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_and_anchor_then_a_tagged_flow_sequence_key(
    "!t &a\n!zz [y]: v\n",
    "+STR\n+DOC\n+MAP &a <!t>\n+SEQ [] <!zz>\n=VAL :y\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_and_anchor_then_a_tagged_flow_sequence_key_in_a_value(
    "a: !t &a\n  !zz [y]: v\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n+MAP &a <!t>\n+SEQ [] <!zz>\n=VAL :y\n-SEQ\n=VAL :v\n-MAP\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_and_anchor_then_a_tagged_flow_mapping_key(
    "!t &a\n!zz {y: z}: v\n",
    "+STR\n+DOC\n+MAP &a <!t>\n+MAP {} <!zz>\n=VAL :y\n=VAL :z\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_and_anchor_then_a_tagged_flow_sequence_key_in_an_item(
    "- !t &a\n  !zz [y]: v\n",
    "+STR\n+DOC\n+SEQ\n+MAP &a <!t>\n+SEQ [] <!zz>\n=VAL :y\n-SEQ\n=VAL :v\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::tag_line_then_a_tagged_flow_sequence_key(
    "!t\n!zz [y]: v\n",
    "+STR\n+DOC\n+MAP <!t>\n+SEQ [] <!zz>\n=VAL :y\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_and_tag_then_a_tagged_flow_sequence_key(
    "&a !t\n!zz [y]: v\n",
    "+STR\n+DOC\n+MAP &a <!t>\n+SEQ [] <!zz>\n=VAL :y\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_line_then_a_tagged_flow_sequence_key(
    "&p1\n!q1 [y]: v\n",
    "+STR\n+DOC\n+MAP &p1\n+SEQ [] <!q1>\n=VAL :y\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_line_then_an_anchored_flow_sequence_key(
    "!p1\n&q1 [y]: v\n",
    "+STR\n+DOC\n+MAP <!p1>\n+SEQ [] &q1\n=VAL :y\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_line_then_an_anchored_flow_sequence_key(
    "&z\n&w [y]: v\n",
    "+STR\n+DOC\n+MAP &z\n+SEQ [] &w\n=VAL :y\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_line_then_a_tagged_key(
    "&p1\n!q1 k: v\n",
    "+STR\n+DOC\n+MAP &p1\n=VAL <!q1> :k\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_line_then_an_anchored_key(
    "!p1\n&q1 k: v\n",
    "+STR\n+DOC\n+MAP <!p1>\n=VAL &q1 :k\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
fn parse_events_matches_the_events_of_libfyaml(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(
        render_events(input).as_deref(),
        Ok(expected),
        "input: {input:?}"
    );
}

#[rstest]
// ---- Task 2 ------------------------------------------------------
#[case::marker_scalar_then_comment_then_flush_line("--- v\n# d\nw\n")]
#[case::marker_scalar_then_a_key_line("--- x\ny: 1\n")]
#[case::key_then_flush_lines("a:\nw\nx\n")]
#[case::marker_then_sequence_entry("--- - a\n")]
#[case::marker_then_explicit_key("--- ? a\n")]
#[case::marker_then_bare_dash("--- -\n")]
#[case::marker_then_bare_question_mark("--- ?\n")]
// ---- Task 6 ------------------------------------------------------
#[case::flow_sequence_comment_inside_a_plain_scalar("[a # c\n  b]\n")]
// ---- Task 8 ------------------------------------------------------
#[case::anchor_line_then_anchored_tag_line("&zz\n&a4 !!map\nk: v\n")]
#[case::marker_anchor_line_then_anchored_tag_line("---\n&zz\n&a4 !!map\nk: v\n")]
#[case::anchor_line_then_anchored_tagged_scalar("&zz\n&a4 !!str x\n")]
#[case::anchor_line_then_anchored_tagged_empty_scalar("&zz\n&a4 !!str\nk: v\n")]
#[case::anchored_value_then_anchored_tag_line("a: &zz\n  &a4 !!map\n  k: v\n")]
#[case::anchor_and_tag_then_anchored_scalar("a: &a !t\n  &b zz\n")]
#[case::anchor_tag_lines_then_anchored_scalar("&zz\n!!map\n&a5 x\n")]
#[case::anchor_and_tag_then_anchored_scalar_at_the_root("!t &a\n&b zz\n")]
#[case::tag_and_anchor_then_second_tag_in_a_value("a: !t &a\n  !zz\n")]
#[case::tag_and_anchor_then_second_tag_after_the_marker("---\n!t &a\n!zz\n")]
#[case::tag_then_second_tag_in_a_value("a: !t\n  !zz\n")]
#[case::anchor_and_tag_then_second_tag_in_a_value("a: &a !t\n  !zz\n")]
#[case::tag_and_anchor_then_second_tag_in_an_item("- !t &a\n  !zz\n")]
#[case::tag_then_anchor_then_second_tag_in_an_item("- !t\n  &a\n  !zz\n")]
#[case::anchor_line_then_anchored_flow_sequence("&p1\n&q1 [y]\n")]
#[case::anchor_line_then_anchored_flow_mapping("&p1\n&q1 {y: z}\n")]
#[case::anchor_line_then_anchored_literal_scalar("&p1\n&q1 |\n  t\n")]
#[case::anchor_line_then_anchored_folded_scalar("&p1\n&q1 >\n  t\n")]
#[case::anchor_line_then_tagged_anchored_flow_sequence("&p1\n!q2 &q1 [y]\n")]
#[case::tag_and_anchor_then_anchored_flow_mapping_with_a_pair("!t &a\n&z [a: b]\n")]
#[case::anchor_line_then_anchored_alias("&p1\n&q1 *al\n")]
#[case::marker_anchor_line_then_anchored_flow_sequence("---\n&p1\n&q1 [y]\n")]
#[case::anchored_value_then_anchored_flow_sequence("a:\n  &p1\n  &q1 [y]\n")]
#[case::anchored_item_then_anchored_flow_sequence("- &p1\n  &q1 [y]\n")]
#[case::anchored_explicit_key_then_anchored_flow_sequence("? &p1\n  &q1 [y]\n")]
fn parse_events_reports_an_error_where_libfyaml_does(#[case] input: &str) {
    let results: Vec<_> = parse_events(input).collect();
    let first_error = results.iter().position(Result::is_err);
    assert!(first_error.is_some(), "no error for {input:?}");
    assert_eq!(
        first_error,
        Some(results.len() - 1),
        "events after the error for {input:?}"
    );
    assert!(load(input).is_err(), "the loader reads {input:?}");
}
