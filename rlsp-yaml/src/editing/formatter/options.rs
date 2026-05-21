// SPDX-License-Identifier: MIT

use crate::editing::editor_config::LineEnding;
use crate::server::YamlVersion;

/// YAML-specific formatting options.
#[derive(Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each bool is a distinct, well-named formatting option; a flags enum would add complexity for no benefit"
)]
// When adding or changing settings, check fixture coverage for setting
// interactions — see rlsp-yaml/tests/fixtures/CLAUDE.md.
pub struct YamlFormatOptions {
    /// Maximum line width. Default: 80.
    pub print_width: usize,
    /// Spaces per indent level. Default: 2.
    pub tab_width: usize,
    /// Prefer single-quoted strings. Default: false (double quotes).
    pub single_quote: bool,
    /// Preserve the source quote style of scalars. Default: false.
    pub preserve_quotes: bool,
    /// Add spaces inside flow braces: `{ a: 1 }` vs `{a: 1}`. Default: true.
    pub bracket_spacing: bool,
    /// YAML specification version for quoting decisions. Default: `V1_2`.
    pub yaml_version: YamlVersion,
    /// Override all collection styles to block. When `true`, flow sequences and
    /// flow mappings are emitted in block style regardless of the source style.
    /// Default: false.
    pub format_enforce_block_style: bool,
    /// Remove duplicate mapping keys before formatting, keeping the last
    /// occurrence (YAML spec: last value wins). Default: false.
    pub format_remove_duplicate_keys: bool,
    /// Indent block sequences that are values of mapping keys. Default: true.
    pub format_indent_sequences: bool,
    /// Line-ending style for output. Default: `LineEnding::Lf`.
    pub line_ending: LineEnding,
    /// Whether to append a trailing newline to the output. Default: `true`.
    pub insert_final_newline: bool,
}

impl Default for YamlFormatOptions {
    fn default() -> Self {
        Self {
            print_width: 80,
            tab_width: 2,
            single_quote: false,
            preserve_quotes: false,
            bracket_spacing: true,
            yaml_version: YamlVersion::V1_2,
            format_enforce_block_style: false,
            format_remove_duplicate_keys: false,
            format_indent_sequences: true,
            line_ending: LineEnding::Lf,
            insert_final_newline: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A1: format_enforce_block_style defaults to false.
    #[test]
    fn format_enforce_block_style_defaults_to_false() {
        assert!(!YamlFormatOptions::default().format_enforce_block_style);
    }

    // A1: format_remove_duplicate_keys defaults to false.
    #[test]
    fn format_remove_duplicate_keys_defaults_to_false() {
        assert!(!YamlFormatOptions::default().format_remove_duplicate_keys);
    }
}
