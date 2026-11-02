// SPDX-License-Identifier: MIT
//
// Every comment is an `Event::Comment` (YAML 1.2.2 [75] `c-nb-comment-text`,
// [77] `s-b-comment`, [78] `l-comment`): a `#` at a line start or after
// whitespace that is not part of a scalar starts the span of a comment event.
// Checked on every yaml-test-suite input that parses without error.

use std::path::PathBuf;
use std::time::Duration;

use rlsp_yaml_parser::{Event, ScalarStyle, parse_events};
use rstest::rstest;

use super::load_cases_from_file;

/// Byte offsets of the `#` that begin a comment unless a scalar holds them.
fn comment_candidates(yaml: &str) -> Vec<usize> {
    let bytes = yaml.as_bytes();
    bytes
        .iter()
        .enumerate()
        .filter(|&(i, &b)| {
            b == b'#' && (i == 0 || matches!(bytes.get(i - 1), Some(b' ' | b'\t' | b'\n' | b'\r')))
        })
        .map(|(i, _)| i)
        .collect()
}

/// The byte range of a scalar that can hold a `#` as content: its whole span,
/// except for a block scalar, whose header line (`| # c`) is not content.
fn scalar_content_range(
    yaml: &str,
    start: usize,
    end: usize,
    style: ScalarStyle,
) -> (usize, usize) {
    if !matches!(style, ScalarStyle::Literal(_) | ScalarStyle::Folded(_)) {
        return (start, end);
    }
    let header_end = yaml
        .get(start..end)
        .and_then(|span| span.find('\n'))
        .map_or(end, |newline| start + newline + 1);
    (header_end, end)
}

/// What is wrong with the comment events of a parse of `yaml`: a `#` that
/// starts a comment (and is in no scalar's content) without a `Comment` event,
/// a `Comment` event that does not start at such a `#`, and `Comment` events
/// whose starts are not strictly ascending.  Also returns the number of `#`
/// candidates that were checked.
fn comment_event_violations(
    yaml: &str,
    scalar_content: &[(usize, usize)],
    comments: &[(usize, usize)],
) -> (Vec<String>, usize) {
    let candidates = comment_candidates(yaml);
    let mut violations = Vec::new();
    for &offset in &candidates {
        let in_scalar = scalar_content
            .iter()
            .any(|&(start, end)| start <= offset && offset < end);
        let in_comment = comments
            .iter()
            .any(|&(start, end)| start <= offset && offset < end);
        if !in_scalar && !in_comment {
            violations.push(format!("`#` at byte {offset} has no Comment event"));
        }
    }
    for &(start, end) in comments {
        if !candidates.contains(&start) || end <= start {
            violations.push(format!(
                "Comment span {start}..{end} does not start at a comment `#`"
            ));
        }
    }
    for pair in comments.windows(2) {
        if let [(a, _), (b, _)] = pair
            && a >= b
        {
            violations.push(format!(
                "Comment starts {a} and {b} are not strictly ascending"
            ));
        }
    }
    (violations, candidates.len())
}

/// `None` when `yaml` does not parse; otherwise the violations and the number
/// of candidates checked.
fn check_comment_events(yaml: &str) -> Option<(Vec<String>, usize)> {
    let events = parse_events(yaml).collect::<Result<Vec<_>, _>>().ok()?;
    let mut scalar_content = Vec::new();
    let mut comments = Vec::new();
    for (event, span) in &events {
        let (start, end) = (span.start as usize, span.end as usize);
        match event {
            Event::Scalar { style, .. } => {
                scalar_content.push(scalar_content_range(yaml, start, end, *style));
            }
            Event::Comment { .. } => comments.push((start, end)),
            Event::StreamStart
            | Event::StreamEnd
            | Event::Alias { .. }
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::SequenceStart { .. }
            | Event::SequenceEnd
            | Event::MappingStart { .. }
            | Event::MappingEnd => {}
        }
    }
    Some(comment_event_violations(yaml, &scalar_content, &comments))
}

fn assert_every_comment_has_an_event(yaml: &str, tag: &str) -> usize {
    let Some((violations, checked)) = check_comment_events(yaml) else {
        return 0;
    };
    assert!(violations.is_empty(), "{violations:?}: {tag}");
    checked
}

