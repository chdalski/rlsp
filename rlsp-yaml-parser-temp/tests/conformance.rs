// SPDX-License-Identifier: MIT
//
// Conformance test suite using yaml-test-suite data symlinked at
// `tests/yaml-test-suite/src/` (commit recorded in `src/.commit`).
//
// Each `.yaml` file in that directory contains one or more test cases.
// For each case:
//   - If `fail: true` — the YAML is intentionally invalid.  We verify that
//     `parse_events` produces at least one `Err` item.
//   - Otherwise — we verify that `parse_events` produces no `Err` items
//     (the entire event stream is successfully parsed).
//
// rstest `#[files]` generates one independent test per matched file,
// giving per-file pass/fail visibility in test output.

#![allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stderr
)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use rlsp_yaml_parser_temp::parse_events;
use rstest::rstest;

// ---- Test-case data model ---------------------------------------------------

#[derive(Debug)]
struct ConformanceCase {
    file: String,
    index: usize,
    name: String,
    yaml: String,
    fail: bool,
}

// ---- Visual-representation helpers (from yaml-test-suite convention) --------

/// Convert the yaml-test-suite "visual" representation to real YAML text.
///
/// The test suite uses Unicode markers to represent whitespace characters that
/// would be invisible in plain text:
///   `␣` → space, `»` → tab, `←` → CR, `⇔` → BOM, `↵` → (nothing), `∎\n` → (nothing)
fn visual_to_raw(s: &str) -> String {
    s.replace('␣', " ")
        .replace('»', "\t")
        .replace('—', "")
        .replace('←', "\r")
        .replace('\u{21D4}', "\u{FEFF}")
        .replace('↵', "")
        .replace("∎\n", "")
}

// ---- Loading test cases from the vendored YAML files ------------------------

/// Accumulated fields for one metadata entry. Fields not set in the current
/// entry are inherited from the previous one (except `fail`, which resets).
#[derive(Default)]
struct EntryFields {
    name: Option<String>,
    yaml: Option<String>,
    fail: Option<bool>,
    skip: bool,
}

impl EntryFields {
    /// Apply a parsed `key: value` pair (inline scalar).
    fn set(&mut self, key: &str, value: &str) {
        match key {
            "name" => self.name = Some(value.to_string()),
            "yaml" => self.yaml = Some(value.to_string()),
            "fail" => self.fail = Some(value == "true"),
            "skip" => self.skip = true,
            _ => {}
        }
    }

    /// Apply a completed block scalar for the given key.
    fn set_block(&mut self, key: &str, block: String) {
        match key {
            "name" => self.name = Some(block),
            "yaml" => self.yaml = Some(block),
            "fail" => self.fail = Some(block.trim() == "true"),
            "skip" => self.skip = true,
            _ => {}
        }
    }
}

