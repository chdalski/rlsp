// SPDX-License-Identifier: MIT

//! Reusable fixture generators for benchmarks.
//!
//! All generators are deterministic — same inputs produce the same output.

#![expect(
    dead_code,
    reason = "bench fixture compiled into multiple binaries; each uses a subset"
)]

use std::fmt::Write as _;

use rlsp_yaml::schema::{AdditionalProperties, JsonSchema, SchemaType};
use serde_json::Value;

// ──────────────────────────────────────────────────────────────────────────────
// Size presets
// ──────────────────────────────────────────────────────────────────────────────

/// 20 lines of flat key-value YAML.
pub fn tiny() -> String {
    generate_yaml(20)
}

/// 500 lines of flat key-value YAML.
pub fn medium() -> String {
    generate_yaml(500)
}

/// 2 000 lines of flat key-value YAML.
pub fn large() -> String {
    generate_yaml(2_000)
}

/// 10 000 lines of flat key-value YAML.
pub fn huge() -> String {
    generate_yaml(10_000)
}

/// Deeply nested YAML — depth 20, width 3.
///
/// Tests Wadler-Lindig depth sensitivity in the formatter.
pub fn deeply_nested() -> String {
    generate_nested_yaml(20, 3)
}

// ──────────────────────────────────────────────────────────────────────────────
// YAML generators
// ──────────────────────────────────────────────────────────────────────────────

/// Flat key-value YAML with `lines` entries.
///
/// ```yaml
/// key0: value0
/// key1: value1
/// ...
/// ```
#[must_use]
pub fn generate_yaml(lines: usize) -> String {
    let mut out = String::with_capacity(lines * 16);
    for i in 0..lines {
        let _ = writeln!(out, "key{i}: value{i}");
    }
    out
}

/// Nested mapping YAML with given depth and width.
///
/// At each level, `width` keys are emitted: `(width - 1)` leaf values and one
/// nested child mapping that continues to `depth` levels total.
#[must_use]
pub fn generate_nested_yaml(depth: usize, width: usize) -> String {
    let mut out = String::new();
    write_nested(&mut out, depth, width, 0);
    out
}

fn write_nested(out: &mut String, depth: usize, width: usize, current: usize) {
    if current >= depth {
        return;
    }
    let indent = "  ".repeat(current);
    // Emit sibling leaf keys at this level
    for i in 0..width.saturating_sub(1) {
        let _ = writeln!(out, "{indent}prop{current}_{i}: leaf_value");
    }
    // Emit one child mapping that recurses
    let _ = writeln!(out, "{indent}nested{current}:");
    write_nested(out, depth, width, current + 1);
}

/// YAML document with `anchor_count` anchors and one alias each.
///
/// ```yaml
/// anchor0: &anchor0 value0
/// alias0: *anchor0
/// ...
/// ```
#[must_use]
pub fn generate_anchor_yaml(anchor_count: usize) -> String {
    let mut out = String::with_capacity(anchor_count * 40);
    for i in 0..anchor_count {
        let _ = writeln!(out, "anchor{i}: &anchor{i} value{i}");
        let _ = writeln!(out, "alias{i}: *anchor{i}");
    }
    out
}

// ──────────────────────────────────────────────────────────────────────────────
// Schema generator
// ──────────────────────────────────────────────────────────────────────────────

/// Build a `JsonSchema` with Kubernetes-like complexity.
///
/// - `properties` top-level properties
/// - `depth` levels of nested object schemas
///
/// Each property at every level includes: description, type, required fields,
/// enum values, pattern constraint, and allOf branches — matching real-world
/// complexity found in Kubernetes CRD schemas.
#[must_use]
pub fn generate_schema(properties: usize, depth: usize) -> JsonSchema {
    build_object_schema(properties, depth, 0)
}

fn build_object_schema(properties: usize, depth: usize, current_depth: usize) -> JsonSchema {
    let mut props = std::collections::HashMap::new();
    let mut required = Vec::new();

    for i in 0..properties {
        let name = format!("prop{i}");
        let schema = if current_depth < depth {
            build_object_schema(properties / 2 + 1, depth, current_depth + 1)
        } else {
            build_leaf_schema(i)
        };
        if i % 3 == 0 {
            required.push(name.clone());
        }
        props.insert(name, schema);
    }

    // allOf with two branches for schema composition complexity
    let all_of = if current_depth == 0 {
        Some(vec![
            JsonSchema {
                description: Some("allOf branch A — metadata constraints".to_string()),
                required: Some(vec!["name".to_string()]),
                ..JsonSchema::default()
            },
            JsonSchema {
                description: Some("allOf branch B — spec constraints".to_string()),
                required: Some(vec!["spec".to_string()]),
                ..JsonSchema::default()
            },
        ])
    } else {
        None
    };

    JsonSchema {
        schema_type: Some(SchemaType::Single("object".to_string())),
        description: Some(format!("Generated object schema at depth {current_depth}")),
        properties: Some(props),
        required: Some(required),
        additional_properties: Some(AdditionalProperties::Denied),
        all_of,
        ..JsonSchema::default()
    }
}

fn build_leaf_schema(index: usize) -> JsonSchema {
    match index % 5 {
        0 => JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            description: Some(format!("String property {index}")),
            enum_values: Some(vec![
                Value::String("alpha".to_string()),
                Value::String("beta".to_string()),
                Value::String("gamma".to_string()),
            ]),
            ..JsonSchema::default()
        },
        1 => JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            description: Some(format!("Pattern-constrained string {index}")),
            pattern: Some("^[a-z][a-z0-9-]*$".to_string()),
            min_length: Some(1),
            max_length: Some(63),
            ..JsonSchema::default()
        },
        2 => JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            description: Some(format!("Integer property {index}")),
            minimum: Some(0.0),
            maximum: Some(65535.0),
            ..JsonSchema::default()
        },
        3 => JsonSchema {
            schema_type: Some(SchemaType::Single("boolean".to_string())),
            description: Some(format!("Boolean flag {index}")),
            ..JsonSchema::default()
        },
        _ => JsonSchema {
            schema_type: Some(SchemaType::Single("array".to_string())),
            description: Some(format!("Array property {index}")),
            items: Some(Box::new(JsonSchema {
                schema_type: Some(SchemaType::Single("string".to_string())),
                ..JsonSchema::default()
            })),
            min_items: Some(1),
            max_items: Some(10),
            ..JsonSchema::default()
        },
    }
}
