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
///
/// # Examples
///
/// ```
/// use rlsp_fmt::{text, format, FormatOptions};
///
/// let doc = text("hello");
/// assert_eq!(format(&doc, &FormatOptions::default()), "hello");
/// ```
#[must_use]
pub fn text(s: impl Into<String>) -> Doc {
    Doc::Text(s.into())
}

/// Construct a `Doc::HardLine` node.
///
/// # Examples
///
/// ```
/// use rlsp_fmt::{concat, hard_line, text, format, FormatOptions};
///
/// let doc = concat(vec![text("a"), hard_line(), text("b")]);
/// assert_eq!(format(&doc, &FormatOptions::default()), "a\nb");
/// ```
#[must_use]
pub const fn hard_line() -> Doc {
    Doc::HardLine
}

/// Construct a `Doc::Line` node (soft break).
///
/// # Examples
///
/// ```
/// use rlsp_fmt::{concat, group, line, text, format, FormatOptions};
///
/// // In a group that fits, Line renders as a space.
/// let doc = group(concat(vec![text("a"), line(), text("b")]));
/// assert_eq!(format(&doc, &FormatOptions::default()), "a b");
/// ```
#[must_use]
pub const fn line() -> Doc {
    Doc::Line
}

/// Construct a `Doc::Indent` node.
///
/// # Examples
///
/// ```
/// use rlsp_fmt::{concat, group, indent, line, text, format, FormatOptions};
///
/// let doc = group(concat(vec![
///     text("key:"),
///     indent(concat(vec![line(), text("value")])),
/// ]));
/// let opts = FormatOptions { print_width: 5, ..Default::default() };
/// assert_eq!(format(&doc, &opts), "key:\n  value");
/// ```
#[must_use]
pub fn indent(doc: Doc) -> Doc {
    Doc::Indent(Box::new(doc))
}

/// Construct a `Doc::Group` node.
///
/// # Examples
///
/// ```
/// use rlsp_fmt::{concat, group, line, text, format, FormatOptions};
///
/// // Wide width: group stays flat.
/// let doc = group(concat(vec![text("a"), line(), text("b")]));
/// assert_eq!(format(&doc, &FormatOptions::default()), "a b");
///
/// // Narrow width: group breaks.
/// let opts = FormatOptions { print_width: 1, ..Default::default() };
/// assert_eq!(format(&doc, &opts), "a\nb");
/// ```
#[must_use]
pub fn group(doc: Doc) -> Doc {
    Doc::Group(Box::new(doc))
}

/// Construct a `Doc::Concat` node from a vector of documents.
///
/// # Examples
///
/// ```
/// use rlsp_fmt::{concat, text, format, FormatOptions};
///
/// let doc = concat(vec![text("foo"), text("bar")]);
/// assert_eq!(format(&doc, &FormatOptions::default()), "foobar");
/// ```
#[must_use]
pub const fn concat(docs: Vec<Doc>) -> Doc {
    Doc::Concat(docs)
}

/// Construct a `Doc::FlatAlt` node.
///
/// # Examples
///
/// ```
/// use rlsp_fmt::{flat_alt, group, text, format, FormatOptions};
///
/// // In a group that fits, the flat variant is used.
/// let doc = group(flat_alt(text("flat"), text("break")));
/// assert_eq!(format(&doc, &FormatOptions::default()), "flat");
/// ```
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
///
/// # Examples
///
/// ```
/// use rlsp_fmt::{join, text, format, FormatOptions};
///
/// let sep = text(", ");
/// let items = vec![text("a"), text("b"), text("c")];
/// let doc = join(&sep, items);
/// assert_eq!(format(&doc, &FormatOptions::default()), "a, b, c");
/// ```
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
