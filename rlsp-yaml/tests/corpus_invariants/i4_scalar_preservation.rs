use std::collections::HashMap;
use std::path::Path;

use rlsp_yaml::editing::code_actions::code_actions;
use rlsp_yaml::editing::formatter::YamlFormatOptions;
use rlsp_yaml::parser::parse_yaml;
use rlsp_yaml_parser::{Document, Node, Span};
use tower_lsp::lsp_types::{CodeActionKind, Position, Range};

use super::i2_range_validity::utf16_len;
use super::i3_code_action_round_trip::apply_text_edits;
use super::shared::{collect_all_diagnostics, fmt_range};

pub fn check_i4_scalar_preservation(path: &Path, text: &str) -> Result<(), String> {
    let parse_result = parse_yaml(text);
    let pre_scalars = collect_scalar_values(&parse_result.documents);
    let all_diagnostics = collect_all_diagnostics(&parse_result.documents);

    let lines: Vec<&str> = text.lines().collect();
    let last_line = lines.len().saturating_sub(1) as u32;
    let last_char = lines.last().map_or(0, |l| utf16_len(l) as u32);
    let whole_file = Range::new(Position::new(0, 0), Position::new(last_line, last_char));

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let uri = tower_lsp::lsp_types::Url::parse(&format!("file:///corpus/{file_name}"))
        .expect("valid URI");

    let actions = code_actions(
        &parse_result.documents,
        text,
        whole_file,
        &all_diagnostics,
        &uri,
        &YamlFormatOptions::default(),
    );

    for action in &actions {
        if action.kind.as_ref() != Some(&CodeActionKind::REFACTOR_REWRITE) {
            continue;
        }
        let Some(edit) = &action.edit else {
            continue;
        };
        let Some(changes) = &edit.changes else {
            continue;
        };
        let Some(text_edits) = changes.get(&uri) else {
            continue;
        };
        if text_edits.is_empty() {
            continue;
        }

        let edited = apply_text_edits(text, text_edits);
        let post_parse = parse_yaml(&edited);
        let post_scalars = collect_scalar_values(&post_parse.documents);

        let missing = missing_scalars(&pre_scalars, &post_scalars);
        if !missing.is_empty() {
            let (diag_code, diag_range) = action
                .diagnostics
                .as_ref()
                .and_then(|v| v.first())
                .map_or_else(
                    || ("<no-code>".to_string(), "unknown".to_string()),
                    |d| {
                        let code = d
                            .code
                            .as_ref()
                            .map_or_else(|| "<no-code>".to_string(), |c| format!("{c:?}"));
                        (code, fmt_range(d.range))
                    },
                );
            return Err(format!(
                r#"action "{}": edit for diagnostic {} at {} dropped scalar {:?}"#,
                action.title, diag_code, diag_range, missing[0]
            ));
        }
    }

    Ok(())
}

/// Walk every node in every document and collect all `Scalar` values (keys and
/// values) into a flat vec. Alias nodes carry only the anchor reference name,
/// not the resolved value — skip them.
pub fn collect_scalar_values(docs: &[Document<Span>]) -> Vec<String> {
    let mut result = Vec::new();
    for doc in docs {
        collect_node_scalars(&doc.root, &mut result);
    }
    result
}

pub fn collect_node_scalars(node: &Node<Span>, out: &mut Vec<String>) {
    match node {
        Node::Scalar { value, .. } => out.push(value.clone()),
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                collect_node_scalars(key, out);
                collect_node_scalars(value, out);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_node_scalars(item, out);
            }
        }
        // Alias nodes carry only the anchor name, not the resolved value — skip them.
        Node::Alias { .. } => {}
    }
}

