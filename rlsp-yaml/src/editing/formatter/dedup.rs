// SPDX-License-Identifier: MIT

use std::collections::HashSet;

use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{ScalarStyle, Span};

use super::flow_comments::{drain_comments, leading_comments_mut};
use crate::server::YamlVersion;

/// Extract a dedup key string for a mapping key node.
///
/// Returns `Some(key)` for scalar and alias keys, `None` for keys that are never
/// removed: complex keys (mapping or sequence as key) and the merge key `<<`,
/// whose entries each merge their own mappings.
///
/// A scalar key is its tag and its value: `a` and `"a"` are one key, but `1` and
/// `"1"`, or the null key (`: v`) and the empty string key (`"": v`), are two.
/// Under YAML 1.1 the quoting tells keys apart too: `y` is a boolean there and
/// `"y"` a string.
fn dedup_key_str(key: &Node<Span>, version: YamlVersion) -> Option<String> {
    match key {
        Node::Scalar {
            value, tag, style, ..
        } => {
            let plain = matches!(style, ScalarStyle::Plain);
            if plain && value == "<<" {
                return None;
            }
            let quoting = if version == YamlVersion::V1_1 {
                if plain { "plain" } else { "quoted" }
            } else {
                ""
            };
            Some(format!(
                "{}\0{value}\0{quoting}",
                tag.as_deref().unwrap_or_default()
            ))
        }
        Node::Alias { name, .. } => Some(format!("*{name}")),
        Node::Mapping { .. } | Node::Sequence { .. } => None,
    }
}

/// Whether `node` or a node inside it defines an anchor.  An alias further down
/// the document may name it, and the alias loads only while the anchor is there.
fn defines_anchor(node: &Node<Span>) -> bool {
    node.anchor().is_some()
        || match node {
            Node::Mapping { entries, .. } => entries
                .iter()
                .any(|(key, value)| defines_anchor(key) || defines_anchor(value)),
            Node::Sequence { items, .. } => items.iter().any(defines_anchor),
            Node::Scalar { .. } | Node::Alias { .. } => false,
        }
}

