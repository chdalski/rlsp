// SPDX-License-Identifier: MIT

use std::collections::HashMap;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use crate::server::MAX_LOGGED_SETTING_CHARS;

/// Most rejections [`SettingsRead`] lists; further ones are only counted, so a
/// payload full of wrong entries cannot flood the client log.
pub const MAX_LISTED_REJECTIONS: usize = 20;

/// Settings whose values can carry credentials (a proxy or schema URL with
/// userinfo): a rejected value of one of these is never recorded, only its
/// JSON type.
const URL_SETTINGS: [&str; 2] = ["httpProxy", "schemas"];

const BOOLEAN: &str = "a boolean";
const COUNT: &str = "a non-negative integer";
const TEXT: &str = "a string";

/// Workspace settings received via LSP initialization options or
/// `workspace/didChangeConfiguration`.
///
/// Read through [`SettingsRead::from_json`], which takes each field on its own:
/// a field of the wrong type falls back to its default instead of failing the
/// whole read. `Deserialize` goes through the same reader.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
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

/// The settings read from a JSON object, with what the reader rejected.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SettingsRead {
    /// The settings, with a default for every rejected or absent field.
    pub settings: Settings,
    /// The first [`MAX_LISTED_REJECTIONS`] rejected fields and entries, in
    /// field declaration order.
    pub rejections: Vec<Rejection>,
    /// How many further rejections were counted but not listed.
    pub unlisted_rejections: usize,
}

/// A field or entry the reader did not accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    /// The camelCase setting name, never taken from the payload.
    pub setting: &'static str,
    /// The part of the setting that was rejected.
    pub entry: Entry,
    /// The type the setting or entry has to be, as a phrase ("a boolean").
    pub expected: &'static str,
    /// What the payload held instead.
    pub found: RejectedValue,
}

/// Which part of a setting was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// The whole setting; it takes its default.
    Whole,
    /// The entry at this index of `customTags`; it is dropped.
    Index(usize),
    /// The entry with this key of `schemas`; it is dropped.
    Key(String),
}

/// A bounded description of a rejected JSON value: scalars as they are, text
/// cut to the log preview length, containers by size only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectedValue {
    /// JSON `null`; only a dropped entry can hold it.
    Null,
    /// A JSON boolean.
    Bool(bool),
    /// A JSON number, as serde renders it.
    Number(String),
    /// At most `MAX_LOGGED_SETTING_CHARS + 1` characters of the text, so a
    /// reader of the preview can tell whether it was cut.
    Text(String),
    /// A JSON array with this many elements.
    Array(usize),
    /// A JSON object with this many entries.
    Object(usize),
    /// Value of a setting in [`URL_SETTINGS`]; only the JSON type is kept.
    Withheld(&'static str),
}

impl Rejection {
    fn new(setting: &'static str, entry: Entry, expected: &'static str, value: &Value) -> Self {
        let found = if URL_SETTINGS.contains(&setting) {
            RejectedValue::Withheld(json_type(value))
        } else {
            RejectedValue::of(value)
        };
        Self {
            setting,
            entry,
            expected,
            found,
        }
    }
}

impl RejectedValue {
    fn of(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(flag) => Self::Bool(*flag),
            Value::Number(number) => Self::Number(number.to_string()),
            Value::String(text) => {
                Self::Text(text.chars().take(MAX_LOGGED_SETTING_CHARS + 1).collect())
            }
            Value::Array(items) => Self::Array(items.len()),
            Value::Object(entries) => Self::Object(entries.len()),
        }
    }
}

const fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

