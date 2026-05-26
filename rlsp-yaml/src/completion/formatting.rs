// SPDX-License-Identifier: MIT

use crate::schema::{JsonSchema, SchemaType};

use super::support::{MAX_DESCRIPTION_LEN, MAX_ENUM_LABEL_LEN};

/// Convert a `serde_json::Value` to a YAML scalar label string.
/// Returns `None` for values that have no natural YAML scalar representation
/// (arrays, objects).
pub(super) fn json_value_to_yaml_label(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Null => Some("null".to_string()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => None,
    }
}

/// Return the type label string for a schema (e.g. `"string"`, `"integer"`),
/// or `None` if no type is defined.
pub(super) fn type_label(schema: &JsonSchema) -> Option<String> {
    match &schema.schema_type {
        Some(SchemaType::Single(t)) => Some(t.clone()),
        Some(SchemaType::Multiple(ts)) => Some(ts.join(" | ")),
        None => None,
    }
}

/// Truncate a description so the result (including ellipsis) is at most
/// `MAX_DESCRIPTION_LEN` Unicode characters.
pub(super) fn truncate_description(desc: &str) -> String {
    if desc.chars().count() <= MAX_DESCRIPTION_LEN {
        return desc.to_string();
    }
    // Keep MAX_DESCRIPTION_LEN-1 chars, then append "…" (1 char) = MAX_DESCRIPTION_LEN total.
    let keep = MAX_DESCRIPTION_LEN - 1;
    let boundary = desc.char_indices().nth(keep).map_or(desc.len(), |(i, _)| i);
    format!("{}…", &desc[..boundary])
}

/// Truncate an enum label so the result (including ellipsis) is at most
/// `MAX_ENUM_LABEL_LEN` Unicode characters.
pub(super) fn truncate_enum_label(label: &str) -> String {
    if label.chars().count() <= MAX_ENUM_LABEL_LEN {
        return label.to_string();
    }
    // Keep MAX_ENUM_LABEL_LEN-1 chars, then append "…" (1 char) = MAX_ENUM_LABEL_LEN total.
    let keep = MAX_ENUM_LABEL_LEN - 1;
    let boundary = label
        .char_indices()
        .nth(keep)
        .map_or(label.len(), |(i, _)| i);
    format!("{}…", &label[..boundary])
}

#[cfg(test)]
mod tests {
    use crate::schema::{JsonSchema, SchemaType};

    use super::{json_value_to_yaml_label, truncate_description, truncate_enum_label, type_label};

    // ── truncate_description ─────────────────────────────────────────────────

    #[test]
    fn truncate_description_short_string_unchanged() {
        let s = "hello";
        assert_eq!(truncate_description(s), s);
    }

    #[test]
    fn truncate_description_exactly_200_chars_unchanged() {
        let s = "a".repeat(200);
        assert_eq!(truncate_description(&s), s);
    }

    #[test]
    fn truncate_description_201_chars_truncated_to_200_with_ellipsis() {
        let s = "a".repeat(201);
        let result = truncate_description(&s);
        assert_eq!(result.chars().count(), 200);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn truncate_description_multibyte_unicode_boundary_preserved() {
        // Each "é" is 2 bytes. Put one at position 200 (0-indexed = char 199 is last kept char,
        // char 200 is the first dropped). Fill 199 ASCII chars + "é" at position 199 + more.
        let s = format!("{}é{}", "a".repeat(199), "b".repeat(10));
        // s has 210 chars total: 199 'a' + 1 'é' + 10 'b'
        // truncate_description keeps 199 chars, appends "…"
        let result = truncate_description(&s);
        assert_eq!(result.chars().count(), 200);
        assert!(result.ends_with('…'));
        // Verify the byte boundary wasn't split mid-UTF-8
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    // ── truncate_enum_label ──────────────────────────────────────────────────

    #[test]
    fn truncate_enum_label_short_string_unchanged() {
        let s = "hello";
        assert_eq!(truncate_enum_label(s), s);
    }

    #[test]
    fn truncate_enum_label_exactly_50_chars_unchanged() {
        let s = "a".repeat(50);
        assert_eq!(truncate_enum_label(&s), s);
    }

    #[test]
    fn truncate_enum_label_51_chars_truncated_to_50_with_ellipsis() {
        let s = "a".repeat(51);
        let result = truncate_enum_label(&s);
        assert_eq!(result.chars().count(), 50);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn truncate_enum_label_multibyte_unicode_boundary_preserved() {
        // 49 ASCII chars + "é" at position 49 + more chars
        let s = format!("{}é{}", "a".repeat(49), "b".repeat(10));
        // s has 60 chars: 49 'a' + 1 'é' + 10 'b'
        // truncate_enum_label keeps 49 chars, appends "…"
        let result = truncate_enum_label(&s);
        assert_eq!(result.chars().count(), 50);
        assert!(result.ends_with('…'));
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    // ── json_value_to_yaml_label ─────────────────────────────────────────────

    #[test]
    fn json_value_string_returns_that_string() {
        let v = serde_json::Value::String("hello".to_string());
        assert_eq!(json_value_to_yaml_label(&v), Some("hello".to_string()));
    }

    #[test]
    fn json_value_bool_true_returns_true_string() {
        assert_eq!(
            json_value_to_yaml_label(&serde_json::Value::Bool(true)),
            Some("true".to_string())
        );
    }

    #[test]
    fn json_value_bool_false_returns_false_string() {
        assert_eq!(
            json_value_to_yaml_label(&serde_json::Value::Bool(false)),
            Some("false".to_string())
        );
    }

    #[test]
    fn json_value_integer_returns_number_string() {
        let v = serde_json::Value::Number(42.into());
        assert_eq!(json_value_to_yaml_label(&v), Some("42".to_string()));
    }

    #[test]
    #[expect(
        clippy::approx_constant,
        reason = "3.14 is a test value, not an approximation of PI"
    )]
    fn json_value_float_returns_number_string() {
        let v = serde_json::Value::Number(serde_json::Number::from_f64(3.14).unwrap());
        let result = json_value_to_yaml_label(&v);
        assert!(result.is_some());
        assert!(result.unwrap().starts_with("3.14"));
    }

    #[test]
    fn json_value_null_returns_null_string() {
        assert_eq!(
            json_value_to_yaml_label(&serde_json::Value::Null),
            Some("null".to_string())
        );
    }

    #[test]
    fn json_value_array_returns_none() {
        let v = serde_json::json!(["a", "b"]);
        assert_eq!(json_value_to_yaml_label(&v), None);
    }

    #[test]
    fn json_value_object_returns_none() {
        let v = serde_json::json!({"k": "v"});
        assert_eq!(json_value_to_yaml_label(&v), None);
    }

    // ── type_label ───────────────────────────────────────────────────────────

    #[test]
    fn type_label_single_type_returns_that_string() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            ..JsonSchema::default()
        };
        assert_eq!(type_label(&schema), Some("string".to_string()));
    }

    #[test]
    fn type_label_multiple_types_returns_pipe_separated() {
        let schema = JsonSchema {
            schema_type: Some(SchemaType::Multiple(vec![
                "string".to_string(),
                "null".to_string(),
            ])),
            ..JsonSchema::default()
        };
        assert_eq!(type_label(&schema), Some("string | null".to_string()));
    }

    #[test]
    fn type_label_no_type_returns_none() {
        let schema = JsonSchema {
            schema_type: None,
            ..JsonSchema::default()
        };
        assert_eq!(type_label(&schema), None);
    }
}
