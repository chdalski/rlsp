// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeLens, Command, Position, Range};

use crate::schema::ParsedSchema;

/// Produce code lenses for a document associated with `schema_url`.
#[must_use]
pub fn code_lenses(schema_url: &str, schema: Option<&ParsedSchema>) -> Vec<CodeLens> {
    let title = schema
        .and_then(|s| s.resolved_root().title.clone())
        .unwrap_or_else(|| schema_url.to_string());

    let range = Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: 0,
        },
    };

    let command = Command {
        title,
        command: "vscode.open".to_string(),
        arguments: Some(vec![serde_json::Value::String(schema_url.to_string())]),
    };

    vec![CodeLens {
        range,
        command: Some(command),
        data: None,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::test_support::{reference_to, with_targets};
    use crate::schema::{JsonSchema, SchemaRef};

    fn schema_with_title(title: &str) -> JsonSchema {
        JsonSchema {
            title: Some(title.to_string()),
            ..JsonSchema::default()
        }
    }

    #[test]
    fn uses_schema_title_when_present() {
        let schema = schema_with_title("My Schema");
        let lenses = code_lenses(
            "https://example.com/schema.json",
            Some(&ParsedSchema::new(schema)),
        );
        assert_eq!(lenses.len(), 1);
        assert_eq!(lenses[0].command.as_ref().unwrap().title, "My Schema");
    }

    #[test]
    fn uses_url_when_schema_has_no_title() {
        let schema = JsonSchema::default();
        let lenses = code_lenses(
            "https://example.com/schema.json",
            Some(&ParsedSchema::new(schema)),
        );
        assert_eq!(lenses.len(), 1);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "https://example.com/schema.json"
        );
    }

    #[test]
    fn uses_url_when_schema_is_none() {
        let lenses = code_lenses("https://example.com/schema.json", None);
        assert_eq!(lenses.len(), 1);
        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "https://example.com/schema.json"
        );
    }

    #[test]
    fn command_id_is_vscode_open() {
        let lenses = code_lenses("https://example.com/schema.json", None);
        assert_eq!(lenses[0].command.as_ref().unwrap().command, "vscode.open");
    }

    #[test]
    fn range_is_at_line_0_col_0() {
        let lenses = code_lenses("https://example.com/schema.json", None);
        let range = lenses[0].range;
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 0);
    }

    #[test]
    fn uses_the_title_of_the_schema_a_root_reference_stands_for() {
        let schema = with_targets(reference_to(0), vec![schema_with_title("Shared Title")]);
        let lenses = code_lenses("https://example.com/schema.json", Some(&schema));

        assert_eq!(lenses[0].command.as_ref().unwrap().title, "Shared Title");
    }

    #[test]
    fn uses_the_title_of_the_target_and_not_the_own_title_of_a_root_reference() {
        let root = JsonSchema {
            title: Some("Own Title".to_string()),
            ..reference_to(0)
        };
        let schema = with_targets(root, vec![schema_with_title("Shared Title")]);
        let lenses = code_lenses("https://example.com/schema.json", Some(&schema));

        assert_eq!(lenses[0].command.as_ref().unwrap().title, "Shared Title");
    }

    #[test]
    fn uses_url_when_the_root_reference_points_past_the_target_table() {
        let schema = with_targets(reference_to(3), vec![schema_with_title("Shared Title")]);
        let lenses = code_lenses("https://example.com/schema.json", Some(&schema));

        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "https://example.com/schema.json"
        );
    }

    #[test]
    fn uses_url_when_the_root_is_an_unresolved_reference() {
        let schema = ParsedSchema::new(JsonSchema {
            reference: Some(SchemaRef::Unresolved("#/missing".into())),
            ..JsonSchema::default()
        });
        let lenses = code_lenses("https://example.com/schema.json", Some(&schema));

        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "https://example.com/schema.json"
        );
    }

    #[test]
    fn uses_url_when_the_root_reference_has_no_target() {
        let schema = ParsedSchema::new(reference_to(0));
        let lenses = code_lenses("https://example.com/schema.json", Some(&schema));

        assert_eq!(
            lenses[0].command.as_ref().unwrap().title,
            "https://example.com/schema.json"
        );
    }

    #[test]
    fn uses_the_title_of_the_target_of_a_parsed_root_reference() {
        let schema = crate::schema::parse_schema(&serde_json::json!({
            "$ref": "#/definitions/Root",
            "definitions": { "Root": { "title": "Root Title" } }
        }))
        .expect("parses");

        let lenses = code_lenses("https://example.com/schema.json", Some(&schema));

        assert_eq!(lenses[0].command.as_ref().unwrap().title, "Root Title");
    }
}