/// Remove duplicate mapping keys from the AST, keeping the last occurrence.
///
/// Iterates each `Node::Mapping` in reverse, tracking seen key strings.
/// Earlier duplicate entries are removed; the last occurrence is kept.  An
/// earlier entry that defines an anchor stays, since an alias after it may name
/// the anchor (`a: &x 1` and `b: *x` before a second `a`), and moving the anchor
/// to the entry that stays would change the value of the alias.  The
/// comments of a removed entry, nested ones included, become leading comments of
/// the next entry that stays, ahead of its own.  The last occurrence of a key
/// stays and follows every earlier one, so such an entry always exists.
/// Recurses into values of remaining entries and into sequence items.
///
/// Key extraction rules:
/// - `Node::Scalar { value, tag, .. }` → key string is `tag` and `value`
/// - `Node::Alias { name, .. }` → key string is `*name`
/// - Complex keys (`Node::Mapping`, `Node::Sequence`) and `<<` → skipped (not deduplicated)
pub(super) fn dedup_mapping_keys(node: &mut Node<Span>, version: YamlVersion) {
    match node {
        Node::Mapping { entries, .. } => {
            // Determine which keys to keep by scanning in reverse: the last
            // occurrence of each key is encountered first in reverse order,
            // so it gets inserted into `seen`; earlier occurrences are dropped.
            let mut seen: HashSet<String> = HashSet::new();
            // Build a bitmask of which entries survive, working in reverse.
            let keep: Vec<bool> = entries
                .iter()
                .rev()
                .map(|(key, value)| {
                    // First time we see this key (in reverse) → keep it.
                    // Subsequent times → it's a duplicate earlier occurrence → drop,
                    // unless an anchor of the entry is still needed.
                    // Complex keys (None) are always kept.
                    dedup_key_str(key, version).is_none_or(|k| seen.insert(k))
                        || defines_anchor(key)
                        || defines_anchor(value)
                })
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();

            // Keep the flagged entries; the comments of the others wait in
            // `moved` for the next entry that stays.
            let mut moved: Vec<String> = Vec::new();
            let old = std::mem::take(entries);
            for (mut entry, keep_entry) in old.into_iter().zip(keep) {
                if keep_entry {
                    if !moved.is_empty() {
                        leading_comments_mut(&mut entry.0).splice(0..0, std::mem::take(&mut moved));
                    }
                    entries.push(entry);
                } else {
                    drain_comments(&mut entry.0, &mut moved);
                    drain_comments(&mut entry.1, &mut moved);
                }
            }

            // Recurse into remaining values.
            for (_, value) in entries.iter_mut() {
                dedup_mapping_keys(value, version);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items.iter_mut() {
                dedup_mapping_keys(item, version);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use rlsp_yaml_parser::Event;
    use rstest::rstest;

    use super::super::{YamlFormatOptions, format_yaml};

    fn default_opts() -> YamlFormatOptions {
        YamlFormatOptions::default()
    }

    fn dedup_opts() -> YamlFormatOptions {
        YamlFormatOptions {
            format_remove_duplicate_keys: true,
            ..default_opts()
        }
    }

    // ---- Group B (dedup): Setting disabled — no-op ----

    // B1: duplicate keys are NOT removed when setting is false (default).
    #[test]
    fn dedup_disabled_does_not_remove_duplicate_keys() {
        let input = "key: 1\nkey: 2\n";
        let result = format_yaml(input, &default_opts());
        let count = result.matches("key:").count();
        assert!(
            count >= 2,
            "both keys should remain when dedup disabled: {result:?}"
        );
    }

    // ---- Group C (dedup): Basic dedup behavior ----

    // C1: single duplicate key — last occurrence kept.
    #[test]
    fn dedup_single_duplicate_keeps_last() {
        let result = format_yaml("key: 1\nkey: 2\n", &dedup_opts());
        assert!(
            result.contains("key: 2"),
            "last occurrence missing: {result:?}"
        );
        assert!(
            !result.contains("key: 1"),
            "first occurrence should be removed: {result:?}"
        );
    }

    // C2: three occurrences of same key — only last kept.
    #[test]
    fn dedup_three_occurrences_keeps_only_last() {
        let result = format_yaml("key: a\nkey: b\nkey: c\n", &dedup_opts());
        assert!(
            result.contains("key: c"),
            "last occurrence missing: {result:?}"
        );
        assert!(
            !result.contains("key: a"),
            "first occurrence should be removed: {result:?}"
        );
        assert!(
            !result.contains("key: b"),
            "middle occurrence should be removed: {result:?}"
        );
    }

    // C3: two unique keys — nothing removed.
    #[test]
    fn dedup_unique_keys_unchanged() {
        let result = format_yaml("a: 1\nb: 2\n", &dedup_opts());
        assert!(result.contains("a: 1"), "a:1 missing: {result:?}");
        assert!(result.contains("b: 2"), "b:2 missing: {result:?}");
    }

    // C4: mixed unique and duplicate — only duplicates removed.
    #[test]
    fn dedup_mixed_unique_and_duplicate() {
        let result = format_yaml("a: 1\nb: 2\na: 3\n", &dedup_opts());
        assert!(
            result.contains("a: 3"),
            "last a: should be present: {result:?}"
        );
        assert!(
            result.contains("b: 2"),
            "unique b: should be present: {result:?}"
        );
        assert!(
            !result.contains("a: 1"),
            "first a: should be removed: {result:?}"
        );
    }

    // ---- Group D (dedup): Edge cases — empty and single-entry mappings ----

    // D1: empty mapping — no change.
    #[test]
    fn dedup_empty_mapping_unchanged() {
        let result = format_yaml("map: {}\n", &dedup_opts());
        assert!(
            result.contains("{}"),
            "empty mapping should be preserved: {result:?}"
        );
    }

    // D2: single-entry mapping — no change.
    #[test]
    fn dedup_single_entry_mapping_unchanged() {
        let result = format_yaml("key: value\n", &dedup_opts());
        assert!(
            result.contains("key: value"),
            "single entry should be preserved: {result:?}"
        );
    }

    // ---- Group E (dedup): Key types ----

    // E1: alias key — duplicate alias keys, last kept.
    #[test]
    fn dedup_alias_key_duplicate_keeps_last() {
        // `? *ref` is explicit-key syntax that produces Node::Alias in key position.
        let input = "? *ref\n: value1\n? *ref\n: value2\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(
            result.contains("value2"),
            "last alias-keyed value missing: {result:?}"
        );
        assert!(
            !result.contains("value1"),
            "first alias-keyed value should be removed: {result:?}"
        );
    }

    // E2: complex key (mapping as key) — dedup skipped, no panic.
    #[test]
    fn dedup_complex_mapping_key_no_panic() {
        // Explicit complex key: `? {a: 1}: value`
        // Parser may or may not support this; if parsing fails, format_yaml returns input unchanged.
        let input = "? {a: 1}\n: value\n";
        let result = format_yaml(input, &dedup_opts());
        // Must not panic. Output may be unchanged (if unparseable) or formatted.
        let _ = result;
    }

    // E3: complex key (sequence as key) — dedup skipped, no panic.
    #[test]
    fn dedup_complex_sequence_key_no_panic() {
        let input = "? [1, 2]\n: value\n";
        let result = format_yaml(input, &dedup_opts());
        let _ = result;
    }

    // E4: case-sensitive key comparison — `Key` and `key` are distinct.
    #[test]
    fn dedup_case_sensitive_keys_both_kept() {
        let result = format_yaml("Key: 1\nkey: 2\n", &dedup_opts());
        assert!(result.contains("Key: 1"), "Key:1 missing: {result:?}");
        assert!(result.contains("key: 2"), "key:2 missing: {result:?}");
    }

    // ---- Group F (dedup): Recursion ----

    // F1: nested mapping — duplicate keys in nested mapping removed.
    #[test]
    fn dedup_nested_mapping_removes_inner_duplicates() {
        let input = "outer:\n  inner: 1\n  inner: 2\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(result.contains("outer:"), "outer key missing: {result:?}");
        assert!(
            result.contains("inner: 2"),
            "last inner should be kept: {result:?}"
        );
        assert!(
            !result.contains("inner: 1"),
            "first inner should be removed: {result:?}"
        );
    }

    // F2: mapping inside sequence — dedup recurses through sequence items.
    #[test]
    fn dedup_recurses_into_sequence_items() {
        let input = "items:\n  - key: 1\n    key: 2\n  - key: 3\n    key: 4\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(
            result.contains("key: 2"),
            "last key in first item missing: {result:?}"
        );
        assert!(
            result.contains("key: 4"),
            "last key in second item missing: {result:?}"
        );
        assert!(
            !result.contains("key: 1"),
            "first key in first item should be removed: {result:?}"
        );
        assert!(
            !result.contains("key: 3"),
            "first key in second item should be removed: {result:?}"
        );
    }

    // F3: deeply nested — dedup recurses more than two levels.
    #[test]
    fn dedup_deeply_nested_removes_innermost_duplicates() {
        let input = "a:\n  b:\n    c: 1\n    c: 2\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(result.contains("a:"), "a: missing: {result:?}");
        assert!(result.contains("b:"), "b: missing: {result:?}");
        assert!(
            result.contains("c: 2"),
            "last c: should be kept: {result:?}"
        );
        assert!(
            !result.contains("c: 1"),
            "first c: should be removed: {result:?}"
        );
    }

    // ---- Group G (dedup): Flow mappings ----

    // G1: flow mapping — dedup works for flow style.
    #[test]
    fn dedup_flow_mapping_removes_duplicate() {
        let result = format_yaml("{key: 1, key: 2}\n", &dedup_opts());
        assert!(
            result.contains("key: 2"),
            "last occurrence missing: {result:?}"
        );
        assert!(
            !result.contains("key: 1"),
            "first occurrence should be removed: {result:?}"
        );
    }

    // ---- Group H (dedup): Comments on removed entries ----

    // H1: comment on removed entry — no crash; surviving output is valid.
    #[test]
    fn dedup_removed_entry_with_trailing_comment_no_crash() {
        // The first `key` has an inline comment; it should be removed without panic.
        let input = "key: 1  # this gets removed\nkey: 2\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(
            result.contains("key: 2"),
            "last occurrence missing: {result:?}"
        );
        assert!(
            !result.contains("key: 1"),
            "first occurrence should be removed: {result:?}"
        );
    }

    // H2: leading comment on surviving (last) entry — comment preserved.
    #[test]
    fn dedup_surviving_entry_leading_comment_preserved() {
        // The last `key` has a leading comment; it should survive dedup.
        let input = "key: 1\n# keep this\nkey: 2\n";
        let result = format_yaml(input, &dedup_opts());
        assert!(
            result.contains("key: 2"),
            "last occurrence missing: {result:?}"
        );
        assert!(
            result.contains("# keep this"),
            "leading comment should be preserved: {result:?}"
        );
    }

    // ---- Group K (dedup): Comments of removed entries move ----

    /// The comment texts of `yaml`, as the parser reports them, sorted.
    fn sorted_comments(yaml: &str) -> Vec<String> {
        let mut comments: Vec<String> = rlsp_yaml_parser::parse_events(yaml)
            .filter_map(|event| match event {
                Ok((Event::Comment { text }, _)) => Some(text.to_string()),
                _ => None,
            })
            .collect();
        comments.sort_unstable();
        comments
    }

    // K1: every comment of the input reaches the output, wherever it is placed.
    #[rstest]
    #[case::trailing_comment("a: 1 # x\nb: 2\na: 3\n")]
    #[case::three_occurrences("a: 1 # x\na: 2 # y\na: 3 # z\nb: 0\n")]
    #[case::removed_entries_apart("a: 1 # x\nb: 1\na: 2 # y\nb: 2 # z\na: 3\n")]
    #[case::nested_value("a: # k\n  # lead v\n  b: 1 # t\n  # tail\nc: 2\na: 3\n")]
    #[case::sequence_value("a:\n  - x # i1\n  - # i2\n    y: 1 # i3\nb: 1\na: 2\n")]
    #[case::duplicates_inside_removed_value("a:\n  x: 1 # c1\n  x: 2 # c2\nb: 1\na: 3\n")]
    #[case::nested_mapping("o:\n  a: 1 # x\n  a: 2\np: 3\n")]
    #[case::sequence_item("- a: 1 # x\n  a: 2\n- b: 1 # y\n  b: 2\n")]
    #[case::root_flow_mapping("{a: 1, # c\n b: 2, a: 3}\n")]
    #[case::flow_key_survivor("a: 1 # x\n? [b, # in\n  c]\n: v\na: 2\n")]
    #[case::alias_key("? *r # kc\n: 1 # vc\n? *r\n: 2\n")]
    fn removed_entry_comments_reach_output(#[case] input: &str) {
        let result = format_yaml(input, &dedup_opts());
        assert_eq!(
            sorted_comments(&result),
            sorted_comments(input),
            "comments changed: {result:?}"
        );
    }

    // K3: the output of K1 changes nothing under a second format.
    #[rstest]
    #[case::trailing_comment("a: 1 # x\nb: 2\na: 3\n")]
    #[case::removed_entries_apart("a: 1 # x\nb: 1\na: 2 # y\nb: 2 # z\na: 3\n")]
    #[case::nested_value("a: # k\n  # lead v\n  b: 1 # t\n  # tail\nc: 2\na: 3\n")]
    #[case::sequence_item("- a: 1 # x\n  a: 2\n- b: 1 # y\n  b: 2\n")]
    #[case::root_flow_mapping("{a: 1, # c\n b: 2, a: 3}\n")]
    #[case::flow_key_survivor("a: 1 # x\n? [b, # in\n  c]\n: v\na: 2\n")]
    #[case::alias_key("? *r # kc\n: 1 # vc\n? *r\n: 2\n")]
    fn output_with_moved_comments_is_stable(#[case] input: &str) {
        let first = format_yaml(input, &dedup_opts());
        assert_eq!(
            format_yaml(&first, &dedup_opts()),
            first,
            "not stable: {first:?}"
        );
    }

    // K4: the comment of a removed entry prints above the key that stays.
    #[test]
    fn dedup_removed_entry_comment_lands_above_survivor() {
        let result = format_yaml("key: 1  # this gets removed\nkey: 2\n", &dedup_opts());
        assert_eq!(result, "# this gets removed\nkey: 2\n");
    }

    // K5: without the option nothing is removed, so no comment moves.
    #[test]
    fn dedup_disabled_keeps_comments_in_place() {
        let result = format_yaml("a: 1 # x\nb: 2\na: 3\n", &default_opts());
        assert_eq!(result, "a: 1  # x\nb: 2\na: 3\n");
    }

    // K6: without a duplicate nothing is removed, so no comment moves.
    #[test]
    fn dedup_without_duplicates_does_not_move_comments() {
        let result = format_yaml("a: 1 # x\nb: 2 # y\n", &dedup_opts());
        assert_eq!(result, "a: 1  # x\nb: 2  # y\n");
    }

    // ---- Group I (dedup): Multiple documents ----

    // I1: duplicate keys in each document — dedup is per-document.
    #[test]
    fn dedup_multi_document_per_document() {
        let input = "key: 1\nkey: 2\n---\nkey: 3\nkey: 4\n";
        let result = format_yaml(input, &dedup_opts());
        // Each document should have had its first `key` removed.
        // The result should contain `key: 2` and `key: 4` (the last in each doc).
        assert!(
            result.contains("key: 2"),
            "last key in doc1 missing: {result:?}"
        );
        assert!(
            result.contains("key: 4"),
            "last key in doc2 missing: {result:?}"
        );
        assert!(
            result.contains("---"),
            "document separator missing: {result:?}"
        );
        assert!(
            !result.contains("key: 1"),
            "first key in doc1 should be removed: {result:?}"
        );
        assert!(
            !result.contains("key: 3"),
            "first key in doc2 should be removed: {result:?}"
        );
    }

    // ---- Group J (dedup): Idempotency ----

    // J1: format_remove_duplicate_keys: true is idempotent.
    #[test]
    fn dedup_idempotent() {
        let input = "key: 1\nkey: 2\n";
        let first = format_yaml(input, &dedup_opts());
        let second = format_yaml(&first, &dedup_opts());
        assert_eq!(first, second, "dedup not idempotent: {first:?}");
    }
}