/// Parse the yaml-test-suite metadata format without a YAML library.
///
/// Returns `(name, yaml, fail)` tuples. Entries with `skip` are omitted.
fn parse_test_metadata(content: &str) -> Vec<(String, String, bool)> {
    let mut results: Vec<(String, String, bool)> = Vec::new();
    let mut inherited = EntryFields::default();
    let mut current = EntryFields::default();
    let mut block_key: Option<String> = None;
    let mut block_buf: Option<String> = None;
    let mut in_entry = false;

    let flush_block = |current: &mut EntryFields,
                       block_key: &mut Option<String>,
                       block_buf: &mut Option<String>| {
        if let (Some(k), Some(b)) = (block_key.take(), block_buf.take()) {
            current.set_block(&k, b);
        }
    };

    let flush_entry =
        |current: &mut EntryFields, inherited: &mut EntryFields, results: &mut Vec<_>| {
            // Merge current into inherited
            if let Some(n) = current.name.take() {
                inherited.name = Some(n);
            }
            if let Some(y) = current.yaml.take() {
                inherited.yaml = Some(y);
            }
            if current.skip {
                inherited.skip = true;
            }
            let fail = current.fail.take().unwrap_or(false);
            if !inherited.skip {
                if let Some(ref yaml) = inherited.yaml {
                    let name = inherited.name.clone().unwrap_or_default();
                    results.push((name, yaml.clone(), fail));
                }
            }
        };

    let parse_field = |line: &str,
                       current: &mut EntryFields,
                       block_key: &mut Option<String>,
                       block_buf: &mut Option<String>| {
        if let Some((key, value)) = line.split_once(": ") {
            let key = key.trim();
            let value = value.trim();
            if value == "|" {
                *block_key = Some(key.to_string());
                *block_buf = Some(String::new());
            } else {
                current.set(key, value);
            }
        }
    };

    for line in content.lines() {
        if line == "---" {
            continue;
        }

        if let Some(rest) = line.strip_prefix("- ") {
            flush_block(&mut current, &mut block_key, &mut block_buf);
            if in_entry {
                flush_entry(&mut current, &mut inherited, &mut results);
            }
            in_entry = true;
            current = EntryFields::default();
            parse_field(rest, &mut current, &mut block_key, &mut block_buf);
        } else if let Some(indented) = block_buf
            .is_some()
            .then(|| line.strip_prefix("    "))
            .flatten()
        {
            block_buf.as_mut().unwrap().push_str(indented);
            block_buf.as_mut().unwrap().push('\n');
        } else if line.starts_with("  ") && !line.starts_with("    ") {
            flush_block(&mut current, &mut block_key, &mut block_buf);
            parse_field(line.trim(), &mut current, &mut block_key, &mut block_buf);
        }
    }

    flush_block(&mut current, &mut block_key, &mut block_buf);
    if in_entry {
        flush_entry(&mut current, &mut inherited, &mut results);
    }

    results
}

fn load_cases_from_file(path: &Path) -> Vec<ConformanceCase> {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    parse_test_metadata(&content)
        .into_iter()
        .enumerate()
        .map(|(idx, (name, yaml, fail))| {
            let name = if name.is_empty() {
                format!("{file_name}-{idx:02}")
            } else {
                name
            };
            ConformanceCase {
                file: file_name.clone(),
                index: idx,
                name,
                yaml: visual_to_raw(&yaml),
                fail,
            }
        })
        .collect()
}

// ---- Helpers ----------------------------------------------------------------

/// Returns `true` if `parse_events` produces at least one `Err` for `input`.
fn has_parse_error(input: &str) -> bool {
    parse_events(input).any(|r| r.is_err())
}

/// Returns `true` if `parse_events` produces no `Err` for `input`.
fn parses_clean(input: &str) -> bool {
    parse_events(input).all(|r| r.is_ok())
}

// ---- Parameterized conformance test -----------------------------------------