#[rstest]
#[timeout(Duration::from_secs(5))]
pub fn every_comment_in_suite_has_an_event(
    #[files("../tests/yaml-test-suite/src/*.yaml")] path: PathBuf,
) {
    for case in &load_cases_from_file(&path) {
        let tag = format!("{}[{}] {}", case.file, case.index, case.name);
        assert_every_comment_has_an_event(&case.yaml, &tag);
    }
}

// The sweep must not pass by checking nothing: it looks at many `#`.
#[test]
fn comment_sweep_checks_many_candidates_in_the_suite() {
    let dir = std::path::Path::new("../tests/yaml-test-suite/src");
    let mut checked = 0;
    let mut files = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        for case in &load_cases_from_file(&path) {
            files += 1;
            checked += assert_every_comment_has_an_event(&case.yaml, &case.name);
        }
    }
    assert!(files > 300, "suite directory has {files} cases");
    assert!(checked > 100, "only {checked} candidates were checked");
}

// Inputs for every position a comment can follow; the suite has none for most.
#[rstest]
#[case::double_quoted_value("a: \"q\" # c\n")]
#[case::single_quoted_value("a: 'q' # c\n")]
#[case::quoted_sequence_item("- \"q\" # c\n")]
#[case::key_without_value("k: # c\n")]
#[case::bare_dash("- # c\n  a: 1\n")]
#[case::bare_question_mark("? # c\n: v\n")]
#[case::bare_colon("? k\n: # c\n  v\n")]
#[case::anchor("a: &x # c\n  b: 1\n")]
#[case::tag("a: !t # c\n  b: 1\n")]
#[case::literal_header("a: | # c\n  t # x\n")]
#[case::folded_header("a: > # c\n  t # x\n  # y\n")]
#[case::multi_line_plain("a:\n  v\n  w # c\n")]
#[case::document_start("--- v # c\n")]
#[case::yaml_directive("%YAML 1.2 # c\n---\na: 1\n")]
#[case::tag_directive("%TAG ! tag:x: # c\n---\na: 1\n")]
#[case::document_end("a: 1\n... # c\n")]
#[case::orphan_document_end("a: 1\n...\n... # c\n")]
#[case::root_quoted("\"q\" # c\n")]
#[case::root_literal_header("--- |  # c\n t\n")]
#[case::property_lines("--- &x # c\na: 1\n")]
#[case::explicit_key_with_properties("? &x # c\n  k\n: v\n")]
#[case::empty_value_before_next_key("k: # c\nb: 2\n")]
#[case::tab_before_hash("k:\t# c\n")]
#[case::multi_line_double_quoted("a: \"x\n  y\" # c\n")]
#[case::flow_in_block("a: [x, # c\n  y] # d\n")]
#[case::bare_dash_comment_below_before_value("-\n# c\n  b\n")]
#[case::bare_dash_comment_below_empty_item("-\n  # d\n")]
#[case::bare_dash_comment_before_next_item("-\n  # c\n- x\n")]
#[case::bare_dash_comment_crlf("-\r\n  # c\r\n")]
#[case::bare_dash_comment_at_end_without_line_break("-\n  # c")]
#[case::mixed("%YAML 1.2 # a\n---  # b\nk: &x # c\n  - \"q\" # d\n  - | # e\n    t\n... # f\n")]
fn every_comment_in_explicit_inputs_has_an_event(#[case] yaml: &str) {
    let checked = assert_every_comment_has_an_event(yaml, yaml);
    assert!(checked > 0, "input has no comment candidate: {yaml:?}");
}

// The checker itself reports what it should, so the sweep cannot pass vacuously.
#[rstest]
#[case::missing_event("a: \"q\" # c\n", &[], &[], 1)]
#[case::event_present("a: \"q\" # c\n", &[], &[(7, 10)], 0)]
#[case::hash_in_scalar_content("a: \"x # y\"\n", &[(3, 10)], &[], 0)]
#[case::hash_in_header_line_is_not_scalar_content("a: | # c\n  t\n", &[(9, 13)], &[], 1)]
#[case::hash_without_space_is_no_candidate("a: x#y\n", &[(3, 6)], &[], 0)]
#[case::event_not_at_a_hash("a: v\n", &[], &[(3, 4)], 1)]
#[case::comments_out_of_order("# a\n# b\n", &[], &[(4, 7), (0, 3)], 1)]
#[case::comment_twice("# a\n", &[], &[(0, 3), (0, 3)], 1)]
fn comment_check_reports_exactly_the_violations(
    #[case] yaml: &str,
    #[case] scalar_content: &[(usize, usize)],
    #[case] comments: &[(usize, usize)],
    #[case] expected: usize,
) {
    let (violations, _) = comment_event_violations(yaml, scalar_content, comments);
    assert_eq!(violations.len(), expected, "{violations:?}");
}

