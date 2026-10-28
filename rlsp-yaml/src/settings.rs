// SPDX-License-Identifier: MIT

use std::collections::HashMap;

/// Workspace settings received via LSP initialization options or
/// `workspace/didChangeConfiguration`.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Custom YAML tag names to treat as valid (suppresses unknown-tag diagnostics).
    pub custom_tags: Vec<String>,
    /// Whether to enforce alphabetical key ordering in mappings.
    pub key_ordering: bool,
    /// Maps schema URL → glob pattern (upstream yaml-language-server convention).
    pub schemas: HashMap<String, String>,
    /// Kubernetes cluster version for schema resolution (e.g. `"1.29.0"`).
    /// Defaults to `"master"` (tracks latest schemas) when absent.
    pub kubernetes_version: Option<String>,
    /// Enable `SchemaStore` automatic schema association. Defaults to `true` when absent.
    pub schema_store: Option<bool>,
    /// Maximum line width for the full-document formatter. Defaults to 80.
    pub format_print_width: Option<usize>,
    /// Prefer single-quoted strings. Defaults to false.
    pub format_single_quote: Option<bool>,
    /// Preserve the source quote style of scalars. Defaults to false.
    pub format_preserve_quotes: Option<bool>,
    /// HTTP proxy URL for schema fetching (e.g. `"http://proxy.corp:8080"`).
    /// When absent, no proxy is used.
    pub http_proxy: Option<String>,
    /// Enable color decorators (color picker) for color values. Defaults to `true` when absent.
    pub color_decorators: Option<bool>,
    /// Enable `format` keyword validation. Defaults to `true` when absent.
    pub format_validation: Option<bool>,
    /// YAML specification version for formatting and diagnostics (`"1.1"` or
    /// `"1.2"`). Defaults to `"1.2"` when absent.
    pub yaml_version: Option<String>,
    /// Enable diagnostic validation. Defaults to `true` when absent.
    pub validate: Option<bool>,
    /// Enable hover responses. Defaults to `true` when absent.
    pub hover: Option<bool>,
    /// Enable completion responses. Defaults to `true` when absent.
    pub completion: Option<bool>,
    /// Maximum number of items returned by document symbols and folding ranges.
    /// Defaults to 5000 when absent.
    pub max_items_computed: Option<usize>,
    /// Controls severity of flow-style diagnostics: `"off"` disables them,
    /// `"warning"` (default) emits warnings, `"error"` emits errors.
    pub flow_style: Option<String>,
    /// Controls severity of duplicate-key diagnostics: `"off"` disables them,
    /// `"warning"` emits warnings, `"error"` (default) emits errors.
    pub duplicate_keys: Option<String>,
    /// When `true`, the formatter converts all flow collections to block style.
    /// Defaults to `false` when absent.
    pub format_enforce_block_style: Option<bool>,
    /// When `true`, the formatter removes duplicate mapping keys before
    /// rendering, keeping the last occurrence. Defaults to `false` when absent.
    pub format_remove_duplicate_keys: Option<bool>,
    /// Add spaces inside flow braces: `{ a: 1 }` vs `{a: 1}`. Defaults to `true` when absent.
    pub format_bracket_spacing: Option<bool>,
    /// Indent block sequences that are values of mapping keys. Defaults to `true` when absent.
    pub format_indent_sequences: Option<bool>,
    /// Enable the built-in formatter. Defaults to `true` when absent. When `false`, all three
    /// formatting handlers (`textDocument/formatting`, `textDocument/rangeFormatting`,
    /// `textDocument/onTypeFormatting`) return no edits. Capabilities remain advertised —
    /// matches Red Hat yaml-language-server / VS Code HTML/JSON convention; avoids dynamic
    /// capability re-registration.
    pub format_enable: Option<bool>,
    /// When `false`, `.editorconfig` files are not read during formatting. Defaults to `true`.
    pub format_respect_editorconfig: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Settings deserialization ----

    #[test]
    fn settings_reads_every_key_with_a_non_default_value() {
        let json = serde_json::json!({
            "customTags": ["!include"],
            "keyOrdering": true,
            "schemas": {"https://example.com/s.json": "*.yaml"},
            "kubernetesVersion": "1.29.0",
            "schemaStore": false,
            "formatPrintWidth": 100,
            "formatSingleQuote": true,
            "formatPreserveQuotes": true,
            "httpProxy": "http://proxy.corp:8080",
            "colorDecorators": false,
            "formatValidation": false,
            "yamlVersion": "1.1",
            "validate": false,
            "hover": false,
            "completion": false,
            "maxItemsComputed": 10,
            "flowStyle": "error",
            "duplicateKeys": "off",
            "formatEnforceBlockStyle": true,
            "formatRemoveDuplicateKeys": true,
            "formatBracketSpacing": false,
            "formatIndentSequences": false,
            "formatEnable": false,
            "formatRespectEditorconfig": false,
        });
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(
            settings,
            Settings {
                custom_tags: vec!["!include".to_string()],
                key_ordering: true,
                schemas: HashMap::from([(
                    "https://example.com/s.json".to_string(),
                    "*.yaml".to_string()
                )]),
                kubernetes_version: Some("1.29.0".to_string()),
                schema_store: Some(false),
                format_print_width: Some(100),
                format_single_quote: Some(true),
                format_preserve_quotes: Some(true),
                http_proxy: Some("http://proxy.corp:8080".to_string()),
                color_decorators: Some(false),
                format_validation: Some(false),
                yaml_version: Some("1.1".to_string()),
                validate: Some(false),
                hover: Some(false),
                completion: Some(false),
                max_items_computed: Some(10),
                flow_style: Some("error".to_string()),
                duplicate_keys: Some("off".to_string()),
                format_enforce_block_style: Some(true),
                format_remove_duplicate_keys: Some(true),
                format_bracket_spacing: Some(false),
                format_indent_sequences: Some(false),
                format_enable: Some(false),
                format_respect_editorconfig: Some(false),
            }
        );
    }

    #[test]
    fn settings_reads_empty_object_as_default() {
        let settings: Settings = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn settings_deserializes_custom_tags_from_json() {
        let json = serde_json::json!({"customTags": ["!include", "!ref"]});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.custom_tags, vec!["!include", "!ref"]);
    }

    #[test]
    fn settings_defaults_to_empty_custom_tags_when_field_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.custom_tags.is_empty());
    }

    #[test]
    fn settings_accepts_empty_custom_tags_array() {
        let json = serde_json::json!({"customTags": []});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.custom_tags.is_empty());
    }

    #[test]
    fn settings_deserializes_key_ordering_true() {
        let json = serde_json::json!({"keyOrdering": true});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.key_ordering);
    }

    #[test]
    fn settings_defaults_key_ordering_to_false_when_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(!settings.key_ordering);
    }

    // ---- Schemas deserialization ----

    #[test]
    fn settings_deserializes_schemas_from_json() {
        let json = serde_json::json!({"schemas": {"https://example.com/schema.json": "*.yaml"}});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(
            settings
                .schemas
                .get("https://example.com/schema.json")
                .map(String::as_str),
            Some("*.yaml")
        );
    }

    #[test]
    fn settings_defaults_to_empty_schemas_when_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.schemas.is_empty());
    }

    // ---- Kubernetes version setting ----

    #[test]
    fn settings_deserializes_kubernetes_version() {
        let json = serde_json::json!({"kubernetesVersion": "1.29.0"});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.kubernetes_version.as_deref(), Some("1.29.0"));
    }

    #[test]
    fn settings_defaults_kubernetes_version_to_none_when_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.kubernetes_version.is_none());
    }

    #[test]
    fn settings_keeps_invalid_kubernetes_version_raw_and_other_settings() {
        let json = serde_json::json!({"kubernetesVersion": "bad", "keyOrdering": true});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.kubernetes_version.as_deref(), Some("bad"));
        assert!(settings.key_ordering);
    }

    // ---- YAML version setting ----

    #[test]
    fn settings_deserializes_yaml_version() {
        let json = serde_json::json!({"yamlVersion": "1.1"});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.yaml_version.as_deref(), Some("1.1"));
    }

    #[test]
    fn settings_defaults_yaml_version_to_none_when_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.yaml_version.is_none());
    }

    // ---- schemaStore setting ----

    #[test]
    fn settings_deserializes_schema_store_true() {
        let json = serde_json::json!({"schemaStore": true});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.schema_store, Some(true));
    }

    #[test]
    fn settings_deserializes_schema_store_false() {
        let json = serde_json::json!({"schemaStore": false});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.schema_store, Some(false));
    }

    #[test]
    fn settings_defaults_schema_store_to_none_when_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.schema_store, None);
    }

    // ---- Formatting settings ----

    #[test]
    fn settings_deserializes_format_print_width() {
        let json = serde_json::json!({ "formatPrintWidth": 120 });
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.format_print_width, Some(120));
    }

    #[test]
    fn settings_deserializes_format_single_quote() {
        let json = serde_json::json!({ "formatSingleQuote": true });
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.format_single_quote, Some(true));
    }

    #[test]
    fn settings_deserializes_format_preserve_quotes() {
        let json = serde_json::json!({ "formatPreserveQuotes": true });
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.format_preserve_quotes, Some(true));
    }

    #[test]
    fn settings_format_fields_default_to_none_when_absent() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.format_print_width, None);
        assert_eq!(settings.format_single_quote, None);
    }

    // ---- HTTP proxy setting ----

    #[test]
    fn settings_deserializes_http_proxy() {
        let json = serde_json::json!({ "httpProxy": "http://proxy.corp:8080" });
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(
            settings.http_proxy.as_deref(),
            Some("http://proxy.corp:8080")
        );
    }

    #[test]
    fn settings_defaults_http_proxy_to_none_when_absent() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.http_proxy.is_none());
    }

    // ---- colorDecorators setting ----

    #[test]
    fn settings_deserializes_color_decorators_false() {
        let json = serde_json::json!({ "colorDecorators": false });
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.color_decorators, Some(false));
    }

    #[test]
    fn settings_defaults_color_decorators_to_none_when_absent() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.color_decorators.is_none());
    }

    // ---- validate setting ----

    #[test]
    fn settings_deserializes_validate_false() {
        let json = serde_json::json!({"validate": false});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.validate, Some(false));
    }

    #[test]
    fn settings_deserializes_validate_true() {
        let json = serde_json::json!({"validate": true});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.validate, Some(true));
    }

    #[test]
    fn settings_defaults_validate_to_none_when_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.validate.is_none());
    }

    // ---- hover setting ----

    #[test]
    fn settings_deserializes_hover_false() {
        let json = serde_json::json!({"hover": false});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.hover, Some(false));
    }

    #[test]
    fn settings_defaults_hover_to_none_when_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.hover.is_none());
    }

    // ---- completion setting ----

    #[test]
    fn settings_deserializes_completion_false() {
        let json = serde_json::json!({"completion": false});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.completion, Some(false));
    }

    #[test]
    fn settings_defaults_completion_to_none_when_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.completion.is_none());
    }

    // ---- maxItemsComputed setting ----

    #[test]
    fn settings_deserializes_max_items_computed() {
        let json = serde_json::json!({"maxItemsComputed": 100});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.max_items_computed, Some(100));
    }

    #[test]
    fn settings_defaults_max_items_computed_to_none_when_absent() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.max_items_computed.is_none());
    }

    #[test]
    fn settings_accepts_max_items_computed_zero() {
        let json = serde_json::json!({"maxItemsComputed": 0});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.max_items_computed, Some(0));
    }

    // ---- duplicateKeys setting ----

    #[test]
    fn settings_deserializes_duplicate_keys_off() {
        let json = serde_json::json!({"duplicateKeys": "off"});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.duplicate_keys.as_deref(), Some("off"));
    }

    #[test]
    fn settings_deserializes_duplicate_keys_warning() {
        let json = serde_json::json!({"duplicateKeys": "warning"});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.duplicate_keys.as_deref(), Some("warning"));
    }

    #[test]
    fn settings_deserializes_duplicate_keys_error() {
        let json = serde_json::json!({"duplicateKeys": "error"});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.duplicate_keys.as_deref(), Some("error"));
    }

    #[test]
    fn settings_defaults_duplicate_keys_to_none_when_absent() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.duplicate_keys.is_none());
    }

    // ---- formatRemoveDuplicateKeys setting ----

    // K1: formatRemoveDuplicateKeys: true deserializes correctly.
    #[test]
    fn settings_deserializes_format_remove_duplicate_keys_true() {
        let json = serde_json::json!({"formatRemoveDuplicateKeys": true});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.format_remove_duplicate_keys, Some(true));
    }

    // K2: formatRemoveDuplicateKeys: false deserializes correctly.
    #[test]
    fn settings_deserializes_format_remove_duplicate_keys_false() {
        let json = serde_json::json!({"formatRemoveDuplicateKeys": false});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.format_remove_duplicate_keys, Some(false));
    }

    // K3: field absent — defaults to None.
    #[test]
    fn settings_defaults_format_remove_duplicate_keys_to_none_when_absent() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(settings.format_remove_duplicate_keys.is_none());
    }

    // ---- formatEnable setting ----

    #[test]
    fn settings_deserializes_format_enable_true() {
        let json = serde_json::json!({ "formatEnable": true });
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.format_enable, Some(true));
    }

    #[test]
    fn settings_deserializes_format_enable_false() {
        let json = serde_json::json!({ "formatEnable": false });
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.format_enable, Some(false));
    }

    #[test]
    fn settings_defaults_format_enable_to_none_when_missing() {
        let json = serde_json::json!({});
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.format_enable, None);
    }
}
