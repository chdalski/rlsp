// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{CodeLens, Command, Position, Range};

use crate::schema::JsonSchema;

#[must_use]
pub fn code_lenses(schema_url: &str, schema: Option<&JsonSchema>) -> Vec<CodeLens> {
    let title = schema
        .and_then(|s| s.title.clone())
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
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::schema::JsonSchema;

    fn schema_with_title(title: &str) -> JsonSchema {
        JsonSchema {
            title: Some(title.to_string()),
            ..JsonSchema::default()
        }
    }

    #[test]
    fn uses_schema_title_when_present() {
        let schema = schema_with_title("My Schema");
        let lenses = code_lenses("https://example.com/schema.json", Some(&schema));
        assert_eq!(lenses.len(), 1);
        assert_eq!(lenses[0].command.as_ref().unwrap().title, "My Schema");
    }

    #[test]
    fn uses_url_when_schema_has_no_title() {
        let schema = JsonSchema::default();
        let lenses = code_lenses("https://example.com/schema.json", Some(&schema));
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
}
