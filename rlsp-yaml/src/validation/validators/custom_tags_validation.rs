// SPDX-License-Identifier: MIT

use std::collections::HashMap;

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, Span};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};

use crate::lsp_util::span_to_lsp;

use super::custom_tag::{CustomTag, TagNodeType};

/// Validate custom YAML tags against an allowed set.
///
/// Returns warning diagnostics for any `!tag` found in the YAML documents that is not
/// listed in `allowed`. When `allowed` is empty, validation is skipped and an empty
/// vec is returned — no tags configured means no warnings.
///
/// When a tag name matches an entry that carries an `expected_type`, the node's actual
/// structure is compared against the expected type. A mismatch emits a `tagTypeMismatch`
/// diagnostic. Tags without a type annotation only emit `unknownTag` on non-match.
///
/// Diagnostic ranges use the tagged node's full `loc` span. Quoted scalars are never
/// flagged because the parser stores `tag: None` for them.
#[must_use]
pub fn validate_custom_tags(docs: &[Document<Span>], allowed: &[CustomTag]) -> Vec<Diagnostic> {
    if allowed.is_empty() {
        return Vec::new();
    }

    // Build a name-keyed lookup. First entry for a given name wins (modeline dedup
    // happens upstream in server.rs).
    let tag_map: HashMap<&str, &CustomTag> =
        allowed.iter().rev().map(|t| (t.name.as_str(), t)).collect();

    let mut diagnostics = Vec::new();

    for doc in docs {
        let idx = doc.line_index();
        collect_tag_diagnostics(&doc.root, &tag_map, &mut diagnostics, 0, idx);
    }

    diagnostics
}