/// The events of `yaml` without spans and comments, as strings, with the spans
/// of the scalars and of the comments; `None` when `yaml` does not parse.
#[expect(clippy::type_complexity, reason = "one-off test helper")]
fn shape_and_spans(
    yaml: &str,
) -> Option<(
    Vec<String>,
    Vec<(usize, usize, ScalarStyle)>,
    Vec<(usize, usize)>,
)> {
    let mut shape = Vec::new();
    let mut scalars = Vec::new();
    let mut comments = Vec::new();
    for item in parse_events(yaml) {
        let (event, span) = item.ok()?;
        let (start, end) = (span.start as usize, span.end as usize);
        match &event {
            Event::Comment { .. } => comments.push((start, end)),
            Event::Scalar { value, style, .. } => {
                scalars.push((start, end, *style));
                shape.push(format!(
                    "{value:?} {style:?} {:?} {:?}",
                    event.anchor(),
                    event.tag()
                ));
            }
            Event::MappingStart { style, .. } => {
                shape.push(format!(
                    "map {style:?} {:?} {:?}",
                    event.anchor(),
                    event.tag()
                ));
            }
            Event::SequenceStart { style, .. } => {
                shape.push(format!(
                    "seq {style:?} {:?} {:?}",
                    event.anchor(),
                    event.tag()
                ));
            }
            Event::StreamStart
            | Event::StreamEnd
            | Event::Alias { .. }
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::SequenceEnd
            | Event::MappingEnd => shape.push(format!("{event:?}")),
        }
    }
    Some((shape, scalars, comments))
}

// A comment appended to the end of a line that no scalar continues past is
// one more `Comment` event and changes no other event, whichever node, indicator,
// property, marker or directive the line ends with.
fn assert_appended_comment_is_one_more_event(yaml: &str, tag: &str) {
    let Some((shape, scalars, comments)) = shape_and_spans(yaml) else {
        return;
    };
    let mut line_start = 0;
    for line in yaml.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let end = line_start + body.len();
        line_start += line.len();
        let ends_inside_scalar = scalars.iter().any(|&(start, scalar_end, style)| {
            let block = matches!(style, ScalarStyle::Literal(_) | ScalarStyle::Folded(_));
            start < end && (end < scalar_end || (block && end == scalar_end))
        });
        let in_comment = comments
            .iter()
            .any(|&(start, comment_end)| start < end && end <= comment_end);
        if body.trim().is_empty() || ends_inside_scalar || in_comment {
            continue;
        }
        let variant = format!("{} # zz{}", &yaml[..end], &yaml[end..]);
        let parsed = shape_and_spans(&variant);
        assert!(
            parsed.is_some(),
            "a comment after {body:?} breaks the parse: {tag}"
        );
        let (variant_shape, _, variant_comments) = parsed.unwrap_or_default();
        assert_eq!(
            variant_shape, shape,
            "a comment after {body:?} changes events: {tag}"
        );
        assert_eq!(
            variant_comments.len(),
            comments.len() + 1,
            "a comment after {body:?} is not one more Comment event: {tag}"
        );
        assert!(
            variant_comments.iter().any(|&(start, _)| start == end + 1),
            "no Comment event starts at the appended `#` after {body:?}: {tag}"
        );
    }
}

#[rstest]
#[timeout(Duration::from_secs(5))]
pub fn a_comment_appended_to_any_line_in_suite_is_one_more_event(
    #[files("../tests/yaml-test-suite/src/*.yaml")] path: PathBuf,
) {
    for case in &load_cases_from_file(&path) {
        let tag = format!("{}[{}] {}", case.file, case.index, case.name);
        assert_appended_comment_is_one_more_event(&case.yaml, &tag);
    }
}
