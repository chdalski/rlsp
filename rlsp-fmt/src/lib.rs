// SPDX-License-Identifier: MIT

pub mod ir;
pub mod printer;

pub use ir::{Doc, concat, flat_alt, group, hard_line, indent, join, line, text};
pub use printer::format;

/// Options that control how a document is rendered.
#[derive(Debug, Clone)]
pub struct FormatOptions {
    /// Maximum line width before the printer breaks groups. Default: 80.
    pub print_width: usize,
    /// Number of spaces per indentation level (ignored when `use_tabs` is
    /// true). Default: 2.
    pub tab_width: usize,
    /// Use tab characters instead of spaces for indentation. Default: false.
    pub use_tabs: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            print_width: 80,
            tab_width: 2,
            use_tabs: false,
        }
    }
}