impl SettingsRead {
    /// Read `value` field by field; `None` when it is not a JSON object.
    ///
    /// An absent or `null` field takes its default. A field of the wrong type
    /// takes its default too and is recorded as a rejection. Entries of
    /// `customTags` and `schemas` are checked one by one: a wrong entry is
    /// dropped and recorded, the others stay. Unknown keys are ignored.
    #[must_use]
    pub fn from_json(value: &Value) -> Option<Self> {
        let mut reader = FieldReader {
            object: value.as_object()?,
            rejections: Vec::new(),
            unlisted_rejections: 0,
        };
        let settings = Settings {
            custom_tags: reader.custom_tags(),
            key_ordering: reader.field("keyOrdering", BOOLEAN),
            schemas: reader.schemas(),
            kubernetes_version: reader.field("kubernetesVersion", TEXT),
            schema_store: reader.field("schemaStore", BOOLEAN),
            format_print_width: reader.field("formatPrintWidth", COUNT),
            format_single_quote: reader.field("formatSingleQuote", BOOLEAN),
            format_preserve_quotes: reader.field("formatPreserveQuotes", BOOLEAN),
            http_proxy: reader.field("httpProxy", TEXT),
            color_decorators: reader.field("colorDecorators", BOOLEAN),
            format_validation: reader.field("formatValidation", BOOLEAN),
            yaml_version: reader.field("yamlVersion", TEXT),
            validate: reader.field("validate", BOOLEAN),
            hover: reader.field("hover", BOOLEAN),
            completion: reader.field("completion", BOOLEAN),
            max_items_computed: reader.field("maxItemsComputed", COUNT),
            flow_style: reader.field("flowStyle", TEXT),
            duplicate_keys: reader.field("duplicateKeys", TEXT),
            format_enforce_block_style: reader.field("formatEnforceBlockStyle", BOOLEAN),
            format_remove_duplicate_keys: reader.field("formatRemoveDuplicateKeys", BOOLEAN),
            format_bracket_spacing: reader.field("formatBracketSpacing", BOOLEAN),
            format_indent_sequences: reader.field("formatIndentSequences", BOOLEAN),
            format_enable: reader.field("formatEnable", BOOLEAN),
            format_respect_editorconfig: reader.field("formatRespectEditorconfig", BOOLEAN),
        };
        Some(Self {
            settings,
            rejections: reader.rejections,
            unlisted_rejections: reader.unlisted_rejections,
        })
    }
}

impl<'de> Deserialize<'de> for Settings {
    /// Reads a JSON object through [`SettingsRead::from_json`], dropping the
    /// rejections; any other JSON value is an error.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        SettingsRead::from_json(&value)
            .map(|read| read.settings)
            .ok_or_else(|| {
                D::Error::custom(format!(
                    "settings must be an object, got {}",
                    json_type(&value)
                ))
            })
    }
}

/// Reads the fields of one settings object, collecting what it rejects.
struct FieldReader<'a> {
    object: &'a Map<String, Value>,
    rejections: Vec<Rejection>,
    unlisted_rejections: usize,
}

