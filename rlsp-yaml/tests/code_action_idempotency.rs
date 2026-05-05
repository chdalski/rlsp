// SPDX-License-Identifier: MIT
//
// Property-based idempotency test for cursor-driven code actions.
//
// For each available context-driven code action on a generated YAML input,
// applying the action once and then applying code_actions again must produce
// no second action with the same title — the first application is a no-op
// target for the second call (idempotency invariant).
//
// This covers: tab-to-spaces, quoted-bool, block-to-flow, block-scalar.
// Diagnostic-driven actions (flow-to-block, delete-anchor, yaml11-bool,
// yaml11-octal) are excluded because their trigger requires a pre-existing
// diagnostic that the edited document may or may not produce.

#![expect(missing_docs, reason = "test code")]
#![expect(
    dead_code,
    reason = "common module has helpers not used by every test file"
)]
#![expect(
    clippy::cast_possible_truncation,
    reason = "test code — LSP line/col counts fit in u32 for any real YAML file"
)]

mod common;
use common::*;

use proptest::prelude::*;
use rlsp_yaml::editing::code_actions::code_actions;
use rlsp_yaml::editing::formatter::YamlFormatOptions;
use tower_lsp::lsp_types::{Position, Range};

// ---- Helpers ----------------------------------------------------------------

fn whole_file_range(text: &str) -> Range {
    let lines: Vec<&str> = text.lines().collect();
    let last_line = lines.len().saturating_sub(1) as u32;
    let last_char = lines.last().map_or(0, |l| l.len() as u32);
    Range::new(Position::new(0, 0), Position::new(last_line, last_char))
}

/// Apply the first [`TextEdit`](tower_lsp::lsp_types::TextEdit) from a
/// [`CodeAction`](tower_lsp::lsp_types::CodeAction) to `text`.
fn apply_action_edit(
    text: &str,
    action: &tower_lsp::lsp_types::CodeAction,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<String> {
    let edit = action.edit.as_ref()?;
    let changes = edit.changes.as_ref()?;
    let text_edits = changes.get(uri)?;
    let first_edit = text_edits.first()?;
    Some(apply_text_edit(text, first_edit))
}

// ---- YAML input strategies --------------------------------------------------

/// Simple block mapping scalars that trigger context-driven actions.
fn simple_block_mapping() -> impl Strategy<Value = String> {
    prop_oneof![
        // Quoted-bool candidates (block context)
        Just("enabled: \"true\"\n".to_string()),
        Just("enabled: 'false'\n".to_string()),
        Just("debug: \"true\"\nverbose: 'false'\n".to_string()),
        // Block-to-flow candidates (block sequences and mappings)
        Just("items:\n  - one\n  - two\n".to_string()),
        Just("config:\n  host: localhost\n  port: \"8080\"\n".to_string()),
        // Block-scalar candidate (long string in mapping value)
        Just("description: this is a very long plain scalar value that exceeds forty chars for sure\n".to_string()),
        // Tab candidate (tab in indentation)
        Just("\tkey: value\n".to_string()),
    ]
}

/// Flow-style YAML inputs.
fn simple_flow_yaml() -> impl Strategy<Value = String> {
    prop_oneof![
        // Quoted-bool in flow mapping
        Just("config: {enabled: \"true\"}\n".to_string()),
        Just("items: [\"true\", \"false\"]\n".to_string()),
        // Already-converted block-to-flow (no action expected)
        Just("items: [one, two]\n".to_string()),
    ]
}

fn yaml_strategy() -> impl Strategy<Value = String> {
    prop_oneof![simple_block_mapping(), simple_flow_yaml(),]
}

// ---- Property test ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    })]

    /// Applying a context-driven code action a second time on the result of the
    /// first application must not yield a second action with the same title.
    ///
    /// Invariant: for every action returned by the first `code_actions()` call,
    /// applying its first `TextEdit` to the input document produces a new document
    /// for which `code_actions()` does NOT return an action with the same title.
    #[test]
    fn code_action_second_application_is_noop(input in yaml_strategy()) {
        let uri = test_uri();
        let opts = YamlFormatOptions::default();

        let docs = docs_for(&input);
        let range = whole_file_range(&input);

        let first_actions = code_actions(&docs, &input, range, &[], &uri, &opts);

        for action in &first_actions {
            let title = &action.title;

            // Apply the first action's edit.
            let Some(edited) = apply_action_edit(&input, action, &uri) else {
                continue; // no edit — skip
            };

            // Call code_actions on the edited document.
            let edited_docs = docs_for(&edited);
            let edited_range = whole_file_range(&edited);
            let second_actions = code_actions(&edited_docs, &edited, edited_range, &[], &uri, &opts);

            // The edited document must not produce an action with the same title.
            let repeated = second_actions.iter().find(|a| &a.title == title);
            prop_assert!(
                repeated.is_none(),
                "action {:?} was offered again after first application.\n  \
                 input:  {:?}\n  edited: {:?}",
                title, input, edited,
            );
        }
    }
}
