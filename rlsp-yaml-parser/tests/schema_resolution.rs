// SPDX-License-Identifier: MIT
#![expect(
    clippy::panic,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::wildcard_enum_match_arm,
    reason = "test code"
)]

//! Integration tests for YAML 1.2.2 §10 schema tag resolution wired into the
//! loader.
//!
//! All tests exercise the public API through the production entry points:
//! `LoaderBuilder::new().schema(s).build().load(input)` and `load()`.

use std::borrow::Cow;

use rlsp_yaml_parser::loader::LoaderBuilder;
use rlsp_yaml_parser::{LoadError, Node, Schema};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the tag from the root scalar of a single-document YAML string
/// loaded with the given schema.
fn scalar_tag(input: &str, schema: Schema) -> Option<Cow<'static, str>> {
    let docs = LoaderBuilder::new()
        .schema(schema)
        .build()
        .load(input)
        .expect("load failed");
    assert_eq!(docs.len(), 1, "expected exactly one document");
    match &docs[0].root {
        Node::Scalar { tag, .. } => tag.clone(),
        other => panic!("expected root scalar, got: {other:?}"),
    }
}

/// Extract the tag from the root mapping of a single-document YAML string.
fn mapping_tag(input: &str, schema: Schema) -> Option<Cow<'static, str>> {
    let docs = LoaderBuilder::new()
        .schema(schema)
        .build()
        .load(input)
        .expect("load failed");
    assert_eq!(docs.len(), 1, "expected exactly one document");
    match &docs[0].root {
        Node::Mapping { tag, .. } => tag.clone(),
        other => panic!("expected root mapping, got: {other:?}"),
    }
}

/// Extract the tag from the root sequence of a single-document YAML string.
fn sequence_tag(input: &str, schema: Schema) -> Option<Cow<'static, str>> {
    let docs = LoaderBuilder::new()
        .schema(schema)
        .build()
        .load(input)
        .expect("load failed");
    assert_eq!(docs.len(), 1, "expected exactly one document");
    match &docs[0].root {
        Node::Sequence { tag, .. } => tag.clone(),
        other => panic!("expected root sequence, got: {other:?}"),
    }
}

/// Load YAML text with the given schema.  File-local replacement for the
/// removed `load_with_schema` public convenience function.
fn load_with_schema(
    input: &str,
    schema: Schema,
) -> Result<Vec<rlsp_yaml_parser::Document<rlsp_yaml_parser::Span>>, rlsp_yaml_parser::LoadError> {
    LoaderBuilder::new().schema(schema).build().load(input)
}

// ---------------------------------------------------------------------------
// Group 1 — Core schema: plain scalar type dispatch (IT-1 through IT-7)
// ---------------------------------------------------------------------------

// IT-1 (spike): plain integer resolves to !!int
#[test]
fn core_plain_integer_resolves_to_int() {
    let tag = scalar_tag("42\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:int"));
}

// IT-2: plain `true` resolves to !!bool
#[test]
fn core_plain_bool_resolves_to_bool() {
    let tag = scalar_tag("true\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:bool"));
}

// IT-3: plain unmatched string resolves to !!str
#[test]
fn core_plain_string_resolves_to_str() {
    let tag = scalar_tag("hello\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// IT-4: plain `3.14` resolves to !!float
#[test]
fn core_plain_float_resolves_to_float() {
    let tag = scalar_tag("3.14\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:float"));
}

// IT-5: plain `null` resolves to !!null
#[test]
fn core_plain_null_lowercase_resolves_to_null() {
    let tag = scalar_tag("null\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:null"));
}

// IT-6: plain `~` resolves to !!null
#[test]
fn core_plain_tilde_resolves_to_null() {
    let tag = scalar_tag("~\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:null"));
}

// IT-7: plain empty scalar resolves to !!null
#[test]
fn core_plain_empty_resolves_to_null() {
    // A document with only a `---` marker has an empty root scalar.
    let tag = scalar_tag("---\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:null"));
}

// ---------------------------------------------------------------------------
// Group 2 — Core schema: non-plain styles always resolve to !!str (IT-8, IT-9)
// ---------------------------------------------------------------------------

