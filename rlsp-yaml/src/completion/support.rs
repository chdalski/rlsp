// SPDX-License-Identifier: MIT

/// Maximum number of completion items returned.
pub(super) const MAX_COMPLETION_ITEMS: usize = 100;
/// Maximum number of Unicode characters in a description shown in documentation.
pub(super) const MAX_DESCRIPTION_LEN: usize = 200;
/// Maximum number of Unicode characters in an enum label.
pub(super) const MAX_ENUM_LABEL_LEN: usize = 50;

#[cfg(test)]
pub(super) mod test_fixtures {
    use tower_lsp::lsp_types::{CompletionItem, Position};

    pub use crate::schema::test_support::{
        boolean_schema, integer_schema, object_schema, string_schema,
    };

    pub fn pos(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    pub fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }
}
