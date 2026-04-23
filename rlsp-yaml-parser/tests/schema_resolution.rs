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
//! `load_with_schema` and `LoaderBuilder::new().lossless().schema(s).build().load(input)`.

use rlsp_yaml_parser::loader::{LoaderBuilder, load_with_schema};
use rlsp_yaml_parser::{Node, Schema};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the tag from the root scalar of a single-document YAML string
/// loaded with the given schema.
fn scalar_tag(input: &str, schema: Schema) -> Option<String> {
    let docs = load_with_schema(input, schema).expect("load failed");
    assert_eq!(docs.len(), 1, "expected exactly one document");
    match &docs[0].root {
        Node::Scalar { tag, .. } => tag.clone(),
        other => panic!("expected root scalar, got: {other:?}"),
    }
}

/// Extract the tag from the root mapping of a single-document YAML string.
fn mapping_tag(input: &str, schema: Schema) -> Option<String> {
    let docs = load_with_schema(input, schema).expect("load failed");
    assert_eq!(docs.len(), 1, "expected exactly one document");
    match &docs[0].root {
        Node::Mapping { tag, .. } => tag.clone(),
        other => panic!("expected root mapping, got: {other:?}"),
    }
}

/// Extract the tag from the root sequence of a single-document YAML string.
fn sequence_tag(input: &str, schema: Schema) -> Option<String> {
    let docs = load_with_schema(input, schema).expect("load failed");
    assert_eq!(docs.len(), 1, "expected exactly one document");
    match &docs[0].root {
        Node::Sequence { tag, .. } => tag.clone(),
        other => panic!("expected root sequence, got: {other:?}"),
    }
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
// Group 8 — Regression: default `load()` leaves all tags None (IT-25–IT-27)
// ---------------------------------------------------------------------------

// IT-25: default load() — plain integer tag stays None
#[test]
fn no_schema_plain_integer_tag_is_none() {
    let docs = rlsp_yaml_parser::loader::load("42\n").expect("load failed");
    let Node::Scalar { tag, .. } = &docs[0].root else {
        panic!("expected scalar");
    };
    assert_eq!(*tag, None, "default load must not resolve tags");
}

// IT-26: default load() — plain `null` tag stays None
#[test]
fn no_schema_plain_null_tag_is_none() {
    let docs = rlsp_yaml_parser::loader::load("null\n").expect("load failed");
    let Node::Scalar { tag, .. } = &docs[0].root else {
        panic!("expected scalar");
    };
    assert_eq!(*tag, None, "default load must not resolve tags");
}

// IT-27: default load() — mapping tag stays None
#[test]
fn no_schema_mapping_tag_is_none() {
    let docs = rlsp_yaml_parser::loader::load("a: 1\n").expect("load failed");
    let Node::Mapping { tag, .. } = &docs[0].root else {
        panic!("expected mapping");
    };
    assert_eq!(*tag, None, "default load must not resolve tags");
}

// ---------------------------------------------------------------------------
// Group 9 — `load_with_schema` convenience vs. builder chain equivalence
// (IT-28, IT-29)
// ---------------------------------------------------------------------------

// IT-28: load_with_schema convenience produces same tag as builder chain (scalar)
#[test]
fn load_with_schema_equivalent_to_builder_chain_scalar() {
    let input = "42\n";
    let via_convenience = load_with_schema(input, Schema::Core).expect("convenience failed");
    let via_builder = LoaderBuilder::new()
        .lossless()
        .schema(Schema::Core)
        .build()
        .load(input)
        .expect("builder failed");
    assert_eq!(via_convenience, via_builder);
}

// IT-29: load_with_schema convenience produces same result as builder chain (mapping)
#[test]
fn load_with_schema_equivalent_to_builder_chain_mapping() {
    let input = "a: 1\nb: true\n";
    let via_convenience = load_with_schema(input, Schema::Core).expect("convenience failed");
    let via_builder = LoaderBuilder::new()
        .lossless()
        .schema(Schema::Core)
        .build()
        .load(input)
        .expect("builder failed");
    assert_eq!(via_convenience, via_builder);
}