/// Recursively walk a YAML node and emit diagnostics for unknown or mismatched tags.
fn collect_tag_diagnostics<'a>(
    node: &'a Node<Span>,
    tag_map: &HashMap<&str, &'a CustomTag>,
    diagnostics: &mut Vec<Diagnostic>,
    depth: usize,
    idx: &LineIndex,
) {
    const MAX_DEPTH: usize = 100;
    if depth > MAX_DEPTH {
        return;
    }

    // Check the tag and loc fields on this node (all non-Alias variants carry them).
    let tag_and_loc = match node {
        Node::Scalar { tag, loc, .. }
        | Node::Mapping { tag, loc, .. }
        | Node::Sequence { tag, loc, .. } => tag.as_deref().map(|t| (t, loc)),
        Node::Alias { .. } => None,
    };
    if let Some((tag_str, loc)) = tag_and_loc {
        // Skip tags injected by the Core schema resolver — these are never user-supplied.
        if tag_str.starts_with("tag:yaml.org,2002:") {
            // Fall through to child recursion only.
        } else if let Some(custom_tag) = tag_map.get(tag_str) {
            // Tag is in the allowed set — check type annotation if present.
            if let Some(expected) = custom_tag.expected_type {
                let actual_matches = match expected {
                    TagNodeType::Scalar => matches!(node, Node::Scalar { .. }),
                    TagNodeType::Mapping => matches!(node, Node::Mapping { .. }),
                    TagNodeType::Sequence => matches!(node, Node::Sequence { .. }),
                };
                if !actual_matches {
                    let expected_name = match expected {
                        TagNodeType::Scalar => "scalar",
                        TagNodeType::Mapping => "mapping",
                        TagNodeType::Sequence => "sequence",
                    };
                    let actual_name = match node {
                        Node::Scalar { .. } => "scalar",
                        Node::Mapping { .. } => "mapping",
                        Node::Sequence { .. } => "sequence",
                        Node::Alias { .. } => "alias",
                    };
                    let range = span_to_lsp(*loc, idx);
                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("tagTypeMismatch".to_string())),
                        message: format!(
                            "Tag {tag_str} expects a {expected_name} but got a {actual_name}"
                        ),
                        source: Some("rlsp-yaml".to_string()),
                        ..Diagnostic::default()
                    });
                }
            }
        } else {
            let range = span_to_lsp(*loc, idx);
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String("unknownTag".to_string())),
                message: format!("Unknown tag: {tag_str}"),
                source: Some("rlsp-yaml".to_string()),
                ..Diagnostic::default()
            });
        }
    }

    // Recurse into children.
    match node {
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                collect_tag_diagnostics(key, tag_map, diagnostics, depth + 1, idx);
                collect_tag_diagnostics(value, tag_map, diagnostics, depth + 1, idx);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_tag_diagnostics(item, tag_map, diagnostics, depth + 1, idx);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_utils::parse_docs;

    fn tags_no_type(names: &[&str]) -> Vec<CustomTag> {
        names
            .iter()
            .map(|&n| CustomTag {
                name: n.to_string(),
                expected_type: None,
            })
            .collect()
    }

    // ---- Custom Tags Validator: Happy Paths / Multi-document / Nested ----

    #[rstest]
    #[case::allowed_tag_no_diagnostic("value: !include foo.yaml\n", &["!include"] as &[&str])]
    #[case::empty_allowed_skips_validation("value: !include foo.yaml\n", &[])]
    #[case::no_tags_in_document("key: value\nother: 123\n", &["!include"])]
    #[case::multi_doc_both_allowed("a: !include foo.yaml\n---\nb: !ref bar.yaml\n", &["!include", "!ref"])]
    fn custom_tags_returns_empty(#[case] input: &str, #[case] allowed_names: &[&str]) {
        let docs = parse_docs(input);
        let allowed = tags_no_type(allowed_names);
        let result = validate_custom_tags(&docs, &allowed);

        assert!(result.is_empty());
    }

    #[rstest]
    #[case::unknown_tag("value: !include foo.yaml\n", &["!other"] as &[&str])]
    #[case::multiple_tags_only_unknown_flagged("a: !include foo.yaml\nb: !ref bar.yaml\n", &["!include"])]
    #[case::nested_tagged_value("outer:\n  inner: !include nested.yaml\n", &["!other"])]
    fn custom_tags_single_diagnostic(#[case] input: &str, #[case] allowed_names: &[&str]) {
        let docs = parse_docs(input);
        let allowed = tags_no_type(allowed_names);
        let result = validate_custom_tags(&docs, &allowed);

        assert_eq!(result.len(), 1);
    }

    // ---- Custom Tags Validator: standalone ----

    #[test]
    fn unknown_tag_produces_warning_with_unknown_tag_code() {
        let text = "value: !include foo.yaml\n";
        let docs = parse_docs(text);
        let allowed = tags_no_type(&["!other"]);
        let result = validate_custom_tags(&docs, &allowed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "unknownTag")
        );
        assert!(result[0].message.contains("!include"));
        assert_eq!(result[0].source.as_deref(), Some("rlsp-yaml"));
    }

    #[test]
    fn tags_in_multi_document_yaml_are_all_checked() {
        let text = "a: !include foo.yaml\n---\nb: !ref bar.yaml\n";
        let docs = parse_docs(text);

        // Neither allowed
        let neither = tags_no_type(&["!other"]);
        let result = validate_custom_tags(&docs, &neither);
        assert_eq!(result.len(), 2);

        // Both allowed
        let both = tags_no_type(&["!include", "!ref"]);
        let result = validate_custom_tags(&docs, &both);
        assert!(result.is_empty());
    }

    // ---- Custom Tags Validator: AST-range regression tests ----

    #[test]
    fn tag_on_mapping_value_range_equals_scalar_loc() {
        let text = "key: !custom value\n";
        let docs = parse_docs(text);
        let allowed = tags_no_type(&["!other"]);
        let result = validate_custom_tags(&docs, &allowed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 0, "scalar on line 0");
        // The parser's (idx.line_column(loc.start).1 as usize) for a tagged scalar points to where the
        // value content begins (after the tag token and separating space).
        // "key: !custom value" — "value" starts at col 13.
        assert_eq!(
            result[0].range.start.character, 13,
            "scalar value content starts at col 13 (after '!custom ')"
        );
    }

    #[test]
    fn tag_on_sequence_item_range_equals_scalar_loc() {
        let text = "items:\n  - !custom value\n";
        let docs = parse_docs(text);
        let allowed = tags_no_type(&["!other"]);
        let result = validate_custom_tags(&docs, &allowed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].range.start.line, 1, "scalar on line 1");
        // "  - !custom value" — "value" starts at col 12.
        assert_eq!(
            result[0].range.start.character, 12,
            "scalar value content starts at col 12 (after '  - !custom ')"
        );
    }

    #[test]
    fn repeated_tag_strings_each_emit_own_diagnostic() {
        let text = "a: !include file1.yaml\nb: !include file2.yaml\n";
        let docs = parse_docs(text);
        let allowed = tags_no_type(&["!other"]);
        let result = validate_custom_tags(&docs, &allowed);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].range.start.line, 0, "first node on line 0");
        assert_eq!(result[1].range.start.line, 1, "second node on line 1");
    }

    #[test]
    fn quoted_scalar_text_containing_tag_syntax_not_flagged() {
        let text = "note: \"use !include for files\"\n";
        let docs = parse_docs(text);
        let allowed = tags_no_type(&["!other"]);
        let result = validate_custom_tags(&docs, &allowed);

        assert!(
            result.is_empty(),
            "quoted scalar has tag: None — no diagnostic expected"
        );
    }

    // ---- validate_custom_tags: new type-annotation tests ----

    #[test]
    fn validate_custom_tags_empty_slice_skips_validation() {
        let docs = parse_docs("value: !include foo\n");
        let result = validate_custom_tags(&docs, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn validate_custom_tags_tag_in_allowed_no_type_annotation_no_diagnostic() {
        let docs = parse_docs("value: !include foo\n");
        let allowed = [CustomTag {
            name: "!include".to_string(),
            expected_type: None,
        }];
        let result = validate_custom_tags(&docs, &allowed);
        assert!(result.is_empty());
    }

    #[test]
    fn validate_custom_tags_tag_not_in_allowed_emits_unknown_tag() {
        let docs = parse_docs("value: !include foo\n");
        let allowed = tags_no_type(&["!ref"]);
        let result = validate_custom_tags(&docs, &allowed);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "unknownTag")
        );
    }

    #[test]
    fn validate_custom_tags_type_match_scalar_no_diagnostic() {
        let docs = parse_docs("value: !include foo\n");
        let allowed = [CustomTag {
            name: "!include".to_string(),
            expected_type: Some(TagNodeType::Scalar),
        }];
        let result = validate_custom_tags(&docs, &allowed);
        assert!(result.is_empty());
    }

    #[test]
    fn validate_custom_tags_type_match_mapping_no_diagnostic() {
        let docs = parse_docs("value: !include {key: val}\n");
        let allowed = [CustomTag {
            name: "!include".to_string(),
            expected_type: Some(TagNodeType::Mapping),
        }];
        let result = validate_custom_tags(&docs, &allowed);
        assert!(result.is_empty());
    }

    #[test]
    fn validate_custom_tags_type_match_sequence_no_diagnostic() {
        let docs = parse_docs("value: !include [a, b]\n");
        let allowed = [CustomTag {
            name: "!include".to_string(),
            expected_type: Some(TagNodeType::Sequence),
        }];
        let result = validate_custom_tags(&docs, &allowed);
        assert!(result.is_empty());
    }

    #[test]
    fn validate_custom_tags_type_mismatch_scalar_expected_mapping_emits_tag_type_mismatch() {
        // scalar node tagged !include, but allowed expects mapping
        let docs = parse_docs("value: !include foo\n");
        let allowed = [CustomTag {
            name: "!include".to_string(),
            expected_type: Some(TagNodeType::Mapping),
        }];
        let result = validate_custom_tags(&docs, &allowed);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "tagTypeMismatch")
        );
        assert!(result[0].message.contains("!include"));
    }

    #[test]
    fn validate_custom_tags_type_mismatch_mapping_expected_sequence_emits_tag_type_mismatch() {
        let docs = parse_docs("value: !include {key: val}\n");
        let allowed = [CustomTag {
            name: "!include".to_string(),
            expected_type: Some(TagNodeType::Sequence),
        }];
        let result = validate_custom_tags(&docs, &allowed);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "tagTypeMismatch")
        );
    }

    #[test]
    fn validate_custom_tags_type_mismatch_sequence_expected_scalar_emits_tag_type_mismatch() {
        let docs = parse_docs("value: !include [a, b]\n");
        let allowed = [CustomTag {
            name: "!include".to_string(),
            expected_type: Some(TagNodeType::Scalar),
        }];
        let result = validate_custom_tags(&docs, &allowed);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "tagTypeMismatch")
        );
    }

    #[test]
    fn validate_custom_tags_tag_type_mismatch_severity_and_code() {
        let docs = parse_docs("value: !include foo\n");
        let allowed = [CustomTag {
            name: "!include".to_string(),
            expected_type: Some(TagNodeType::Mapping),
        }];
        let result = validate_custom_tags(&docs, &allowed);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "tagTypeMismatch")
        );
        assert_eq!(result[0].source.as_deref(), Some("rlsp-yaml"));
    }

    #[test]
    fn validate_custom_tags_duplicate_tag_names_first_entry_wins() {
        // Two entries for "!include" — first has Scalar, second has Mapping.
        // YAML has !include on a scalar. First entry wins → type matches → no diagnostic.
        let docs = parse_docs("value: !include foo\n");
        let allowed = vec![
            CustomTag {
                name: "!include".to_string(),
                expected_type: Some(TagNodeType::Scalar),
            },
            CustomTag {
                name: "!include".to_string(),
                expected_type: Some(TagNodeType::Mapping),
            },
        ];
        let result = validate_custom_tags(&docs, &allowed);
        assert!(
            result.is_empty(),
            "first entry (Scalar) should win; scalar node matches → no diagnostic"
        );
    }

    #[test]
    fn validate_custom_tags_alias_node_not_flagged() {
        let docs = parse_docs("a: &anchor value\nb: *anchor\n");
        let allowed = tags_no_type(&["!ref"]);
        let result = validate_custom_tags(&docs, &allowed);
        // No tags in this document — only anchors/aliases, no tagged nodes.
        assert!(result.is_empty());
    }

    #[test]
    fn validate_custom_tags_yaml_org_tag_not_flagged() {
        // tag:yaml.org,2002: tags are injected by Core schema and must be silently skipped.
        // The parser may inject these; we verify no diagnostic is emitted even when they appear.
        let docs = parse_docs("key: value\n");
        let allowed = tags_no_type(&["!ref"]);
        let result = validate_custom_tags(&docs, &allowed);
        // No user-supplied tags in this document.
        assert!(result.is_empty());
    }

    #[test]
    fn validate_custom_tags_nested_type_mismatch_in_mapping_value() {
        // !include on a nested mapping value when scalar is expected → tagTypeMismatch
        let docs = parse_docs("outer:\n  inner: !include {key: val}\n");
        let allowed = [CustomTag {
            name: "!include".to_string(),
            expected_type: Some(TagNodeType::Scalar),
        }];
        let result = validate_custom_tags(&docs, &allowed);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(result[0].code.as_ref(), Some(NumberOrString::String(s)) if s == "tagTypeMismatch")
        );
        assert_eq!(result[0].range.start.line, 1, "inner is on line 1");
    }
}