#[rstest]
#[timeout(Duration::from_secs(5))]
fn yaml_test_suite(#[files("tests/yaml-test-suite/src/*.yaml")] path: PathBuf) {
    let cases = load_cases_from_file(&path);
    if cases.is_empty() {
        // All entries are skipped (e.g., ZYU8). Nothing to test.
        return;
    }

    for case in &cases {
        let tag = format!("{}[{}] {}", case.file, case.index, case.name);

        if case.fail {
            assert!(
                has_parse_error(&case.yaml),
                "expected parse error but got clean parse: {tag}\n  yaml: {:?}",
                &case.yaml[..case.yaml.len().min(120)]
            );
        } else {
            let first_err = parse_events(&case.yaml)
                .find_map(std::result::Result::err)
                .map(|e| e.to_string());
            assert!(
                parses_clean(&case.yaml),
                "unexpected parse error: {tag}\n  error: {}\n  yaml: {:?}",
                first_err.unwrap_or_default(),
                &case.yaml[..case.yaml.len().min(120)]
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Custom: anchor/tag before mapping key — indent tracking bugfix
//
// When an anchor or tag appears inline before a mapping key, the parser
// prepends the key content as a synthetic line at the property's column.
// The mapping must be opened at the PHYSICAL line's indent (the property's
// column), not the synthetic line's offset column.
//
// Per YAML test suite 9KAX: inline property (same line as key) annotates
// the KEY SCALAR, not the mapping.  Standalone property (own line) annotates
// the collection node that follows.
// ---------------------------------------------------------------------------

// Test 1: anchor before mapping key at root level.
// `&anchor key: value` — anchor is inline before the key → annotates key scalar.
// Expected: Mapping{no anchor, entries:[(Scalar{anchor:"anchor",value:"key"} → "value")]}
#[test]
fn anchor_before_mapping_key_root_level() {
    use rlsp_yaml_parser_temp::loader::load;
    use rlsp_yaml_parser_temp::node::Node;

    let docs = load("&anchor key: value\n").expect("load failed");
    assert_eq!(docs.len(), 1, "expected 1 document");
    let Node::Mapping {
        entries, anchor, ..
    } = &docs[0].root
    else {
        panic!("expected Mapping root, got: {:?}", docs[0].root);
    };
    assert!(
        anchor.is_none(),
        "mapping must have no anchor (anchor is on key scalar), got: {anchor:?}"
    );
    assert_eq!(entries.len(), 1, "expected 1 entry, got: {}", entries.len());
    let (k, v) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, anchor: Some(a), .. } if value == "key" && a == "anchor"),
        "key scalar must carry anchor 'anchor', got: {k:?}"
    );
    assert!(
        matches!(v, Node::Scalar { value, .. } if value == "value"),
        "val: {v:?}"
    );
}

// Test 2: anchor before mapping key at indented level.
// `outer:\n  &anchor inner_key: inner_value`
// Expected: root Mapping → "outer" → Mapping{no anchor, entries:[(Scalar{anchor:"anchor",value:"inner_key"} → "inner_value")]}
#[test]
fn anchor_before_mapping_key_indented() {
    use rlsp_yaml_parser_temp::loader::load;
    use rlsp_yaml_parser_temp::node::Node;

    let docs = load("outer:\n  &anchor inner_key: inner_value\n").expect("load failed");
    assert_eq!(docs.len(), 1, "expected 1 document");
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping root");
    };
    assert_eq!(entries.len(), 1, "root should have 1 entry");
    let (k, v) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, .. } if value == "outer"),
        "key: {k:?}"
    );
    let Node::Mapping {
        entries: inner_entries,
        anchor,
        ..
    } = v
    else {
        panic!("expected inner Mapping, got: {v:?}");
    };
    assert!(
        anchor.is_none(),
        "inner mapping must have no anchor (anchor is on key scalar), got: {anchor:?}"
    );
    assert_eq!(inner_entries.len(), 1, "inner mapping should have 1 entry");
    let (ik, iv) = &inner_entries[0];
    assert!(
        matches!(ik, Node::Scalar { value, anchor: Some(a), .. } if value == "inner_key" && a == "anchor"),
        "inner key scalar must carry anchor 'anchor', got: {ik:?}"
    );
    assert!(
        matches!(iv, Node::Scalar { value, .. } if value == "inner_value"),
        "inner val: {iv:?}"
    );
}

// Test 3: tag before mapping key at root level.
// `!!str key: value` — tag is inline before the key → annotates key scalar.
// Expected: Mapping{no tag, entries:[(Scalar{tag:Some("str"),value:"key"} → "value")]}
#[test]
fn tag_before_mapping_key_root_level() {
    use rlsp_yaml_parser_temp::loader::load;
    use rlsp_yaml_parser_temp::node::Node;

    let docs = load("!!str key: value\n").expect("load failed");
    assert_eq!(docs.len(), 1, "expected 1 document");
    let Node::Mapping { entries, tag, .. } = &docs[0].root else {
        panic!("expected Mapping root, got: {:?}", docs[0].root);
    };
    assert!(
        tag.is_none(),
        "mapping must have no tag (tag is on key scalar), got: {tag:?}"
    );
    assert_eq!(entries.len(), 1, "expected 1 entry, got: {}", entries.len());
    let (k, v) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, tag: Some(t), .. } if value == "key" && t.contains("str")),
        "key scalar must carry the tag, got: {k:?}"
    );
    assert!(
        matches!(v, Node::Scalar { value, .. } if value == "value"),
        "val: {v:?}"
    );
}

