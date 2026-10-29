// SPDX-License-Identifier: MIT

//! Reusable fixture generators for benchmarks.
//!
//! All generators are deterministic — same inputs produce the same output.

#![expect(
    dead_code,
    reason = "bench fixture compiled into multiple binaries; each uses a subset"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use rlsp_yaml::schema::{JsonSchema, ParsedSchema, SchemaType};
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

/// Build a `ParsedSchema` with Kubernetes-like complexity.
///
/// - `properties` top-level properties
/// - `depth` levels of nested object schemas
///
/// Each property at every level includes: description, type, required fields,
/// enum values, pattern constraint, and allOf branches — matching real-world
/// complexity found in Kubernetes CRD schemas.
#[must_use]
pub fn generate_schema(properties: usize, depth: usize) -> ParsedSchema {
    ParsedSchema::new(build_object_schema(properties, depth, 0))
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
        additional_properties: Some(Box::new(JsonSchema::always_failing())),
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

// ──────────────────────────────────────────────────────────────────────────────
// Schema association generators
// ──────────────────────────────────────────────────────────────────────────────

/// Entries in the synthetic `SchemaStore` catalog.
pub const SCHEMASTORE_ENTRIES: usize = 700;

/// The entry that [`schemastore_late_match`] resolves to, in the last fifth of
/// the catalog.
const SCHEMASTORE_LATE_ENTRY: usize = 697;

/// URL of catalog entry `index`; the zero-padded index keeps URL order equal
/// to catalog order.
fn schemastore_url(index: usize) -> String {
    format!("https://json.schemastore.org/svc{index:04}.json")
}

/// Entry that carries the one `!` pattern, next to its own `**/svc0349.yaml`
/// pattern so that the exclusion has something to exclude.
const SCHEMASTORE_EXCLUSION_ENTRY: usize = 349;

/// Entry that carries the one pattern with a leading `/`.
const SCHEMASTORE_ROOTED_ENTRY: usize = 351;

/// Patterns that follow an entry's YAML and JSON patterns: the shapes that
/// cost more once the catalog is matched by the upstream rules, because they
/// have no leading `**/` to reject a path early or are not YAML-named.
fn schemastore_extra_patterns(i: usize) -> Vec<String> {
    let shape = match i % 5 {
        0 => Some(format!("deploy{i:04}/*.yml")),
        1 => Some(format!(".svc{i:04}rc")),
        2 => Some(format!("*.svc{i:04}.toml")),
        _ => None,
    };
    let exclusion = (i == SCHEMASTORE_EXCLUSION_ENTRY).then(|| format!("!legacy/svc{i:04}.yaml"));
    let rooted = (i == SCHEMASTORE_ROOTED_ENTRY).then(|| format!("/deploy/svc{i:04}.yml"));
    shape.into_iter().chain(exclusion).chain(rooted).collect()
}

/// A synthetic `SchemaStore` catalog in the shape the real one has.
///
/// 700 entries hold 1 500 plain YAML patterns (three in the first 100 entries,
/// two in the rest) and 20 JSON patterns (one in each of the first 20 entries).
/// A plain YAML pattern's shape follows `(entry + position) % 6`, so each shape
/// of the real catalog is present. Of every five entries, the first three add a
/// `<dir>/*.yml` pattern, a `.<tool>rc` pattern, and a `*.toml` pattern in
/// turn, and two entries add the one `!` pattern and the one pattern with a
/// leading `/`.
/// Every pattern names its entry's index, so no two entries match the same
/// path and no generic pattern can match one of the bench paths below.
#[must_use]
pub fn schemastore_catalog_json() -> Value {
    let schemas: Vec<Value> = (0..SCHEMASTORE_ENTRIES)
        .map(|i| {
            let yaml_count = if i < 100 { 3 } else { 2 };
            let mut file_match: Vec<String> = (0..yaml_count)
                .map(|j| match (i + j) % 6 {
                    0 => format!("**/.github/workflows/svc{i:04}-*.yml"),
                    1 => format!("**/svc{i:04}.yaml"),
                    2 => format!("**/*.svc{i:04}.yml"),
                    3 => format!("svc{i:04}.yml"),
                    4 => format!("config{i:04}/**/svc{i:04}.yaml"),
                    _ => format!("*.svc{i:04}.yaml"),
                })
                .collect();
            if i < 20 {
                file_match.push(format!("**/svc{i:04}.json"));
            }
            file_match.extend(schemastore_extra_patterns(i));
            serde_json::json!({
                "name": format!("Service {i:04}"),
                "url": schemastore_url(i),
                "fileMatch": file_match,
            })
        })
        .collect();
    let catalog = serde_json::json!({ "schemas": schemas });
    assert_catalog_shape(&catalog);
    catalog
}

/// The kind of a catalog pattern, as the shape check counts them.
fn schemastore_pattern_kind(pattern: &str) -> &'static str {
    let extension = pattern.rsplit('.').next();
    if pattern.starts_with('!') {
        "exclusion"
    } else if pattern.starts_with('/') {
        "rooted"
    } else if matches!(extension, Some("json" | "jsonc" | "json5")) {
        "json"
    } else if pattern.starts_with("deploy") && pattern.ends_with("/*.yml") {
        "directory_yml"
    } else if matches!(extension, Some("yaml" | "yml")) {
        "yaml"
    } else {
        "other"
    }
}

