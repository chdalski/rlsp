// SPDX-License-Identifier: MIT

/// Intermediate representation node for the Wadler-Lindig pretty printer.
///
/// A `Doc` describes a document structure independent of rendering. The printer
/// decides whether to render `Group` nodes in flat mode (everything on one line)
/// or break mode (line breaks expanded) based on available width.
#[derive(Debug, Clone)]
pub enum Doc {
    /// Literal text content. Must not contain newlines.
    Text(String),
    /// Mandatory line break regardless of mode. Always breaks.
    HardLine,
    /// Soft break: space in flat mode, newline + current indent in break mode.
    Line,
    /// Increase indentation level for the child document.
    Indent(Box<Self>),
    /// Flat/break decision boundary. The printer tries flat mode first; if the
    /// content does not fit within `print_width`, it switches to break mode.
    Group(Box<Self>),
    /// Sequential composition of multiple documents.
    Concat(Vec<Self>),
    /// Different content depending on mode: `flat` in flat mode, `break_` in
    /// break mode.
    FlatAlt { flat: Box<Self>, break_: Box<Self> },
}

/// Construct a `Doc::Text` node.
#[must_use]
pub fn text(s: impl Into<String>) -> Doc {
    Doc::Text(s.into())
}

/// Construct a `Doc::HardLine` node.
#[must_use]
pub const fn hard_line() -> Doc {
    Doc::HardLine
}

/// Construct a `Doc::Line` node (soft break).
#[must_use]
pub const fn line() -> Doc {
    Doc::Line
}

/// Construct a `Doc::Indent` node.
#[must_use]
pub fn indent(doc: Doc) -> Doc {
    Doc::Indent(Box::new(doc))
}

/// Construct a `Doc::Group` node.
#[must_use]
pub fn group(doc: Doc) -> Doc {
    Doc::Group(Box::new(doc))
}

/// Construct a `Doc::Concat` node from a vector of documents.
#[must_use]
pub const fn concat(docs: Vec<Doc>) -> Doc {
    Doc::Concat(docs)
}

/// Construct a `Doc::FlatAlt` node.
#[must_use]
pub fn flat_alt(flat: Doc, break_: Doc) -> Doc {
    Doc::FlatAlt {
        flat: Box::new(flat),
        break_: Box::new(break_),
    }
}

/// Intersperse `separator` between each element of `docs`.
///
/// Returns `Doc::Concat([])` if `docs` is empty.
///
/// # Panics
///
/// Does not panic — the `expect` is unreachable after the empty-list guard.
#[must_use]
pub fn join(separator: &Doc, docs: Vec<Doc>) -> Doc {
    if docs.is_empty() {
        return Doc::Concat(vec![]);
    }
    let mut iter = docs.into_iter();
    let mut result = vec![iter.next().expect("non-empty checked above")];
    for doc in iter {
        result.push(separator.clone());
        result.push(doc);
    }
    Doc::Concat(result)
}