// IT-8: double-quoted integer resolves to !!str (not !!int)
#[test]
fn core_double_quoted_integer_resolves_to_str() {
    let tag = scalar_tag("\"42\"\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// IT-9: block literal scalar resolves to !!str
#[test]
fn core_block_literal_resolves_to_str() {
    let tag = scalar_tag("|\n  hello\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// ---------------------------------------------------------------------------
// Group 3 — Core schema: explicit tag preserved, not overridden (IT-10, IT-11)
// ---------------------------------------------------------------------------

// IT-10: explicitly tagged `!!str 42` — tag preserved as !!str, not overridden
#[test]
fn core_explicit_str_tag_on_integer_value_preserved() {
    let tag = scalar_tag("!!str 42\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// IT-11: explicitly tagged `!!int "123"` (quoted, explicit int tag) — preserved
#[test]
fn core_explicit_int_tag_on_quoted_string_preserved() {
    let tag = scalar_tag("!!int \"123\"\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:int"));
}

// ---------------------------------------------------------------------------
// Group 4 — Core schema: bare `!` tag resolved by kind (IT-12 through IT-15)
// ---------------------------------------------------------------------------

// IT-12: bare `!` on a plain scalar → resolved to !!str (by-kind, not by content)
#[test]
fn core_bare_excl_on_plain_scalar_resolves_to_str() {
    // Plain scalar value is `42`, but bare `!` means "non-specific tag for
    // scalars" which resolves to !!str regardless of content.
    let tag = scalar_tag("! 42\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// IT-13: bare `!` on a plain `true` scalar → !!str (not !!bool)
#[test]
fn core_bare_excl_on_bool_value_resolves_to_str() {
    let tag = scalar_tag("! true\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// IT-14: bare `!` on a sequence → !!seq
#[test]
fn core_bare_excl_on_sequence_resolves_to_seq() {
    let tag = sequence_tag("! [a, b]\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:seq"));
}

// IT-15: bare `!` on a mapping → !!map
#[test]
fn core_bare_excl_on_mapping_resolves_to_map() {
    let tag = mapping_tag("! {a: b}\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:map"));
}

// ---------------------------------------------------------------------------
// Group 5 — Core schema: untagged collections (IT-16, IT-17)
// ---------------------------------------------------------------------------

// IT-16: untagged mapping → !!map
#[test]
fn core_untagged_mapping_resolves_to_map() {
    let tag = mapping_tag("a: 1\nb: 2\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:map"));
}

// IT-17: untagged sequence → !!seq
#[test]
fn core_untagged_sequence_resolves_to_seq() {
    let tag = sequence_tag("- a\n- b\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:seq"));
}

// ---------------------------------------------------------------------------
// Group 6 — Core schema: nested structure, all tags resolved (IT-18)
// ---------------------------------------------------------------------------

// IT-18: nested mapping with sequence values containing mixed scalar types —
// all tags resolved at every level
#[test]
fn core_nested_structure_all_tags_resolved() {
    let yaml = "numbers:\n  - 1\n  - 3.14\n  - hello\n  - true\n";
    let docs = load_with_schema(yaml, Schema::Core).expect("load failed");
    assert_eq!(docs.len(), 1);

    let Node::Mapping {
        entries,
        tag: map_tag,
        ..
    } = &docs[0].root
    else {
        panic!("expected root mapping");
    };
    assert_eq!(map_tag.as_deref(), Some("tag:yaml.org,2002:map"));

    let (_key, value) = &entries[0];
    let Node::Sequence {
        items,
        tag: seq_tag,
        ..
    } = value
    else {
        panic!("expected sequence value");
    };
    assert_eq!(seq_tag.as_deref(), Some("tag:yaml.org,2002:seq"));

    let expected_tags = [
        "tag:yaml.org,2002:int",
        "tag:yaml.org,2002:float",
        "tag:yaml.org,2002:str",
        "tag:yaml.org,2002:bool",
    ];
    assert_eq!(items.len(), expected_tags.len());
    for (item, expected) in items.iter().zip(expected_tags.iter()) {
        let Node::Scalar { tag, .. } = item else {
            panic!("expected scalar item");
        };
        assert_eq!(
            tag.as_deref(),
            Some(*expected),
            "wrong tag for item {item:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Group 7 — Core schema: numeric edge cases (IT-19 through IT-24)
// ---------------------------------------------------------------------------

// IT-19: `0o777` → !!int (octal)
#[test]
fn core_octal_resolves_to_int() {
    let tag = scalar_tag("0o777\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:int"));
}

// IT-20: `0x1A` → !!int (hex)
#[test]
fn core_hex_resolves_to_int() {
    let tag = scalar_tag("0x1A\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:int"));
}

// IT-21: `.inf` → !!float
#[test]
fn core_inf_resolves_to_float() {
    let tag = scalar_tag(".inf\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:float"));
}

// IT-22: `.nan` → !!float
#[test]
fn core_nan_resolves_to_float() {
    let tag = scalar_tag(".nan\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:float"));
}

// IT-GAP-S2: `1.e5` (trailing-dot-with-exponent) → !!float under Core schema
#[test]
fn core_trailing_dot_with_exponent_resolves_to_float() {
    let tag = scalar_tag("1.e5\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:float"));
}

// IT-23: `+12` → !!int (Core allows leading `+`)
#[test]
fn core_positive_signed_int_resolves_to_int() {
    let tag = scalar_tag("+12\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:int"));
}

// IT-24: `-0` → !!int (Core: `-0` matches `[-+]?[0-9]+`)
#[test]
fn core_negative_zero_resolves_to_int() {
    let tag = scalar_tag("-0\n", Schema::Core);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:int"));
}

// ---------------------------------------------------------------------------
// Group 8 — Default `load()` uses Core schema (IT-25r through IT-new)
// ---------------------------------------------------------------------------

// IT-25r: default load() — plain integer resolves to !!int
#[test]
fn load_default_plain_integer_resolves_to_int() {
    let docs = rlsp_yaml_parser::loader::load("42\n").expect("load failed");
    let Node::Scalar { tag, .. } = &docs[0].root else {
        panic!("expected scalar");
    };
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:int"));
}

// IT-26r: default load() — plain `null` resolves to !!null
#[test]
fn load_default_plain_null_resolves_to_null() {
    let docs = rlsp_yaml_parser::loader::load("null\n").expect("load failed");
    let Node::Scalar { tag, .. } = &docs[0].root else {
        panic!("expected scalar");
    };
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:null"));
}

// IT-27r: default load() — untagged mapping resolves to !!map
#[test]
fn load_default_mapping_resolves_to_map() {
    let docs = rlsp_yaml_parser::loader::load("a: 1\n").expect("load failed");
    let Node::Mapping { tag, .. } = &docs[0].root else {
        panic!("expected mapping");
    };
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:map"));
}

// IT-28r: LoaderBuilder default (no .schema() call) uses Core schema
#[test]
fn loader_builder_default_uses_core_schema() {
    let docs = LoaderBuilder::new()
        .build()
        .load("42\n")
        .expect("load failed");
    let Node::Scalar { tag, .. } = &docs[0].root else {
        panic!("expected scalar");
    };
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:int"));
}

// IT-29r: LoaderBuilder default and explicit Core schema produce identical ASTs
#[test]
fn loader_builder_explicit_core_same_as_default() {
    let input = "true\n";
    let via_default = LoaderBuilder::new()
        .build()
        .load(input)
        .expect("default failed");
    let via_explicit = LoaderBuilder::new()
        .schema(Schema::Core)
        .build()
        .load(input)
        .expect("explicit failed");
    assert_eq!(via_default, via_explicit);
}

// IT-new: default load() — empty document (distinct code path) resolves to !!null
#[test]
fn load_default_empty_document_resolves_to_null() {
    let docs = rlsp_yaml_parser::loader::load("---\n").expect("load failed");
    let Node::Scalar { tag, .. } = &docs[0].root else {
        panic!("expected scalar");
    };
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:null"));
}

// ---------------------------------------------------------------------------
// Group 10 — JSON schema: plain scalar happy-path type dispatch (IT-30–IT-34)
// ---------------------------------------------------------------------------

// IT-30: plain `42` → !!int under JSON schema
#[test]
fn json_plain_int_resolves_to_int() {
    let tag = scalar_tag("42\n", Schema::Json);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:int"));
}

// IT-31: plain `true` → !!bool under JSON schema
#[test]
fn json_plain_bool_resolves_to_bool() {
    let tag = scalar_tag("true\n", Schema::Json);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:bool"));
}

// IT-32: plain `null` → !!null under JSON schema
#[test]
fn json_plain_null_resolves_to_null() {
    let tag = scalar_tag("null\n", Schema::Json);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:null"));
}

// IT-33: plain `3.14` → !!float under JSON schema
#[test]
fn json_plain_float_resolves_to_float() {
    let tag = scalar_tag("3.14\n", Schema::Json);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:float"));
}

// IT-GAP-S3: `0e5` (bare-zero-with-exponent) → !!float under JSON schema
#[test]
fn json_zero_with_exponent_resolves_to_float() {
    let tag = scalar_tag("0e5\n", Schema::Json);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:float"));
}

// IT-34: plain `false` → !!bool under JSON schema
#[test]
fn json_plain_false_resolves_to_bool() {
    let tag = scalar_tag("false\n", Schema::Json);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:bool"));
}

// IT-34b: plain `0` → !!int under JSON schema (bare-zero special case in is_json_int)
#[test]
fn json_plain_zero_resolves_to_int() {
    let tag = scalar_tag("0\n", Schema::Json);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:int"));
}

// ---------------------------------------------------------------------------
// Group 11 — JSON schema: plain scalar error paths (IT-35–IT-41)
// ---------------------------------------------------------------------------

// IT-35: plain `hello` → LoadError::UnresolvedScalar (no JSON pattern matches)
#[test]
fn json_plain_string_returns_unresolved_scalar_error() {
    let result = load_with_schema("hello\n", Schema::Json);
    assert!(
        matches!(
            result,
            Err(rlsp_yaml_parser::LoadError::UnresolvedScalar { .. })
        ),
        "expected UnresolvedScalar, got: {result:?}"
    );
}

// IT-36: plain `0o777` → error (octal is not in JSON int pattern)
#[test]
fn json_octal_plain_returns_unresolved_scalar_error() {
    let result = load_with_schema("0o777\n", Schema::Json);
    assert!(
        matches!(
            result,
            Err(rlsp_yaml_parser::LoadError::UnresolvedScalar { .. })
        ),
        "expected UnresolvedScalar for octal, got: {result:?}"
    );
}

// IT-37: plain `+42` → error (JSON int requires no leading +)
#[test]
fn json_plus_prefix_int_returns_unresolved_scalar_error() {
    let result = load_with_schema("+42\n", Schema::Json);
    assert!(
        matches!(
            result,
            Err(rlsp_yaml_parser::LoadError::UnresolvedScalar { .. })
        ),
        "expected UnresolvedScalar for +42, got: {result:?}"
    );
}

// IT-38: plain `~` → error (JSON null is only `null`, not `~`)
#[test]
fn json_tilde_returns_unresolved_scalar_error() {
    let result = load_with_schema("~\n", Schema::Json);
    assert!(
        matches!(
            result,
            Err(rlsp_yaml_parser::LoadError::UnresolvedScalar { .. })
        ),
        "expected UnresolvedScalar for ~, got: {result:?}"
    );
}

// IT-39: plain `TRUE` → error (JSON bool is only `true`/`false`, not uppercase)
#[test]
fn json_uppercase_bool_returns_unresolved_scalar_error() {
    let result = load_with_schema("TRUE\n", Schema::Json);
    assert!(
        matches!(
            result,
            Err(rlsp_yaml_parser::LoadError::UnresolvedScalar { .. })
        ),
        "expected UnresolvedScalar for TRUE, got: {result:?}"
    );
}

// IT-40: plain `.inf` → error (JSON float does not include infinity notation)
#[test]
fn json_inf_notation_returns_unresolved_scalar_error() {
    let result = load_with_schema(".inf\n", Schema::Json);
    assert!(
        matches!(
            result,
            Err(rlsp_yaml_parser::LoadError::UnresolvedScalar { .. })
        ),
        "expected UnresolvedScalar for .inf, got: {result:?}"
    );
}

// IT-41: plain `.nan` → error (JSON float does not include NaN notation)
#[test]
fn json_nan_notation_returns_unresolved_scalar_error() {
    let result = load_with_schema(".nan\n", Schema::Json);
    assert!(
        matches!(
            result,
            Err(rlsp_yaml_parser::LoadError::UnresolvedScalar { .. })
        ),
        "expected UnresolvedScalar for .nan, got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Group 12 — JSON schema: non-plain styles → !!str, no error (IT-42)
// ---------------------------------------------------------------------------

// IT-42: quoted `"hello"` → !!str (no error — only plain scalars go through JSON patterns)
#[test]
fn json_double_quoted_string_resolves_to_str() {
    let tag = scalar_tag("\"hello\"\n", Schema::Json);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// ---------------------------------------------------------------------------
// Group 13 — JSON schema: collections (IT-43, IT-44)
// ---------------------------------------------------------------------------

// IT-43: untagged sequence → !!seq under JSON schema
#[test]
fn json_untagged_sequence_resolves_to_seq() {
    let tag = sequence_tag("[1, 2]\n", Schema::Json);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:seq"));
}

// IT-44: untagged mapping → !!map under JSON schema
#[test]
fn json_untagged_mapping_resolves_to_map() {
    let tag = mapping_tag("{\"a\": 1}\n", Schema::Json);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:map"));
}

// ---------------------------------------------------------------------------
// Group 14 — JSON schema: empty document and edge cases (IT-45, IT-46)
// ---------------------------------------------------------------------------

// IT-45 (spike): empty document under JSON schema — the empty-scalar path in
// loader.rs must thread the schema error back to the caller correctly.
// An empty plain scalar does not match any JSON pattern → UnresolvedScalar.
#[test]
fn json_empty_document_returns_unresolved_scalar_error() {
    // `---\n` produces an empty document whose root is an empty plain scalar.
    let result = load_with_schema("---\n", Schema::Json);
    assert!(
        matches!(
            result,
            Err(rlsp_yaml_parser::LoadError::UnresolvedScalar { .. })
        ),
        "expected UnresolvedScalar for empty doc under JSON schema, got: {result:?}"
    );
}

// IT-46: negative zero `-0` → !!float under JSON schema (matches float pattern)
#[test]
fn json_negative_zero_resolves_to_float() {
    let tag = scalar_tag("-0\n", Schema::Json);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:float"));
}

// ---------------------------------------------------------------------------
// Group 15 — Failsafe schema: all scalar styles → !!str (IT-47–IT-50)
// ---------------------------------------------------------------------------

// IT-47: Failsafe schema: plain `42` → !!str (not !!int)
#[test]
fn failsafe_plain_int_resolves_to_str() {
    let tag = scalar_tag("42\n", Schema::Failsafe);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// IT-48: Failsafe schema: plain `true` → !!str (not !!bool)
#[test]
fn failsafe_plain_bool_resolves_to_str() {
    let tag = scalar_tag("true\n", Schema::Failsafe);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// IT-49: Failsafe schema: quoted `"hello"` → !!str
#[test]
fn failsafe_quoted_string_resolves_to_str() {
    let tag = scalar_tag("\"hello\"\n", Schema::Failsafe);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// IT-50: Failsafe schema: plain `null` → !!str (not !!null)
#[test]
fn failsafe_plain_null_resolves_to_str() {
    let tag = scalar_tag("null\n", Schema::Failsafe);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// ---------------------------------------------------------------------------
// Group 16 — Failsafe schema: collections (IT-51, IT-52)
// ---------------------------------------------------------------------------

// IT-51: Failsafe schema: untagged sequence → !!seq
#[test]
fn failsafe_untagged_sequence_resolves_to_seq() {
    let tag = sequence_tag("- a\n- b\n", Schema::Failsafe);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:seq"));
}

// IT-52: Failsafe schema: untagged mapping → !!map
#[test]
fn failsafe_untagged_mapping_resolves_to_map() {
    let tag = mapping_tag("a: 1\n", Schema::Failsafe);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:map"));
}

// ---------------------------------------------------------------------------
// Group 17 — Failsafe schema: bare `!` tag (IT-53, IT-54)
// ---------------------------------------------------------------------------

// IT-53: Failsafe schema: bare `!` on a plain scalar → !!str (non-specific scalar tag)
#[test]
fn failsafe_bare_excl_on_scalar_resolves_to_str() {
    let tag = scalar_tag("! 42\n", Schema::Failsafe);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:str"));
}

// IT-54: Failsafe schema: bare `!` on a sequence → !!seq
#[test]
fn failsafe_bare_excl_on_sequence_resolves_to_seq() {
    let tag = sequence_tag("! [a, b]\n", Schema::Failsafe);
    assert_eq!(tag.as_deref(), Some("tag:yaml.org,2002:seq"));
}

// ---------------------------------------------------------------------------
// Group 18 — JSON schema: error propagation through nested structures (IT-55, IT-56)
// ---------------------------------------------------------------------------

// IT-55: UnresolvedScalar propagates from a plain scalar nested inside a sequence
// (confirms `?` wiring in the sequence arm of parse_node).
#[test]
fn json_unresolved_scalar_propagates_from_nested_sequence_item() {
    let result = load_with_schema("[1, hello, 3]\n", Schema::Json);
    assert!(
        matches!(
            result,
            Err(rlsp_yaml_parser::LoadError::UnresolvedScalar { .. })
        ),
        "expected UnresolvedScalar propagating from sequence, got: {result:?}"
    );
}

// IT-56: UnresolvedScalar propagates from a plain scalar nested inside a mapping value
// (confirms `?` wiring in the mapping arm of parse_node).
#[test]
fn json_unresolved_scalar_propagates_from_nested_mapping_value() {
    let result = load_with_schema("{\"key\": hello}\n", Schema::Json);
    assert!(
        matches!(
            result,
            Err(rlsp_yaml_parser::LoadError::UnresolvedScalar { .. })
        ),
        "expected UnresolvedScalar propagating from mapping value, got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Group 19 — LoadError::UnresolvedScalar field verification (IT-57–IT-59)
// ---------------------------------------------------------------------------

// IT-57: Display message is exactly the specified string
#[test]
fn json_unresolved_scalar_display_message_is_exact() {
    let err = load_with_schema("hello\n", Schema::Json).expect_err("expected error");
    assert_eq!(
        err.to_string(),
        "JSON schema: plain scalar does not match any type pattern"
    );
}

// IT-58: pos field is not always Pos::ORIGIN — a scalar in the second document
// has pos.line > 1.
#[test]
fn json_unresolved_scalar_pos_reflects_actual_position() {
    // `42\n---\nhello\n`: first doc has `42` (resolves to !!int), second doc
    // has `hello` (fails JSON resolution on a later line).
    let err =
        load_with_schema("42\n---\nhello\n", Schema::Json).expect_err("expected UnresolvedScalar");
    let LoadError::UnresolvedScalar { pos, .. } = err else {
        panic!("expected UnresolvedScalar, got: {err:?}");
    };
    assert!(
        pos.line >= 3,
        "pos.line should be >= 3 for a scalar on line 3, got: {pos:?}"
    );
}

// IT-59: value field of UnresolvedScalar contains the failing scalar content
#[test]
fn json_unresolved_scalar_value_field_contains_scalar_content() {
    let err = load_with_schema("hello\n", Schema::Json).expect_err("expected error");
    let LoadError::UnresolvedScalar { value, .. } = err else {
        panic!("expected UnresolvedScalar, got: {err:?}");
    };
    assert_eq!(value, "hello");
}

// ---------------------------------------------------------------------------
// Group 20 — UnresolvedScalar value truncation (IT-60)
// ---------------------------------------------------------------------------

// IT-60: very long plain scalar value is truncated at 128 chars with ellipsis
#[test]
fn json_unresolved_scalar_truncates_long_value() {
    // 200 lowercase 'a' chars — unresolvable under JSON schema.
    let long_value = "a".repeat(200);
    let input = format!("{long_value}\n");
    let err = load_with_schema(&input, Schema::Json).expect_err("expected error");
    let LoadError::UnresolvedScalar { value, .. } = err else {
        panic!("expected UnresolvedScalar");
    };
    assert!(
        value.ends_with("..."),
        "truncated value should end with '...', got: {value:?}"
    );
    // 128 chars of 'a' + 3 chars "..." = 131 bytes total.
    assert_eq!(
        value.len(),
        131,
        "truncated value should be 131 chars, got len={}",
        value.len()
    );
}

// ---------------------------------------------------------------------------
// Group 21 — Cow variant identity: resolver-injected vs user-authored tags
// ---------------------------------------------------------------------------

// COW-IT-1 (spike): resolver-injected scalar tag is Cow::Borrowed
#[test]
fn core_schema_resolver_tag_is_borrowed_scalar() {
    let tag = scalar_tag("42\n", Schema::Core);
    assert!(
        matches!(tag, Some(Cow::Borrowed(_))),
        "resolver-injected tag must be Cow::Borrowed, got: {tag:?}"
    );
}

// COW-IT-2: resolver-injected mapping tag is Cow::Borrowed
#[test]
fn core_schema_resolver_tag_is_borrowed_mapping() {
    let tag = mapping_tag("a: 1\n", Schema::Core);
    assert!(
        matches!(tag, Some(Cow::Borrowed(_))),
        "resolver-injected mapping tag must be Cow::Borrowed, got: {tag:?}"
    );
}

// COW-IT-3: resolver-injected sequence tag is Cow::Borrowed
#[test]
fn core_schema_resolver_tag_is_borrowed_sequence() {
    let tag = sequence_tag("- a\n", Schema::Core);
    assert!(
        matches!(tag, Some(Cow::Borrowed(_))),
        "resolver-injected sequence tag must be Cow::Borrowed, got: {tag:?}"
    );
}

// COW-IT-4: explicit user-authored tag is Cow::Owned
#[test]
fn explicit_user_tag_is_owned() {
    let tag = scalar_tag("!!str hello\n", Schema::Core);
    assert!(
        matches!(tag, Some(Cow::Owned(_))),
        "user-authored tag must be Cow::Owned, got: {tag:?}"
    );
}

// COW-IT-5: explicit user-authored tag value matches after Deref coercion
#[test]
fn explicit_user_tag_value_matches() {
    let tag = scalar_tag("!!str hello\n", Schema::Core);
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:str"),
        "user-authored !!str must expand to the full URI"
    );
}

// COW-IT-6: all resolver-injected tags in a nested structure are Cow::Borrowed
#[test]
fn resolver_tags_not_reallocated_in_nested_structure() {
    let docs = LoaderBuilder::new()
        .schema(Schema::Core)
        .build()
        .load("numbers:\n  - 1\n  - 3.14\n")
        .expect("load failed");
    let Node::Mapping {
        entries,
        tag: map_tag,
        ..
    } = &docs[0].root
    else {
        panic!("expected root mapping");
    };
    assert!(
        matches!(map_tag, Some(Cow::Borrowed(_))),
        "mapping tag must be Borrowed"
    );
    let (_key, value) = &entries[0];
    let Node::Sequence {
        items,
        tag: seq_tag,
        ..
    } = value
    else {
        panic!("expected sequence value");
    };
    assert!(
        matches!(seq_tag, Some(Cow::Borrowed(_))),
        "sequence tag must be Borrowed"
    );
    for item in items {
        let Node::Scalar { tag, .. } = item else {
            panic!("expected scalar item");
        };
        assert!(
            matches!(tag, Some(Cow::Borrowed(_))),
            "scalar item tag must be Borrowed, got: {tag:?}"
        );
    }
}

// COW-IT-7: mixed explicit and resolver tags in the same mapping
#[test]
fn mixed_explicit_and_resolver_tags_in_same_mapping() {
    let docs = LoaderBuilder::new()
        .schema(Schema::Core)
        .build()
        .load("a: !!str 1\nb: 2\n")
        .expect("load failed");
    let Node::Mapping { entries, .. } = &docs[0].root else {
        panic!("expected root mapping");
    };
    // First entry: value has user-authored !!str
    let Node::Scalar { tag: tag_a, .. } = &entries[0].1 else {
        panic!("expected scalar value for key a");
    };
    assert!(
        matches!(tag_a, Some(Cow::Owned(_))),
        "user-authored !!str must be Cow::Owned, got: {tag_a:?}"
    );
    // Second entry: value gets resolver-injected !!int
    let Node::Scalar { tag: tag_b, .. } = &entries[1].1 else {
        panic!("expected scalar value for key b");
    };
    assert!(
        matches!(tag_b, Some(Cow::Borrowed(_))),
        "resolver-injected !!int must be Cow::Borrowed, got: {tag_b:?}"
    );
}