// Test 4: anchor + tag together before mapping key.
// `&a !!str key: value` — both inline before the key → annotate key scalar.
// Expected: Mapping{no anchor, no tag, entries:[(Scalar{anchor:"a",tag:Some,value:"key"} → "value")]}
#[test]
fn anchor_and_tag_before_mapping_key() {
    use rlsp_yaml_parser_temp::loader::load;
    use rlsp_yaml_parser_temp::node::Node;

    let docs = load("&a !!str key: value\n").expect("load failed");
    assert_eq!(docs.len(), 1, "expected 1 document");
    let Node::Mapping {
        entries,
        anchor,
        tag,
        ..
    } = &docs[0].root
    else {
        panic!("expected Mapping root, got: {:?}", docs[0].root);
    };
    assert!(
        anchor.is_none(),
        "mapping must have no anchor (anchor is on key scalar), got: {anchor:?}"
    );
    assert!(
        tag.is_none(),
        "mapping must have no tag (tag is on key scalar), got: {tag:?}"
    );
    assert_eq!(entries.len(), 1, "expected 1 entry, got: {}", entries.len());
    let (k, v) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, anchor: Some(a), tag: Some(t), .. }
            if value == "key" && a == "a" && t.contains("str")),
        "key scalar must carry anchor 'a' and tag, got: {k:?}"
    );
    assert!(
        matches!(v, Node::Scalar { value, .. } if value == "value"),
        "val: {v:?}"
    );
}

// ---------------------------------------------------------------------------
// Bug regression: double/single-quoted implicit block mapping keys must be decoded
// ---------------------------------------------------------------------------

#[test]
fn quoted_key_parse_events_style_double() {
    use rlsp_yaml_parser_temp::{ScalarStyle, parse_events};

    let events: Vec<_> = parse_events("\"key\": value\n").collect();
    let key_event = events.iter().find_map(|r| {
        let (ev, _) = r.as_ref().expect("event error");
        if let rlsp_yaml_parser_temp::Event::Scalar { value, style, .. } = ev {
            if value == "key" {
                return Some(*style);
            }
        }
        None
    });
    assert_eq!(
        key_event,
        Some(ScalarStyle::DoubleQuoted),
        "key scalar must have DoubleQuoted style at event layer"
    );
}

#[test]
fn quoted_key_parse_events_style_single() {
    use rlsp_yaml_parser_temp::{ScalarStyle, parse_events};

    let events: Vec<_> = parse_events("'key': value\n").collect();
    let key_event = events.iter().find_map(|r| {
        let (ev, _) = r.as_ref().expect("event error");
        if let rlsp_yaml_parser_temp::Event::Scalar { value, style, .. } = ev {
            if value == "key" {
                return Some(*style);
            }
        }
        None
    });
    assert_eq!(
        key_event,
        Some(ScalarStyle::SingleQuoted),
        "key scalar must have SingleQuoted style at event layer"
    );
}

#[test]
fn quoted_key_double_quoted_simple() {
    use rlsp_yaml_parser_temp::{ScalarStyle, loader::load, node::Node};

    let docs = load("\"key\": value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, v) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value == "key" && *style == ScalarStyle::DoubleQuoted),
        "key must be decoded with DoubleQuoted style, got: {k:?}"
    );
    assert!(
        matches!(v, Node::Scalar { value, .. } if value == "value"),
        "val: {v:?}"
    );
}

#[test]
fn quoted_key_single_quoted_simple() {
    use rlsp_yaml_parser_temp::{ScalarStyle, loader::load, node::Node};

    let docs = load("'key': value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, v) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value == "key" && *style == ScalarStyle::SingleQuoted),
        "key must be decoded with SingleQuoted style, got: {k:?}"
    );
    assert!(
        matches!(v, Node::Scalar { value, .. } if value == "value"),
        "val: {v:?}"
    );
}

#[test]
fn quoted_key_double_quoted_with_escape_sequence() {
    use rlsp_yaml_parser_temp::{ScalarStyle, loader::load, node::Node};

    // \t in double-quoted YAML is a literal tab character
    let docs = load("\"ke\\ty\": value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, _) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value == "ke\ty" && *style == ScalarStyle::DoubleQuoted),
        "key escape must be decoded and style DoubleQuoted, got: {k:?}"
    );
}

#[test]
fn quoted_key_single_quoted_with_escaped_quote() {
    use rlsp_yaml_parser_temp::{ScalarStyle, loader::load, node::Node};

    // In single-quoted scalars, '' is the escape for a literal '
    let docs = load("'it''s': value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, _) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value == "it's" && *style == ScalarStyle::SingleQuoted),
        "single-quoted key escape must be decoded and style SingleQuoted, got: {k:?}"
    );
}

