// SPDX-License-Identifier: MIT
//
// A plain scalar never contains a comment (YAML 1.2.2 [130], [134]): `#` is
// content only right after an `ns-char`, and a continuation line must start
// with an `ns-plain-char`.  Checked on every yaml-test-suite input
// through both APIs.  The suite has no input with an indented comment line
// after a plain scalar, so the same check also runs on such inputs below.

use std::path::PathBuf;
use std::time::Duration;

use rlsp_yaml_parser::{Event, Node, ScalarStyle, Span, load, parse_events};
use rstest::rstest;

use super::load_cases_from_file;

/// Returns the reason `value` cannot be a plain scalar, if there is one.
fn comment_in_plain_scalar(value: &str) -> Option<&'static str> {
    if value.starts_with('#') {
        return Some("starts with `#`");
    }
    value
        .as_bytes()
        .windows(2)
        .any(|w| matches!(w, [b' ' | b'\t' | b'\n', b'#']))
        .then_some("contains `#` after whitespace")
}

fn collect_plain_values<'a>(node: &'a Node<Span>, out: &mut Vec<&'a str>) {
    match node {
        Node::Scalar {
            value,
            style: ScalarStyle::Plain,
            ..
        } => out.push(value),
        Node::Scalar { .. } | Node::Alias { .. } => {}
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                collect_plain_values(key, out);
                collect_plain_values(value, out);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_plain_values(item, out);
            }
        }
    }
}

fn assert_no_comment_in_plain_scalars(yaml: &str, tag: &str) {
    for (event, _) in parse_events(yaml).filter_map(Result::ok) {
        if let Event::Scalar {
            value,
            style: ScalarStyle::Plain,
            ..
        } = event
        {
            let reason = comment_in_plain_scalar(&value);
            assert!(
                reason.is_none(),
                "parse_events: plain scalar {value:?} {reason:?}: {tag}"
            );
        }
    }

    let Ok(docs) = load(yaml) else { return };
    let mut values = Vec::new();
    for doc in &docs {
        collect_plain_values(&doc.root, &mut values);
    }
    for value in values {
        let reason = comment_in_plain_scalar(value);
        assert!(
            reason.is_none(),
            "load: plain scalar {value:?} {reason:?}: {tag}"
        );
    }
}

#[rstest]
#[timeout(Duration::from_secs(5))]
pub fn plain_scalars_in_suite_never_contain_a_comment(
    #[files("../tests/yaml-test-suite/src/*.yaml")] path: PathBuf,
) {
    for case in &load_cases_from_file(&path) {
        let tag = format!("{}[{}] {}", case.file, case.index, case.name);
        assert_no_comment_in_plain_scalars(&case.yaml, &tag);
    }
}

#[rstest]
#[case::mapping_value("a: v\n  # d\nb: 1\n")]
#[case::continuation("a:\n  v\n  w\n  # d\nb: 1\n")]
#[case::blank_line("a: v\n\n  # d\nb: 1\n")]
#[case::tab("a: v\n\t# d\nb: 1\n")]
#[case::sequence_item("- v\n  # d\n- z\n")]
#[case::root_scalar("v\n# d\n")]
#[case::port("port: 8080\n  # default\n")]
#[case::explicit_key("? k\n  # c\n: v\n")]
fn plain_scalars_before_a_comment_line_never_contain_it(#[case] yaml: &str) {
    assert_no_comment_in_plain_scalars(yaml, yaml);
}

#[rstest]
#[case::space_hash("v # d", true)]
#[case::leading_hash("#x", true)]
#[case::tab_hash("v\t#d", true)]
#[case::folded_line_hash("v\n#d", true)]
#[case::hash_inside_word("a#b", false)]
#[case::trailing_hash("x#", false)]
#[case::hash_after_char("v#d", false)]
fn comment_check_flags_exactly_the_values_a_plain_scalar_cannot_hold(
    #[case] value: &str,
    #[case] flagged: bool,
) {
    assert_eq!(comment_in_plain_scalar(value).is_some(), flagged);
}
