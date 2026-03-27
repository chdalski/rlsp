// SPDX-License-Identifier: MIT

//! A Wadler-Lindig pretty-printing engine.
//!
//! Build a [`Doc`] tree that describes the *structure* of your output, then let
//! the printer decide where to break lines based on the available width.  The
//! core idea: [`group`] marks a subtree as a flat/break decision boundary — the
//! printer tries to render it on one line and falls back to multi-line only when
//! the content would exceed [`FormatOptions::print_width`].
//!
//! # Quick start
//!
//! ```
//! use rlsp_fmt::{Doc, concat, group, indent, line, text, format, FormatOptions};
//!
//! // Build a document: "[" <items> "]"
//! let doc = group(concat(vec![
//!     text("["),
//!     indent(concat(vec![line(), text("a"), text(","), line(), text("b")])),
//!     line(),
//!     text("]"),
//! ]));
//!
//! let opts = FormatOptions { print_width: 80, ..Default::default() };
//! assert_eq!(format(&doc, &opts), "[ a, b ]");
//!
//! // Narrow width forces the group to break.
//! let opts_narrow = FormatOptions { print_width: 5, ..Default::default() };
//! assert!(format(&doc, &opts_narrow).contains('\n'));
//! ```
//!
//! See [`FormatOptions`] for all rendering controls.

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