#[test]
fn quoted_key_with_spaces_inside() {
    use rlsp_yaml_parser_temp::{ScalarStyle, loader::load, node::Node};

    let docs = load("\"hello world\": value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, _) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value == "hello world" && *style == ScalarStyle::DoubleQuoted),
        "spaces inside quoted key must be preserved, got: {k:?}"
    );
}

#[test]
fn quoted_key_double_quoted_empty() {
    use rlsp_yaml_parser_temp::{ScalarStyle, loader::load, node::Node};

    let docs = load("\"\": value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, _) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, style, .. }
            if value.is_empty() && *style == ScalarStyle::DoubleQuoted),
        "empty quoted key must decode to empty string with DoubleQuoted style, got: {k:?}"
    );
}

#[test]
fn quoted_key_in_nested_mapping() {
    use rlsp_yaml_parser_temp::{ScalarStyle, loader::load, node::Node};

    let docs = load("outer:\n  \"inner key\": inner value\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (k, v) = &entries[0];
    assert!(
        matches!(k, Node::Scalar { value, .. } if value == "outer"),
        "outer key: {k:?}"
    );
    let Node::Mapping { entries: inner, .. } = v else {
        panic!("expected nested Mapping, got: {v:?}");
    };
    assert_eq!(inner.len(), 1);
    let (ik, _) = &inner[0];
    assert!(
        matches!(ik, Node::Scalar { value, style, .. }
            if value == "inner key" && *style == ScalarStyle::DoubleQuoted),
        "nested quoted key must be decoded, got: {ik:?}"
    );
}

#[test]
fn quoted_key_multiple_entries_mixed() {
    use rlsp_yaml_parser_temp::{ScalarStyle, loader::load, node::Node};

    let docs = load("plain_key: 1\n\"quoted_key\": 2\n'another': 3\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 3);
    let (k0, _) = &entries[0];
    assert!(
        matches!(k0, Node::Scalar { value, style, .. }
            if value == "plain_key" && *style == ScalarStyle::Plain),
        "entry 0 key: {k0:?}"
    );
    let (k1, _) = &entries[1];
    assert!(
        matches!(k1, Node::Scalar { value, style, .. }
            if value == "quoted_key" && *style == ScalarStyle::DoubleQuoted),
        "entry 1 key: {k1:?}"
    );
    let (k2, _) = &entries[2];
    assert!(
        matches!(k2, Node::Scalar { value, style, .. }
            if value == "another" && *style == ScalarStyle::SingleQuoted),
        "entry 2 key: {k2:?}"
    );
}

// ---------------------------------------------------------------------------
// Bug regression: trailing comments must be attached to AST nodes
// ---------------------------------------------------------------------------

#[test]
fn trailing_comment_on_mapping_value_attached() {
    use rlsp_yaml_parser_temp::{loader::load, node::Node};

    let docs = load("key: value  # trailing\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 1);
    let (_, v) = &entries[0];
    assert_eq!(
        v.trailing_comment(),
        Some("# trailing"),
        "trailing comment must be attached to value node, got: {v:?}"
    );
}

#[test]
fn trailing_comment_on_sequence_item_attached() {
    use rlsp_yaml_parser_temp::{loader::load, node::Node};

    let docs = load("- item  # note\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Sequence { items, .. } = &docs[0].root else {
        panic!("expected Sequence, got: {:?}", docs[0].root);
    };
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].trailing_comment(),
        Some("# note"),
        "trailing comment must be attached to sequence item, got: {:?}",
        items[0]
    );
}

#[test]
fn leading_comment_attached_to_second_mapping_key() {
    use rlsp_yaml_parser_temp::{loader::load, node::Node};

    let docs = load("a: 1\n# before b\nb: 2\n").expect("load failed");
    assert_eq!(docs.len(), 1);
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected Mapping, got: {:?}", docs[0].root);
    };
    assert_eq!(entries.len(), 2);
    let (key_b, _) = &entries[1];
    assert_eq!(
        key_b.leading_comments(),
        &["# before b"],
        "leading comment must be attached to second key, got: {key_b:?}"
    );
}
