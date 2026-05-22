// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::Span;
use rlsp_yaml_parser::node::Node;
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::lsp_util::span_to_lsp;
use crate::schema::{AdditionalProperties, JsonSchema};

use super::context::Ctx;
use super::support::{
    collect_evaluated_item_count, format_path, make_diagnostic, node_loc, yaml_to_json,
};
use super::validate_node;

pub(super) fn validate_unevaluated_items(
    seq: &[Node<Span>],
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let evaluated_count = collect_evaluated_item_count(schema);
    let Some(unevaluated_schema) = &schema.unevaluated_items else {
        return;
    };
    for (i, item) in seq.iter().enumerate() {
        if evaluated_count == usize::MAX || i < evaluated_count {
            continue;
        }
        let mut item_path = path.to_vec();
        item_path.push(format!("[{i}]"));
        validate_node(item, unevaluated_schema, &item_path, ctx, depth + 1);
    }
}

pub(super) fn validate_sequence(
    seq: &[Node<Span>],
    seq_loc: Span,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    // prefixItems — validate each element at its positional schema
    let prefix_len = schema.prefix_items.as_ref().map_or(0, Vec::len);
    if let Some(prefix_schemas) = &schema.prefix_items {
        for (i, (item, item_schema)) in seq.iter().zip(prefix_schemas.iter()).enumerate() {
            let mut item_path = path.to_vec();
            item_path.push(format!("[{i}]"));
            validate_node(item, item_schema, &item_path, ctx, depth + 1);
        }
    }
    // items — applies to elements beyond prefixItems
    if let Some(items_schema) = &schema.items {
        for (i, item) in seq.iter().enumerate().skip(prefix_len) {
            let mut item_path = path.to_vec();
            item_path.push(format!("[{i}]"));
            validate_node(item, items_schema, &item_path, ctx, depth + 1);
        }
    } else if let Some(additional_items) = &schema.additional_items {
        // additionalItems (Draft-04/07) — applies to elements beyond the tuple prefix
        for (i, item) in seq.iter().enumerate().skip(prefix_len) {
            let mut item_path = path.to_vec();
            item_path.push(format!("[{i}]"));
            match additional_items {
                AdditionalProperties::Denied => {
                    let range = span_to_lsp(node_loc(item), ctx.idx);
                    ctx.diagnostics.push(make_diagnostic(
                        range,
                        DiagnosticSeverity::WARNING,
                        "schemaAdditionalItems",
                        format!(
                            "Additional item at {}[{i}] is not allowed",
                            format_path(path)
                        ),
                    ));
                }
                AdditionalProperties::Schema(extra_schema) => {
                    validate_node(item, extra_schema, &item_path, ctx, depth + 1);
                }
            }
        }
    }
    validate_array_constraints(seq, seq_loc, schema, path, ctx, depth);
}

pub(super) fn validate_array_constraints(
    seq: &[Node<Span>],
    seq_loc: Span,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let len = seq.len() as u64;

    if let Some(min) = schema.min_items {
        if len < min {
            let range = span_to_lsp(seq_loc, ctx.idx);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMinItems",
                format!(
                    "Array at {} has {} items, minimum is {}",
                    format_path(path),
                    len,
                    min
                ),
            ));
        }
    }

    if let Some(max) = schema.max_items {
        if len > max {
            let range = span_to_lsp(seq_loc, ctx.idx);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaMaxItems",
                format!(
                    "Array at {} has {} items, maximum is {}",
                    format_path(path),
                    len,
                    max
                ),
            ));
        }
    }

    if schema.unique_items == Some(true) {
        let json_items: Vec<serde_json::Value> = seq.iter().filter_map(yaml_to_json).collect();
        let has_duplicate = json_items.iter().enumerate().any(|(i, a)| {
            json_items
                .get(..i)
                .is_some_and(|prev| prev.iter().any(|b| a == b))
        });
        if has_duplicate {
            let range = span_to_lsp(seq_loc, ctx.idx);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaUniqueItems",
                format!("Array at {} contains duplicate items", format_path(path)),
            ));
        }
    }

    if let Some(contains_schema) = &schema.contains {
        validate_contains(seq, seq_loc, contains_schema, schema, path, ctx, depth);
    }
}

pub(super) fn validate_contains(
    seq: &[Node<Span>],
    seq_loc: Span,
    contains_schema: &JsonSchema,
    schema: &JsonSchema,
    path: &[String],
    ctx: &mut Ctx<'_>,
    depth: usize,
) {
    let format_validation = ctx.format_validation;
    let yaml_version = ctx.yaml_version;
    let match_count = seq
        .iter()
        .filter(|item| {
            let mut scratch = Vec::new();
            let mut probe = Ctx::new(&mut scratch, format_validation, yaml_version, ctx.idx);
            validate_node(item, contains_schema, path, &mut probe, depth + 1);
            scratch.is_empty()
        })
        .count() as u64;

    // Default min is 1 when `contains` is present without `minContains`
    let effective_min = schema.min_contains.unwrap_or(1);

    if match_count < effective_min {
        let range = span_to_lsp(seq_loc, ctx.idx);
        ctx.diagnostics.push(make_diagnostic(
            range,
            DiagnosticSeverity::ERROR,
            "schemaContains",
            format!(
                "Array at {} must contain at least {} item(s) matching the schema, found {}",
                format_path(path),
                effective_min,
                match_count
            ),
        ));
    }

    if let Some(max) = schema.max_contains {
        if match_count > max {
            let range = span_to_lsp(seq_loc, ctx.idx);
            ctx.diagnostics.push(make_diagnostic(
                range,
                DiagnosticSeverity::ERROR,
                "schemaContains",
                format!(
                    "Array at {} must contain at most {} item(s) matching the schema, found {}",
                    format_path(path),
                    max,
                    match_count
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::{JsonSchema, SchemaType};
    use crate::server::YamlVersion;
    use crate::test_utils::parse_docs;

    use super::super::support::test_fixtures::{code_of, integer_schema, object_schema_with_props};
    use super::super::validate_schema;

    // Test 37
    #[test]
    fn should_validate_array_items_against_items_schema() {
        let schema = object_schema_with_props(vec![(
            "ports",
            JsonSchema {
                schema_type: Some(SchemaType::Single("array".to_string())),
                items: Some(Box::new(integer_schema())),
                ..JsonSchema::default()
            },
        )]);
        let text = "ports:\n  - 8080\n  - not-an-int";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaType");
    }

    // Test 38
    #[test]
    fn should_produce_no_diagnostics_for_valid_array_items() {
        let schema = object_schema_with_props(vec![(
            "ports",
            JsonSchema {
                schema_type: Some(SchemaType::Single("array".to_string())),
                items: Some(Box::new(integer_schema())),
                ..JsonSchema::default()
            },
        )]);
        let text = "ports:\n  - 8080\n  - 9090";
        let docs = parse_docs(text);
        let result = validate_schema(&docs, &schema, true, YamlVersion::V1_2);
        assert!(result.is_empty());
    }
}