/// Panic unless `catalog` has the documented 700 entries and pattern counts,
/// each entry holding at least one plain YAML pattern and every pattern naming
/// its entry's index.
#[expect(
    clippy::expect_used,
    reason = "bench setup; a malformed fixture must stop the run"
)]
fn assert_catalog_shape(catalog: &Value) {
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    let entries = catalog["schemas"].as_array().expect("schemas is an array");
    assert_eq!(entries.len(), SCHEMASTORE_ENTRIES);
    for (i, entry) in entries.iter().enumerate() {
        let patterns: Vec<&str> = entry["fileMatch"]
            .as_array()
            .expect("fileMatch is an array")
            .iter()
            .map(|p| p.as_str().expect("fileMatch holds strings"))
            .collect();
        let index = format!("{i:04}");
        assert!(
            patterns.iter().all(|p| p.contains(&index)),
            "every pattern names its entry"
        );
        let entry_kinds: Vec<&str> = patterns
            .iter()
            .map(|p| schemastore_pattern_kind(p))
            .collect();
        assert!(
            entry_kinds.contains(&"yaml"),
            "every entry keeps a plain YAML pattern"
        );
        for kind in entry_kinds {
            *kinds.entry(kind).or_default() += 1;
        }
    }
    assert_eq!(
        kinds,
        BTreeMap::from([
            ("directory_yml", 140),
            ("exclusion", 1),
            ("json", 20),
            ("other", 280),
            ("rooted", 1),
            ("yaml", 1_500),
        ])
    );
}

/// A path that only the last fifth of the catalog matches, and the URL of the
/// entry it resolves to.
#[must_use]
pub fn schemastore_late_match() -> (String, String) {
    (
        format!("/workspace/projects/app/svc{SCHEMASTORE_LATE_ENTRY:04}.yaml"),
        schemastore_url(SCHEMASTORE_LATE_ENTRY),
    )
}

/// An absolute YAML path that no catalog entry matches.
#[must_use]
pub fn schemastore_no_match_path() -> String {
    "/workspace/projects/app/config.yaml".to_string()
}

/// An absolute YAML path of exactly 200 bytes that no catalog entry matches.
#[must_use]
pub fn schemastore_deep_path() -> String {
    const TAIL: &str = "/config.yaml";
    const SEGMENT_BYTES: usize = 4;
    let mut path = String::new();
    for i in 0..(200 - TAIL.len()) / SEGMENT_BYTES {
        let _ = write!(path, "/d{:02}", i % 100);
    }
    path.push_str(TAIL);
    path
}

/// An absolute `.yml` path that no catalog entry matches. It ends in `.yml`,
/// the trailing literal of the `<dir>/*.yml` patterns, so a trailing-literal
/// check does not reject those patterns before they run. Every pattern names
/// its entry's index and this path names none, so no rule that reads the
/// patterns can match it.
#[must_use]
pub fn schemastore_yml_no_match_path() -> String {
    "/workspace/projects/app/config.yml".to_string()
}

/// A path that only a JSON-family catalog pattern names.
#[must_use]
pub fn schemastore_json_only_path() -> String {
    "/workspace/projects/app/svc0003.json".to_string()
}

/// Entries in the synthetic workspace `schemas` setting.
const WORKSPACE_ENTRIES: usize = 40;

/// A synthetic workspace `schemas` setting: URL and glob list per entry.
///
/// Each entry has a directory glob, an extension glob, a deep glob, and an
/// exclusion. The zero-padded index keeps URL order equal to entry order.
#[must_use]
pub fn workspace_schemas() -> Vec<(String, Vec<String>)> {
    (0..WORKSPACE_ENTRIES)
        .map(|i| {
            (
                format!("https://example.com/ws/schema{i:03}.json"),
                vec![
                    format!("config/svc{i:03}-*.yaml"),
                    format!("*.svc{i:03}.yml"),
                    format!("/deploy/svc{i:03}/**/*.yaml"),
                    format!("!**/skip-svc{i:03}.yaml"),
                ],
            )
        })
        .collect()
}

/// A path that the last workspace entry matches, and its URL.
#[must_use]
pub fn workspace_match() -> (String, String) {
    let last = WORKSPACE_ENTRIES - 1;
    (
        format!("/workspace/config/svc{last:03}-prod.yaml"),
        format!("https://example.com/ws/schema{last:03}.json"),
    )
}

/// A path that no workspace entry matches.
#[must_use]
pub fn workspace_no_match_path() -> String {
    "/workspace/projects/app/other/config.yaml".to_string()
}