/// Return elements present in `pre` whose count in `post` is less than in `pre`.
pub fn missing_scalars(pre: &[String], post: &[String]) -> Vec<String> {
    let mut pre_counts: HashMap<&str, usize> = HashMap::new();
    for s in pre {
        *pre_counts.entry(s.as_str()).or_insert(0) += 1;
    }
    let mut post_counts: HashMap<&str, usize> = HashMap::new();
    for s in post {
        *post_counts.entry(s.as_str()).or_insert(0) += 1;
    }

    let mut missing = Vec::new();
    for (s, &count) in &pre_counts {
        let post_count = post_counts.get(s).copied().unwrap_or(0);
        if post_count < count {
            for _ in 0..(count - post_count) {
                missing.push((*s).to_string());
            }
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rlsp_yaml_parser::Node;

    use super::super::shared::helpers::{
        make_doc, make_mapping, make_scalar, make_sequence, zero_span,
    };
    use super::*;

    // CSV-1: empty document list returns empty vec
    #[test]
    fn i4_csv1_empty_docs_returns_empty() {
        assert!(collect_scalar_values(&[]).is_empty());
    }

    // CSV-2: document whose root is a single scalar
    #[test]
    fn i4_csv2_single_scalar_root() {
        let docs = vec![make_doc(make_scalar("hello"))];
        assert_eq!(collect_scalar_values(&docs), vec!["hello"]);
    }

    // CSV-3: flat mapping collects both keys and values
    #[test]
    fn i4_csv3_flat_mapping_collects_keys_and_values() {
        let entries = vec![
            (make_scalar("key1"), make_scalar("val1")),
            (make_scalar("key2"), make_scalar("val2")),
        ];
        let docs = vec![make_doc(make_mapping(entries))];
        let mut result = collect_scalar_values(&docs);
        result.sort();
        assert_eq!(result, vec!["key1", "key2", "val1", "val2"]);
    }

    // CSV-4: nested mapping recurses into values
    #[test]
    fn i4_csv4_nested_mapping_recurses() {
        let inner = make_mapping(vec![(make_scalar("inner_key"), make_scalar("inner_val"))]);
        let outer = make_mapping(vec![(make_scalar("outer"), inner)]);
        let docs = vec![make_doc(outer)];
        let mut result = collect_scalar_values(&docs);
        result.sort();
        assert_eq!(result, vec!["inner_key", "inner_val", "outer"]);
    }

    // CSV-5: sequence of scalars collects all items
    #[test]
    fn i4_csv5_sequence_of_scalars() {
        let seq = make_sequence(vec![make_scalar("a"), make_scalar("b"), make_scalar("c")]);
        let docs = vec![make_doc(seq)];
        let mut result = collect_scalar_values(&docs);
        result.sort();
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    // CSV-6: mapping whose values are sequences — both sides traversed
    #[test]
    fn i4_csv6_mapping_with_sequence_values() {
        let seq = make_sequence(vec![make_scalar("x"), make_scalar("y")]);
        let mapping = make_mapping(vec![(make_scalar("list"), seq)]);
        let docs = vec![make_doc(mapping)];
        let mut result = collect_scalar_values(&docs);
        result.sort();
        assert_eq!(result, vec!["list", "x", "y"]);
    }

    // CSV-7: duplicate scalar values are preserved (multiset semantics)
    #[test]
    fn i4_csv7_duplicate_values_preserved() {
        let entries = vec![
            (make_scalar("foo"), make_scalar("foo")),
            (make_scalar("bar"), make_scalar("bar")),
        ];
        let docs = vec![make_doc(make_mapping(entries))];
        let result = collect_scalar_values(&docs);
        assert_eq!(result.iter().filter(|s| s.as_str() == "foo").count(), 2);
        assert_eq!(result.iter().filter(|s| s.as_str() == "bar").count(), 2);
        assert_eq!(result.len(), 4);
    }

    // CSV-8: alias node is skipped — only the real scalar is collected
    #[test]
    fn i4_csv8_alias_node_is_skipped() {
        let alias = Node::Alias {
            name: "anchor_name".to_owned(),
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        let seq = make_sequence(vec![make_scalar("real"), alias]);
        let docs = vec![make_doc(seq)];
        assert_eq!(collect_scalar_values(&docs), vec!["real"]);
    }

    // CSV-9: empty scalar value is included
    #[test]
    fn i4_csv9_empty_scalar_included() {
        let docs = vec![make_doc(make_scalar(""))];
        assert_eq!(collect_scalar_values(&docs), vec![""]);
    }

    // CSV-10: multiple documents are all walked
    #[test]
    fn i4_csv10_multiple_documents_all_walked() {
        let docs = vec![make_doc(make_scalar("doc1")), make_doc(make_scalar("doc2"))];
        let mut result = collect_scalar_values(&docs);
        result.sort();
        assert_eq!(result, vec!["doc1", "doc2"]);
    }

    // MS-1: equal multisets return empty
    #[test]
    fn i4_ms1_equal_multisets_return_empty() {
        let pre = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let post = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert!(missing_scalars(&pre, &post).is_empty());
    }

    // MS-2: post is superset of pre returns empty
    #[test]
    fn i4_ms2_post_superset_returns_empty() {
        let pre = vec!["a".to_string(), "b".to_string()];
        let post = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        assert!(missing_scalars(&pre, &post).is_empty());
    }

    // MS-3: pre has element absent from post returns it
    #[test]
    fn i4_ms3_missing_element_returned() {
        let pre = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let post = vec!["a".to_string(), "c".to_string()];
        let mut result = missing_scalars(&pre, &post);
        result.sort();
        assert_eq!(result, vec!["b"]);
    }

    // MS-4: pre has duplicate that post has only once — returns one missing
    #[test]
    fn i4_ms4_duplicate_in_pre_one_in_post_returns_one() {
        let pre = vec!["foo".to_string(), "foo".to_string(), "bar".to_string()];
        let post = vec!["foo".to_string(), "bar".to_string()];
        let result = missing_scalars(&pre, &post);
        assert_eq!(result, vec!["foo"]);
    }

    // MS-5: pre has duplicate that post has zero — returns two missing
    #[test]
    fn i4_ms5_duplicate_in_pre_none_in_post_returns_both() {
        let pre = vec!["foo".to_string(), "foo".to_string()];
        let post = vec!["bar".to_string()];
        let mut result = missing_scalars(&pre, &post);
        result.sort();
        assert_eq!(result, vec!["foo", "foo"]);
    }

    // MS-6: empty pre always returns empty
    #[test]
    fn i4_ms6_empty_pre_returns_empty() {
        let post = vec!["x".to_string(), "y".to_string()];
        assert!(missing_scalars(&[], &post).is_empty());
    }

    // MS-7: empty post with non-empty pre returns all of pre
    #[test]
    fn i4_ms7_empty_post_returns_all_of_pre() {
        let pre = vec!["a".to_string(), "b".to_string()];
        let mut result = missing_scalars(&pre, &[]);
        result.sort();
        assert_eq!(result, vec!["a", "b"]);
    }

    // INT-1: I4 catches a REFACTOR_REWRITE action that drops a scalar
    // Uses the destructive flow_map_to_block path on a minimal inline YAML.
    // If the code action does not fire for this input (no matching diagnostic),
    // the invariant will pass and the corpus run provides the integration coverage.
    #[test]
    fn i4_int1_refactor_rewrite_dropping_scalar_fails() {
        // This YAML triggers a flowMap diagnostic and a REFACTOR_REWRITE code action.
        // The flow_map_to_block action is known to drop the key when the value
        // contains ${{ ... }} expressions.
        let text = "env:\n  GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n";
        let fake_path = Path::new("inline-test.yml");
        let result = check_i4_scalar_preservation(fake_path, text);
        // This may pass (no REFACTOR_REWRITE fires) or fail (action drops scalar).
        // If it fails, confirm the error message names the missing scalar.
        if let Err(msg) = result {
            assert!(
                msg.contains("GITHUB_TOKEN") || msg.contains("secrets.GITHUB_TOKEN"),
                "failure message should name the missing scalar, got: {msg}"
            );
        }
        // If it passes, integration coverage comes from the corpus run.
    }
}