impl<'a> FieldReader<'a> {
    /// The value of `key` unless it is absent or `null`.
    fn present(&self, key: &str) -> Option<&'a Value> {
        self.object.get(key).filter(|value| !value.is_null())
    }

    /// Record a rejection, built only while the list has room.
    fn record(&mut self, rejection: impl FnOnce() -> Rejection) {
        if self.rejections.len() < MAX_LISTED_REJECTIONS {
            self.rejections.push(rejection());
        } else {
            self.unlisted_rejections = self.unlisted_rejections.saturating_add(1);
        }
    }

    /// Read `key` as `T`; the default when it is absent, `null`, or of the
    /// wrong type (the last is recorded as a rejection of the whole setting).
    fn field<T: Deserialize<'a> + Default>(
        &mut self,
        key: &'static str,
        expected: &'static str,
    ) -> T {
        let Some(value) = self.present(key) else {
            return T::default();
        };
        T::deserialize(value).unwrap_or_else(|_| {
            self.record(|| Rejection::new(key, Entry::Whole, expected, value));
            T::default()
        })
    }

    /// Read one string entry of `setting`; a non-string entry is recorded.
    fn string_entry(
        &mut self,
        setting: &'static str,
        entry: impl FnOnce() -> Entry,
        value: &Value,
    ) -> Option<String> {
        let text = String::deserialize(value).ok();
        if text.is_none() {
            self.record(|| Rejection::new(setting, entry(), TEXT, value));
        }
        text
    }

    fn custom_tags(&mut self) -> Vec<String> {
        match self.present("customTags") {
            None => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    self.string_entry("customTags", || Entry::Index(index), item)
                })
                .collect(),
            Some(other) => {
                self.record(|| {
                    Rejection::new("customTags", Entry::Whole, "an array of strings", other)
                });
                Vec::new()
            }
        }
    }

    fn schemas(&mut self) -> HashMap<String, String> {
        match self.present("schemas") {
            None => HashMap::new(),
            Some(Value::Object(associations)) => associations
                .iter()
                .filter_map(|(url, pattern)| {
                    self.string_entry("schemas", || Entry::Key(url.clone()), pattern)
                        .map(|pattern| (url.clone(), pattern))
                })
                .collect(),
            Some(other) => {
                self.record(|| {
                    Rejection::new("schemas", Entry::Whole, "an object of strings", other)
                });
                HashMap::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;
    use serde_json::json;

    use super::*;

    const BOOLEAN_KEYS: [&str; 15] = [
        "keyOrdering",
        "schemaStore",
        "formatSingleQuote",
        "formatPreserveQuotes",
        "colorDecorators",
        "formatValidation",
        "validate",
        "hover",
        "completion",
        "formatEnforceBlockStyle",
        "formatRemoveDuplicateKeys",
        "formatBracketSpacing",
        "formatIndentSequences",
        "formatEnable",
        "formatRespectEditorconfig",
    ];
    const COUNT_KEYS: [&str; 2] = ["formatPrintWidth", "maxItemsComputed"];
    const TEXT_KEYS: [&str; 5] = [
        "kubernetesVersion",
        "httpProxy",
        "yamlVersion",
        "flowStyle",
        "duplicateKeys",
    ];
    const COLLECTION_KEYS: [&str; 2] = ["customTags", "schemas"];

    fn all_keys() -> Vec<&'static str> {
        [
            BOOLEAN_KEYS.as_slice(),
            COUNT_KEYS.as_slice(),
            TEXT_KEYS.as_slice(),
            COLLECTION_KEYS.as_slice(),
        ]
        .concat()
    }

    fn read(value: &Value) -> SettingsRead {
        SettingsRead::from_json(value).expect("an object reads")
    }

    /// The only rejection of `value`, which must have exactly one.
    fn sole_rejection(value: &Value) -> Rejection {
        let mut rejections = read(value).rejections;
        assert_eq!(rejections.len(), 1, "{rejections:?}");
        rejections.remove(0)
    }

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

    // ---- Field-by-field reader ----

    #[rstest]
    #[case::null(json!(null))]
    #[case::boolean(json!(true))]
    #[case::number(json!(5))]
    #[case::string(json!("str"))]
    #[case::array(json!([{"keyOrdering": true}]))]
    fn reader_and_deserialize_refuse_a_value_that_is_not_an_object(#[case] value: Value) {
        assert_eq!(SettingsRead::from_json(&value), None);
        assert!(serde_json::from_value::<Settings>(value).is_err());
    }

    #[test]
    fn reader_reads_a_valid_object_without_rejections() {
        let read = read(&json!({"keyOrdering": true, "customTags": ["!a"]}));
        assert!(read.settings.key_ordering);
        assert_eq!(read.settings.custom_tags, vec!["!a"]);
        assert!(read.rejections.is_empty());
        assert_eq!(read.unlisted_rejections, 0);
    }

    #[test]
    fn reader_ignores_unknown_keys_without_rejection() {
        let read = read(&json!({"unknownKey": [1, {"x": null}], "keyordering": true}));
        assert_eq!(read, SettingsRead::default());
    }

    #[test]
    fn reader_takes_default_without_rejection_for_null_on_every_key() {
        let object: Map<String, Value> = all_keys()
            .into_iter()
            .map(|key| (key.to_string(), Value::Null))
            .collect();
        assert_eq!(read(&Value::Object(object)), SettingsRead::default());
    }

    #[rstest]
    #[case::string(json!("true"))]
    #[case::number(json!(1))]
    #[case::array(json!([]))]
    #[case::object(json!({}))]
    fn reader_rejects_a_wrong_typed_boolean_field_and_takes_its_default(#[case] bad: Value) {
        for key in BOOLEAN_KEYS {
            let rejection = sole_rejection(&json!({ key: bad }));
            assert_eq!(rejection.setting, key);
            assert_eq!(rejection.entry, Entry::Whole);
            assert_eq!(rejection.expected, BOOLEAN);
            assert_eq!(rejection.found, RejectedValue::of(&bad));
            assert_eq!(read(&json!({ key: bad })).settings, Settings::default());
        }
    }

    #[rstest]
    #[case::string(json!("80"))]
    #[case::boolean(json!(true))]
    #[case::array(json!([]))]
    #[case::object(json!({}))]
    #[case::negative(json!(-1))]
    #[case::fraction(json!(1.5))]
    #[case::whole_float(json!(80.0))]
    #[case::exponent(json!(1e3))]
    #[case::beyond_u64(json!(18_446_744_073_709_551_616.0))]
    fn reader_rejects_a_wrong_typed_count_field_and_takes_its_default(#[case] bad: Value) {
        for key in COUNT_KEYS {
            let rejection = sole_rejection(&json!({ key: bad }));
            assert_eq!(rejection.setting, key);
            assert_eq!(rejection.expected, COUNT);
            assert_eq!(read(&json!({ key: bad })).settings, Settings::default());
        }
    }

    #[rstest]
    #[case::zero(json!(0), 0)]
    #[case::largest(json!(usize::MAX), usize::MAX)]
    fn reader_accepts_every_non_negative_integer_for_a_count_field(
        #[case] value: Value,
        #[case] expected: usize,
    ) {
        let read = read(&json!({"formatPrintWidth": value, "maxItemsComputed": value}));
        assert_eq!(read.settings.format_print_width, Some(expected));
        assert_eq!(read.settings.max_items_computed, Some(expected));
        assert!(read.rejections.is_empty());
    }

    #[rstest]
    #[case::number(json!(1.29))]
    #[case::integer(json!(129))]
    #[case::boolean(json!(true))]
    #[case::array(json!(["1.29.0"]))]
    #[case::object(json!({}))]
    fn reader_rejects_a_wrong_typed_text_field_and_takes_its_default(#[case] bad: Value) {
        for key in TEXT_KEYS {
            let rejection = sole_rejection(&json!({ key: bad }));
            assert_eq!(rejection.setting, key);
            assert_eq!(rejection.expected, TEXT);
            assert_eq!(read(&json!({ key: bad })).settings, Settings::default());
        }
    }

    #[test]
    fn reader_does_not_validate_the_value_of_a_text_field() {
        let read = read(&json!({
            "flowStyle": "bogus",
            "yamlVersion": "9.9",
            "duplicateKeys": "",
            "kubernetesVersion": "bad",
        }));
        assert_eq!(read.settings.flow_style.as_deref(), Some("bogus"));
        assert_eq!(read.settings.yaml_version.as_deref(), Some("9.9"));
        assert_eq!(read.settings.duplicate_keys.as_deref(), Some(""));
        assert_eq!(read.settings.kubernetes_version.as_deref(), Some("bad"));
        assert!(read.rejections.is_empty());
    }

    #[test]
    fn reader_applies_valid_fields_next_to_rejected_ones() {
        let read = read(&json!({
            "keyOrdering": "yes",
            "hover": false,
            "formatPrintWidth": -3,
            "httpProxy": "http://p:1",
        }));
        assert!(!read.settings.key_ordering);
        assert_eq!(read.settings.hover, Some(false));
        assert_eq!(read.settings.format_print_width, None);
        assert_eq!(read.settings.http_proxy.as_deref(), Some("http://p:1"));
        assert_eq!(read.rejections.len(), 2);
    }

    #[test]
    fn reader_lists_rejections_in_field_declaration_order() {
        let read = read(&json!({
            "httpProxy": 1,
            "customTags": [0, "!a", null],
            "keyOrdering": "x",
        }));
        let order: Vec<_> = read
            .rejections
            .iter()
            .map(|rejection| (rejection.setting, rejection.entry.clone()))
            .collect();
        assert_eq!(
            order,
            vec![
                ("customTags", Entry::Index(0)),
                ("customTags", Entry::Index(2)),
                ("keyOrdering", Entry::Whole),
                ("httpProxy", Entry::Whole),
            ]
        );
    }

    #[test]
    fn reader_describes_the_expected_type_per_kind_of_setting() {
        let expected: Vec<_> = [
            json!({"keyOrdering": 1}),
            json!({"formatPrintWidth": "1"}),
            json!({"hover": 1}),
            json!({"flowStyle": 1}),
            json!({"customTags": [1]}),
            json!({"customTags": 1}),
            json!({"schemas": 1}),
        ]
        .iter()
        .map(|value| sole_rejection(value).expected)
        .collect();
        assert_eq!(
            expected,
            vec![
                "a boolean",
                "a non-negative integer",
                "a boolean",
                "a string",
                "a string",
                "an array of strings",
                "an object of strings",
            ]
        );
    }

    // ---- customTags entries ----

    #[test]
    fn custom_tags_drops_non_string_entries_and_keeps_the_rest() {
        let read = read(&json!({"customTags": ["!a", 1, null, "!b", {}, ["x"], true]}));
        assert_eq!(read.settings.custom_tags, vec!["!a", "!b"]);
        let entries: Vec<_> = read.rejections.iter().map(|r| r.entry.clone()).collect();
        assert_eq!(
            entries,
            vec![
                Entry::Index(1),
                Entry::Index(2),
                Entry::Index(4),
                Entry::Index(5),
                Entry::Index(6),
            ]
        );
        assert!(read.rejections.iter().all(|r| r.setting == "customTags"));
    }

    #[test]
    fn custom_tags_keeps_empty_strings_and_duplicates() {
        let read = read(&json!({"customTags": ["", "!a", "!a"]}));
        assert_eq!(read.settings.custom_tags, vec!["", "!a", "!a"]);
        assert!(read.rejections.is_empty());
    }

    #[rstest]
    #[case::string(json!("!include"))]
    #[case::object(json!({}))]
    #[case::number(json!(5))]
    #[case::boolean(json!(true))]
    fn custom_tags_rejects_a_value_that_is_not_an_array(#[case] bad: Value) {
        let rejection = sole_rejection(&json!({"customTags": bad}));
        assert_eq!(rejection.entry, Entry::Whole);
        assert_eq!(rejection.expected, "an array of strings");
        assert!(
            read(&json!({"customTags": bad}))
                .settings
                .custom_tags
                .is_empty()
        );
    }

    // ---- schemas entries ----

    #[test]
    fn schemas_drops_entries_with_a_non_string_value_and_keeps_the_rest() {
        let read = read(&json!({"schemas": {
            "https://a.test/s.json": "*.yaml",
            "https://b.test/s.json": 5,
            "https://c.test/s.json": null,
            "https://d.test/s.json": ["x"],
            "https://e.test/s.json": {},
        }}));
        assert_eq!(
            read.settings.schemas,
            HashMap::from([("https://a.test/s.json".to_string(), "*.yaml".to_string())])
        );
        let keys: Vec<_> = read.rejections.iter().map(|r| r.entry.clone()).collect();
        assert_eq!(
            keys,
            ["b", "c", "d", "e"]
                .map(|host| Entry::Key(format!("https://{host}.test/s.json")))
                .to_vec()
        );
    }

    #[test]
    fn schemas_keeps_an_empty_key_and_an_empty_pattern() {
        let read = read(&json!({"schemas": {"": "", "x": ""}}));
        assert_eq!(read.settings.schemas.len(), 2);
        assert!(read.rejections.is_empty());
    }

    #[rstest]
    #[case::array_of_urls(json!(["https://a/s.json"]))]
    #[case::string(json!("https://u:pw@h/x"))]
    #[case::number(json!(5))]
    #[case::boolean(json!(true))]
    fn schemas_rejects_a_value_that_is_not_an_object(#[case] bad: Value) {
        let rejection = sole_rejection(&json!({"schemas": bad}));
        assert_eq!(rejection.entry, Entry::Whole);
        assert_eq!(rejection.expected, "an object of strings");
        assert!(read(&json!({"schemas": bad})).settings.schemas.is_empty());
    }

    // ---- What a rejection carries ----

    #[rstest]
    #[case::proxy_object(json!({"httpProxy": {"u": "http://u:pw@proxy"}}), "an object")]
    #[case::proxy_array(json!({"httpProxy": ["http://u:pw@proxy"]}), "an array")]
    #[case::proxy_number(json!({"httpProxy": 5}), "a number")]
    #[case::schemas_string(json!({"schemas": "https://u:pw@h/x"}), "a string")]
    #[case::schema_entry_array(
        json!({"schemas": {"https://h/a.json": ["https://u:pw@h/x"]}}),
        "an array"
    )]
    fn rejection_of_a_url_setting_keeps_only_the_json_type(
        #[case] value: Value,
        #[case] kind: &'static str,
    ) {
        assert_eq!(sole_rejection(&value).found, RejectedValue::Withheld(kind));
    }

    #[test]
    fn rejection_keeps_a_bounded_preview_of_a_long_string() {
        let long = "\u{e9}".repeat(1_000_000);
        let found = sole_rejection(&json!({"formatPrintWidth": long})).found;
        let RejectedValue::Text(preview) = found else {
            panic!("a string is previewed as text: {found:?}");
        };
        assert_eq!(preview.chars().count(), MAX_LOGGED_SETTING_CHARS + 1);
    }

    #[test]
    fn rejection_describes_a_container_by_its_size_only() {
        let big: Vec<u32> = (0..100_000).collect();
        assert_eq!(
            sole_rejection(&json!({"keyOrdering": big})).found,
            RejectedValue::Array(100_000)
        );
        assert_eq!(
            sole_rejection(&json!({"keyOrdering": {"a": [1, 2], "b": 1}})).found,
            RejectedValue::Object(2)
        );
    }

    #[test]
    fn rejection_keeps_a_scalar_as_it_is() {
        assert_eq!(
            sole_rejection(&json!({"keyOrdering": 1.5})).found,
            RejectedValue::Number("1.5".to_string())
        );
        assert_eq!(
            sole_rejection(&json!({"formatPrintWidth": true})).found,
            RejectedValue::Bool(true)
        );
        assert_eq!(
            sole_rejection(&json!({"customTags": [null]})).found,
            RejectedValue::Null
        );
    }

    #[test]
    fn reader_lists_at_most_the_limit_and_counts_the_rest() {
        let tags: Vec<Value> = (0..10_000)
            .map(|index| {
                if index == 5_000 {
                    json!("!kept")
                } else {
                    json!(index)
                }
            })
            .collect();
        let read = read(&json!({"customTags": tags}));
        assert_eq!(read.settings.custom_tags, vec!["!kept"]);
        assert_eq!(read.rejections.len(), MAX_LISTED_REJECTIONS);
        assert_eq!(read.unlisted_rejections, 9_999 - MAX_LISTED_REJECTIONS);
    }

    // ---- Deserialize ----

    #[test]
    fn deserialize_reads_an_object_with_wrong_typed_fields_like_the_reader() {
        let json = json!({"keyOrdering": "yes", "hover": false, "customTags": ["!a", 1]});
        let settings: Settings = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(settings, read(&json).settings);
        assert!(!settings.key_ordering);
        assert_eq!(settings.hover, Some(false));
        assert_eq!(settings.custom_tags, vec!["!a"]);
    }

    #[test]
    fn deserialize_reads_json_text() {
        let settings: Settings = serde_json::from_str(r#"{"keyOrdering": true}"#).unwrap();
        assert!(settings.key_ordering);
        assert!(serde_json::from_str::<Settings>("[1]").is_err());
    }

    fn arbitrary_json() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::from),
            any::<i64>().prop_map(Value::from),
            any::<f64>().prop_map(Value::from),
            ".{0,40}".prop_map(Value::from),
        ];
        leaf.prop_recursive(3, 24, 4, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
                prop::collection::vec((".{0,8}", inner), 0..4)
                    .prop_map(|entries| Value::Object(entries.into_iter().collect())),
            ]
        })
    }

    proptest! {
        #[test]
        fn reader_reports_only_known_settings_for_any_value(
            values in prop::collection::vec(arbitrary_json(), 24)
        ) {
            let keys = all_keys();
            let object: Map<String, Value> = keys
                .iter()
                .zip(values)
                .map(|(key, value)| ((*key).to_string(), value))
                .collect();
            let read = read(&Value::Object(object));
            prop_assert!(read.rejections.len() <= MAX_LISTED_REJECTIONS);
            prop_assert!(read.rejections.iter().all(|r| keys.contains(&r.setting)));
        }
    }
}
