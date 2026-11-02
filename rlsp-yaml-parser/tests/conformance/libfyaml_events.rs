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

#![expect(
    clippy::literal_string_with_formatting_args,
    reason = "a flow mapping in an input looks like a format argument"
)]

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
// ---- Task 9 ------------------------------------------------------
#[case::flow_plain_continued_by_an_anchor(
    "[a\n &x b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_tag_over_a_blank_line(
    "[a\n\n !t b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\\n!t b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_double_quote(
    "[a\n \"b\"]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a \"b\"\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_single_quote_over_a_blank_line(
    "[a\n\n 'b']\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\\n'b'\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_alias(
    "[a\n *x]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a *x\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_dash_over_a_blank_line(
    "[a\n\n - b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\\n- b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_dash(
    "[a\n - b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a - b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_pipe_over_a_blank_line(
    "[a\n\n |b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\\n|b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_fold_indicator(
    "[a\n >x]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a >x\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_question_mark(
    "[a\n ? b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a ? b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_lone_bang(
    "[a\n !]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a !\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_lone_ampersand(
    "[a\n &]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_lone_star(
    "[a\n *]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a *\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_pipe_at_line_end(
    "[a\n |\n b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a | b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_fold_indicator_at_line_end(
    "[a\n >\n b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a > b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_dash_at_line_end(
    "[a\n -\n b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a - b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_dash_before_a_comma(
    "[a\n -, b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a -\n=VAL :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_question_mark_before_a_comma(
    "[a\n ?, b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a ?\n=VAL :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_anchor_before_a_comma(
    "[a\n &x, b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x\n=VAL :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_alias_before_a_comma(
    "[a\n *x, b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a *x\n=VAL :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_undefined_tag_handle(
    "[a\n !t!u b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a !t!u b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_broken_verbatim_tag(
    "[a\n !<tag b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a !<tag b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_verbatim_tag(
    "[a\n !<tag:x> b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a !<tag:x> b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_anchor_name_with_a_colon(
    "[a\n &x:b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x:b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_alias_name_with_a_colon(
    "[a\n *x:b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a *x:b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_quote_and_a_colon(
    "[a\n \"b\":c]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a \"b\":c\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_empty_quote_and_text(
    "[a\n \"\" b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a \"\" b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_alias_and_text(
    "[a\n *x b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a *x b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_escaped_single_quote(
    "[a\n 'it''s']\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a 'it''s'\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_unterminated_double_quote(
    "[a\n \"b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a \"b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_unterminated_single_quote(
    "[a\n 'b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a 'b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_quote_holding_a_comma(
    "[a\n \"b,c\"]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a \"b\n=VAL :c\"\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_anchor_and_trailing_space(
    "[a\n &x b ]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_a_quote_and_trailing_space(
    "[a\n \"b\" ]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a \"b\"\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_anchor_in_multibyte_text(
    "[é\n &x ü]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :é &x ü\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_two_anchor_lines(
    "[a\n &x b\n &y c]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x b &y c\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_anchor_line_and_an_alias_line_over_a_blank_line(
    "[a\n &x b\n\n *y c]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x b\\n*y c\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_alias_line_then_text(
    "[a\n *x\n c]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a *x c\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_anchor_line_then_text(
    "[a\n &x\n b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_anchor_over_two_blank_lines(
    "[a\n\n\n &x b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\\n\\n&x b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_anchor_over_a_whitespace_line(
    "[a\n \n\n &x b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\\n\\n&x b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_anchor_after_a_tab(
    "[a\n  \t &x b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_continued_by_an_anchor_then_the_closing_bracket_line(
    "[a\n &x b\n]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_with_an_anchor_continued_by_an_anchor(
    "[&x a\n &y b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL &x :a &y b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_with_a_tag_continued_by_a_tag(
    "[!t a\n !u b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL <!t> :a !u b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_entry_after_a_comma_keeps_its_anchor(
    "[a, &x b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\n=VAL &x :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_entry_on_the_line_after_a_comma_keeps_its_anchor(
    "[a,\n &x b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\n=VAL &x :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_entry_after_a_comma_and_a_blank_line_keeps_its_anchor(
    "[a,\n\n &x b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\n=VAL &x :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_entry_after_a_comma_line_keeps_its_anchor(
    "[a\n, &x b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\n=VAL &x :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_entry_after_a_comma_line_keeps_its_quote(
    "[a\n, \"b\"]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\n=VAL \"b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_first_item_on_its_own_line_keeps_its_anchor(
    "[\n &x b]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL &x :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_first_item_on_its_own_line_keeps_its_quote(
    "[\n \"b\"]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL \"b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_first_item_on_its_own_line_keeps_its_alias(
    "[\n *x]\n",
    "+STR\n+DOC\n+SEQ []\n=ALI *x\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_anchor_line_then_a_quote(
    "[&x\n \"b\"]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL &x \"b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_continuation_then_a_comma(
    "[a\n &x b, c]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x b\n=VAL :c\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_continuation_then_a_comma_and_an_anchored_entry(
    "[a\n &x b, &y c]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x b\n=VAL &y :c\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_continuation_then_a_comma_line_and_an_anchored_entry(
    "[a\n &x b,\n &y c]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a &x b\n=VAL &y :c\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_plain_then_a_comma_and_an_anchored_entry(
    "[a\n b, &x c]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a b\n=VAL &x :c\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_second_entry_continued_by_an_anchor(
    "[a, b\n &x c, d]\n",
    "+STR\n+DOC\n+SEQ []\n=VAL :a\n=VAL :b &x c\n=VAL :d\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_pair_value_on_the_next_line_keeps_its_anchor(
    "[a:\n &x b]\n",
    "+STR\n+DOC\n+SEQ []\n+MAP {}\n=VAL :a\n=VAL &x :b\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_mapping_value_on_the_next_line_keeps_its_anchor(
    "{a:\n &x b}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a\n=VAL &x :b\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_colon_line_then_an_anchored_value(
    "{a\n : &x b}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a\n=VAL &x :b\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_entry_after_a_comma_line_keeps_its_anchor(
    "{a: b,\n &x c: d}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a\n=VAL :b\n=VAL &x :c\n=VAL :d\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_continued_by_an_anchor(
    "{a\n &x b: 1}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a &x b\n=VAL :1\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_continued_by_an_anchor_before_a_spaced_colon(
    "{a\n &x b : v}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a &x b\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_pair_key_continued_by_an_anchor(
    "[a\n &x b: 1]\n",
    "+STR\n+DOC\n+SEQ []\n+MAP {}\n=VAL :a &x b\n=VAL :1\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_pair_key_continued_by_a_double_quote(
    "[a\n \"b\": 1]\n",
    "+STR\n+DOC\n+SEQ []\n+MAP {}\n=VAL :a \"b\"\n=VAL :1\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_continued_by_a_double_quote(
    "{a\n \"b\": 1}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a \"b\"\n=VAL :1\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_continued_by_an_alias(
    "{a\n *x: 1}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a *x\n=VAL :1\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_pair_key_continued_by_an_alias(
    "[a\n *x: 1]\n",
    "+STR\n+DOC\n+SEQ []\n+MAP {}\n=VAL :a *x\n=VAL :1\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_continued_by_a_question_mark(
    "{a\n ? b: 1}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a ? b\n=VAL :1\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_continued_by_a_quote_and_text(
    "{a\n \"b\" c: 1}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a \"b\" c\n=VAL :1\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_pair_key_continued_by_an_anchor_with_a_continued_value(
    "[a\n &x b: 1\n c]\n",
    "+STR\n+DOC\n+SEQ []\n+MAP {}\n=VAL :a &x b\n=VAL :1 c\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_pair_value_continued_by_an_anchor(
    "[a: b\n &x c]\n",
    "+STR\n+DOC\n+SEQ []\n+MAP {}\n=VAL :a\n=VAL :b &x c\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_pair_key_continued_by_an_anchor_then_an_entry(
    "[a\n &x b: 1, c]\n",
    "+STR\n+DOC\n+SEQ []\n+MAP {}\n=VAL :a &x b\n=VAL :1\n-MAP\n=VAL :c\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_continued_by_an_anchor_then_a_key_continued_by_a_quote(
    "{a\n &x b: 1, c\n \"d\": 2}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a &x b\n=VAL :1\n=VAL :c \"d\"\n=VAL :2\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_pair_key_continued_by_a_quote_holding_a_colon(
    "[a\n \"b: c\"]\n",
    "+STR\n+DOC\n+SEQ []\n+MAP {}\n=VAL :a \"b\n=VAL :c\"\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_continued_by_an_anchor_without_a_value(
    "{a\n &x b}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a &x b\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_continued_without_a_value(
    "{a\n b}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a b\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_continued_by_a_colon_and_text(
    "{a\n :x}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a :x\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_continued_by_a_colon_and_text_before_a_comma(
    "{a\n :x, e: f}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a :x\n=VAL :\n=VAL :e\n=VAL :f\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_explicit_key_continued_by_an_anchor(
    "{? a\n &x b : c}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a &x b\n=VAL :c\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_sequence_explicit_key_continued_by_an_anchor(
    "[? a\n &x b]\n",
    "+STR\n+DOC\n+SEQ []\n+MAP {}\n=VAL :a &x b\n=VAL :\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_pair_key_continued_by_an_anchor_line_and_a_colon(
    "[a\n &zz : v\n b]\n",
    "+STR\n+DOC\n+SEQ []\n+MAP {}\n=VAL :a &zz\n=VAL :v b\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_pair_key_continued_by_a_tag_line_and_a_colon(
    "[a\n !zz : v\n b]\n",
    "+STR\n+DOC\n+SEQ []\n+MAP {}\n=VAL :a !zz\n=VAL :v b\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_nested_sequence_continued_by_an_anchor(
    "[[a\n &x b]]\n",
    "+STR\n+DOC\n+SEQ []\n+SEQ []\n=VAL :a &x b\n-SEQ\n-SEQ\n-DOC\n-STR\n"
)]
#[case::flow_mapping_value_sequence_continued_by_an_anchor(
    "{k: [b\n &x c]}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :k\n+SEQ []\n=VAL :b &x c\n-SEQ\n-MAP\n-DOC\n-STR\n"
)]
#[case::block_sequence_flow_item_continued_by_an_anchor(
    "- [a\n  &x b]\n- c\n",
    "+STR\n+DOC\n+SEQ\n+SEQ []\n=VAL :a &x b\n-SEQ\n=VAL :c\n-SEQ\n-DOC\n-STR\n"
)]
#[case::block_mapping_flow_value_continued_by_an_anchor(
    "a: [b\n  &x c]\nd: 1\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n+SEQ []\n=VAL :b &x c\n-SEQ\n=VAL :d\n=VAL :1\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_over_three_lines(
    "{a\n b\n c: d}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a b c\n=VAL :d\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_over_three_lines_with_an_anchor(
    "{a\n b\n &x c: d}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a b &x c\n=VAL :d\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_over_three_lines_without_a_value(
    "{a\n b\n c}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a b c\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_over_three_lines_ending_in_a_quote(
    "{a\n b\n \"c\": d}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a b \"c\"\n=VAL :d\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_key_over_three_lines_ending_in_a_colon_and_text(
    "{a\n b\n :c}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a b :c\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_mapping_keys_over_three_lines_each(
    "{a\n b\n c: d, e\n f\n g: h}\n",
    "+STR\n+DOC\n+MAP {}\n=VAL :a b c\n=VAL :d\n=VAL :e f g\n=VAL :h\n-MAP\n-DOC\n-STR\n"
)]
// ---- Task 12 -----------------------------------------------------
#[case::nested_explicit_key_with_an_empty_value(
    "?\n  ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::inline_nested_explicit_key(
    "? ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_empty_explicit_key(
    "?\n  ?\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::inline_nested_empty_explicit_key(
    "? ?\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_on_the_outer_explicit_key_of_a_nested_one(
    "? &a\n  ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP &a\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_on_the_empty_inner_explicit_key(
    "?\n  ? &a\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL &a :\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::flow_sequence_as_nested_explicit_key(
    "?\n  ? [a]\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n+SEQ []\n=VAL :a\n-SEQ\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::alias_as_nested_explicit_key(
    "?\n  ? *x\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=ALI *x\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_in_a_sequence_key(
    "?\n  - ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+SEQ\n+MAP\n=VAL :a\n=VAL :\n-MAP\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_in_an_inline_sequence_key(
    "? - ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+SEQ\n+MAP\n=VAL :a\n=VAL :\n-MAP\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_in_a_same_column_sequence_key(
    "?\n- ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+SEQ\n+MAP\n=VAL :a\n=VAL :\n-MAP\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_in_the_second_item_of_a_sequence_key(
    "?\n- a\n- ? b\n: v\n",
    "+STR\n+DOC\n+MAP\n+SEQ\n=VAL :a\n+MAP\n=VAL :b\n=VAL :\n-MAP\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_nested_in_an_item_with_a_value(
    "- ?\n    ? a\n  : v\n",
    "+STR\n+DOC\n+SEQ\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::explicit_key_nested_in_a_mapping_value(
    "x:\n  ?\n    ? a\n  : v\n",
    "+STR\n+DOC\n+MAP\n=VAL :x\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_after_an_implicit_entry_in_a_key_mapping(
    "?\n  a: 1\n  ? b\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :1\n=VAL :b\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_with_a_value_after_a_nested_explicit_key(
    "?\n  ? a\n  : b\n  ? c\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :b\n=VAL :c\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_under_an_implicit_key_in_a_key(
    "?\n  x:\n    ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :x\n+MAP\n=VAL :a\n=VAL :\n-MAP\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_in_a_sequence_under_an_implicit_key_in_a_key(
    "?\n  x:\n  - ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :x\n+SEQ\n+MAP\n=VAL :a\n=VAL :\n-MAP\n-SEQ\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::two_inline_nested_explicit_keys(
    "?\n  ? ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::two_nested_explicit_keys_over_lines(
    "?\n  ?\n    ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_an_empty_outer_value(
    "?\n  ? a\n:\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_at_the_end_of_the_input(
    "?\n  ? a\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_a_value_at_the_end_of_the_input(
    "?\n  ? a\n  : b\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :b\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_before_a_document_end(
    "?\n  ? a\n...\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC ...\n-STR\n"
)]
#[case::nested_explicit_key_before_a_document_start(
    "?\n  ? a\n---\nb\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC\n+DOC ---\n=VAL :b\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_before_an_explicit_key(
    "?\n  ? a\n? b\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n=VAL :b\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_before_a_plain_key(
    "?\n  ? a\nb: 1\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n=VAL :b\n=VAL :1\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_before_an_alias_key(
    "?\n  ? a\n*x : 1\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n=ALI *x\n=VAL :1\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_before_a_flow_key(
    "?\n  ? a\n[x]: 1\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n+SEQ []\n=VAL :x\n-SEQ\n=VAL :1\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_before_an_anchored_empty_key(
    "?\n  ? a\n&k : v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n=VAL &k :\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_in_an_item_before_the_next_item(
    "- ?\n    ? a\n- b\n",
    "+STR\n+DOC\n+SEQ\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n=VAL :b\n-SEQ\n-DOC\n-STR\n"
)]
#[case::explicit_key_then_a_key_that_holds_a_mapping(
    "? a\n?\n  b: 1\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :\n+MAP\n=VAL :b\n=VAL :1\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_then_a_compact_key_mapping(
    "? a\n? b: 1\n: v\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :\n+MAP\n=VAL :b\n=VAL :1\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::entry_then_an_explicit_key_that_holds_a_mapping(
    "a: 1\n?\n  b: 2\n: v\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :1\n+MAP\n=VAL :b\n=VAL :2\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_entry_then_a_compact_key_mapping(
    "? a\n: b\n? c: d\n: e\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :b\n+MAP\n=VAL :c\n=VAL :d\n-MAP\n=VAL :e\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_then_a_key_that_holds_an_explicit_key(
    "? a\n?\n  ? b\n: v\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :\n+MAP\n=VAL :b\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::entry_then_a_compact_key_mapping(
    "a: 1\n? b: 2\n: c\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :1\n+MAP\n=VAL :b\n=VAL :2\n-MAP\n=VAL :c\n-MAP\n-DOC\n-STR\n"
)]
#[case::three_inline_nested_explicit_keys_with_an_empty_value(
    "? ? ? ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::three_nested_empty_explicit_keys(
    "?\n  ?\n  ?\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :\n=VAL :\n=VAL :\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::four_nested_explicit_keys(
    "?\n  ?\n    ?\n      ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_then_a_second_one(
    "? ? a\n  ? b\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n=VAL :b\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_then_an_outer_key(
    "? ? a\n? b\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n=VAL :b\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_then_a_key_with_two_nested_ones(
    "? a\n?\n  ? b\n  ? c\n: v\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :\n+MAP\n=VAL :b\n=VAL :\n=VAL :c\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::entry_then_a_key_mapping_with_two_entries(
    "a: 1\n? b: 2\n  c: 3\n: v\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :1\n+MAP\n=VAL :b\n=VAL :2\n=VAL :c\n=VAL :3\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::item_mapping_then_an_explicit_key_that_holds_a_mapping(
    "- a: 1\n  ?\n    b: 2\n  : v\n",
    "+STR\n+DOC\n+SEQ\n+MAP\n=VAL :a\n=VAL :1\n+MAP\n=VAL :b\n=VAL :2\n-MAP\n=VAL :v\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::item_then_an_item_with_an_explicit_key_that_holds_a_mapping(
    "- x\n- ?\n    a: 1\n  : v\n",
    "+STR\n+DOC\n+SEQ\n=VAL :x\n+MAP\n+MAP\n=VAL :a\n=VAL :1\n-MAP\n=VAL :v\n-MAP\n-SEQ\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_that_holds_an_empty_entry(
    "? ?\n  : v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :\n=VAL :v\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_a_literal_scalar(
    "?\n  ? |\n    lit\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL |lit\\n\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::inline_nested_explicit_key_with_a_literal_scalar(
    "? ? |\n    x\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL |x\\n\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_a_multiline_scalar(
    "?\n  ? a\n    b\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a b\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_a_quoted_scalar(
    "?\n  ? \"a\"\n: 'v'\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL \"a\n=VAL :\n-MAP\n=VAL 'v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_a_flow_mapping(
    "?\n  ? {a: b}\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n+MAP {}\n=VAL :a\n=VAL :b\n-MAP\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::inline_nested_explicit_key_with_a_flow_sequence(
    "? ? [a, b]\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n+SEQ []\n=VAL :a\n=VAL :b\n-SEQ\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_on_the_outer_explicit_key_of_a_nested_one(
    "? !!map\n  ? a\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP <tag:yaml.org,2002:map>\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_on_the_scalar_of_a_nested_explicit_key(
    "?\n  ? !!str x\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL <tag:yaml.org,2002:str> :x\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_on_the_outer_key_and_an_alias_value(
    "? &m\n  ? a\n: *m\n",
    "+STR\n+DOC\n+MAP\n+MAP &m\n=VAL :a\n=VAL :\n-MAP\n=ALI *m\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_then_a_sequence_under_an_implicit_key(
    "?\n  x:\n  - a\n  ? b\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :x\n+SEQ\n=VAL :a\n-SEQ\n=VAL :b\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_a_sequence_value(
    "?\n  ? a\n:\n  - x\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n+SEQ\n=VAL :x\n-SEQ\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_a_mapping_value(
    "?\n  ? a\n:\n  k: x\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n+MAP\n=VAL :k\n=VAL :x\n-MAP\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_then_another_entry(
    "?\n  ? a\n: v\n? b\n: c\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n=VAL :b\n=VAL :c\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_then_a_plain_entry(
    "?\n  ? a\n: v\nz: 1\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n=VAL :z\n=VAL :1\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_an_empty_value_then_another_entry(
    "?\n  ? a\n:\n? b\n: c\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n=VAL :b\n=VAL :c\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_keys_in_two_documents(
    "---\n?\n  ? a\n---\n?\n  ? b\n...\n",
    "+STR\n+DOC ---\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC\n+DOC ---\n+MAP\n+MAP\n=VAL :b\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC ...\n-STR\n"
)]
#[case::nested_explicit_key_with_an_empty_value_before_a_document_end(
    "?\n  ? a\n:\n...\n?\n  ? b\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC ...\n+DOC\n+MAP\n+MAP\n=VAL :b\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_without_a_line_break(
    "?\n  ? a",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::inline_nested_explicit_key_without_a_line_break(
    "? ? a",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_empty_explicit_key_without_a_line_break(
    "?\n  ?",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::inline_nested_empty_explicit_key_without_a_line_break(
    "? ?",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_a_bare_colon_at_the_end(
    "? ? a\n:",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_a_value_at_the_end(
    "? ? a\n: v",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_on_the_root_before_a_nested_explicit_key(
    "!!map\n? ? a\n: v\n",
    "+STR\n+DOC\n+MAP <tag:yaml.org,2002:map>\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::anchor_on_the_root_before_a_nested_explicit_key(
    "&r\n?\n  ? a\n: v\n",
    "+STR\n+DOC\n+MAP &r\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::tag_after_the_marker_before_a_nested_explicit_key(
    "--- !!map\n?\n  ? a\n: v\n",
    "+STR\n+DOC ---\n+MAP <tag:yaml.org,2002:map>\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_crlf(
    "?\r\n  ? a\r\n: v\r\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::nested_explicit_key_with_a_tab_before_the_value(
    "?\n  ? a\n:\tv\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_as_the_value_of_an_explicit_key(
    "? a\n: ? b\n  : c\n? d\n: e\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n+MAP\n=VAL :b\n=VAL :c\n-MAP\n=VAL :d\n=VAL :e\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_as_the_value_in_an_item(
    "- ? a\n  : ? b\n- c\n",
    "+STR\n+DOC\n+SEQ\n+MAP\n=VAL :a\n+MAP\n=VAL :b\n=VAL :\n-MAP\n-MAP\n=VAL :c\n-SEQ\n-DOC\n-STR\n"
)]
#[case::sequence_with_an_explicit_key_as_the_value(
    "? a\n: - ? b\n    : c\n? d\n: e\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n+SEQ\n+MAP\n=VAL :b\n=VAL :c\n-MAP\n-SEQ\n=VAL :d\n=VAL :e\n-MAP\n-DOC\n-STR\n"
)]
#[case::bare_explicit_key_then_a_colon_line(
    "?\n: v\n",
    "+STR\n+DOC\n+MAP\n=VAL :\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::bare_explicit_key_with_a_sequence_key(
    "?\n- a\n: v\n",
    "+STR\n+DOC\n+MAP\n+SEQ\n=VAL :a\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_with_a_compact_empty_key(
    "? : v",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :\n=VAL :v\n-MAP\n=VAL :\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_with_a_block_mapping(
    "?\n  a: 1\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=VAL :a\n=VAL :1\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_with_a_block_sequence(
    "?\n  - a\n: v\n",
    "+STR\n+DOC\n+MAP\n+SEQ\n=VAL :a\n-SEQ\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_with_an_alias_key_entry(
    "?\n  *x :\n: v\n",
    "+STR\n+DOC\n+MAP\n+MAP\n=ALI *x\n=VAL :\n-MAP\n=VAL :v\n-MAP\n-DOC\n-STR\n"
)]
#[case::explicit_key_then_an_alias_key(
    "? a\n? b\n*y : 3\n",
    "+STR\n+DOC\n+MAP\n=VAL :a\n=VAL :\n=VAL :b\n=VAL :\n=ALI *y\n=VAL :3\n-MAP\n-DOC\n-STR\n"
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
// ---- Task 9 ------------------------------------------------------
#[case::flow_plain_comment_then_an_anchor_line("[a\n # c\n &x b]\n")]
#[case::flow_plain_comment_then_an_anchor("[a #c\n &x b]\n")]
#[case::flow_plain_anchor_line_comment_then_text("[a\n &x b # c\n d]\n")]
#[case::flow_plain_then_an_anchored_sequence("[a\n &x [b]]\n")]
#[case::flow_plain_then_an_aliased_sequence("[a\n *x [b]]\n")]
#[case::flow_plain_then_an_anchored_mapping("[a\n &x {b}]\n")]
#[case::flow_plain_then_an_anchor_line_and_a_sequence("[a\n &x\n[b]]\n")]
#[case::flow_quote_then_an_anchored_line("[\"a\"\n &x b]\n")]
#[case::flow_alias_then_a_quoted_line("[*x\n \"b\"]\n")]
#[case::flow_sequence_then_a_quoted_line("[[a]\n \"b\"]\n")]
#[case::flow_nested_continued_sequence_then_an_anchored_line("[[a\n b]\n &x c]\n")]
#[case::flow_mapping_then_an_anchored_line("[{a: b}\n &x c]\n")]
#[case::flow_mapping_quoted_value_then_an_anchored_line("{a: \"b\"\n &x c}\n")]
#[case::flow_mapping_value_then_an_anchored_key("{a: b\n &x c: d}\n")]
#[case::flow_mapping_value_then_a_quoted_key("{a: b\n\"c\": d}\n")]
#[case::flow_mapping_value_then_a_single_quoted_key("{a: b\n'c': d}\n")]
#[case::flow_mapping_value_then_an_alias_key("{a: b\n*x : d}\n")]
#[case::flow_mapping_key_anchor_line_colon_then_a_second_key("{a\n &zz : v\n b: 1}\n")]
#[case::flow_plain_then_a_colon_and_a_space("[a\n : b]\n")]
// ---- Task 12 ------------------------------------------------------
// The verdict "libfyaml reports an error here as well"; `error_reporting.rs`
// holds the error line of the same inputs.
#[case::sequence_entry_after_an_empty_explicit_key_in_a_value("x:\n  ?\n- a\n")]
#[case::flow_sequence_after_an_empty_explicit_key_in_a_value("x:\n  ?\n[a]\n")]
#[case::sequence_entry_after_a_nested_explicit_key("?\n  ? a\n- b\n")]
#[case::sequence_entry_after_a_nested_explicit_key_with_a_colon("?\n  ? a\n  :\n- b\n")]
#[case::sequence_entry_after_a_nested_empty_explicit_key("?\n  ?\n- a\n")]
#[case::colon_between_the_columns_of_nested_explicit_keys("?\n  ? a\n : v\n")]
#[case::sequence_entry_after_a_plain_explicit_key("? x\n- a\n")]
#[case::sequence_entry_after_an_explicit_value("? a\n: b\n- c\n")]
#[case::sequence_entry_after_a_complete_explicit_key_in_an_item("- ?\n- a\n: v\n")]
#[case::tab_after_an_inline_explicit_key("?\t? a\n: v\n")]
#[case::explicit_key_on_the_line_of_a_value("k: ?\n    ? b\n  : c\n")]
#[case::explicit_key_as_a_value_with_a_deeper_colon("?\n  ? a\n: ? b\n    : c\n")]
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
