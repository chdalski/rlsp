// SPDX-License-Identifier: MIT

/// Maximum number of completion items returned.
pub(super) const MAX_COMPLETION_ITEMS: usize = 100;
/// Maximum number of allOf/anyOf/oneOf branches walked for property collection.
pub(super) const MAX_BRANCH_COUNT: usize = 20;
/// Maximum number of Unicode characters in a description shown in documentation.
pub(super) const MAX_DESCRIPTION_LEN: usize = 200;
/// Maximum number of Unicode characters in an enum label.
pub(super) const MAX_ENUM_LABEL_LEN: usize = 50;

#[cfg(test)]
pub(super) mod test_fixtures {
    use tower_lsp::lsp_types::{CompletionItem, Position};

    use crate::schema::{JsonSchema, SchemaType};

    pub fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    pub fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    pub fn string_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            ..JsonSchema::default()
        }
    }

    pub fn integer_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("integer".to_string())),
            ..JsonSchema::default()
        }
    }

    pub fn boolean_schema() -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("boolean".to_string())),
            ..JsonSchema::default()
        }
    }

    pub fn object_schema(props: Vec<(&str, JsonSchema)>) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("object".to_string())),
            properties: Some(props.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
            ..JsonSchema::default()
        }
    }
}
