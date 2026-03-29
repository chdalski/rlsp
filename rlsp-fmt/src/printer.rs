// SPDX-License-Identifier: MIT

use crate::FormatOptions;
use crate::ir::Doc;

/// Rendering mode for the printer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

/// Work item: current indent level, mode, and document reference.
struct Work<'a> {
    indent: usize,
    mode: Mode,
    doc: &'a Doc,
}

/// Render a `Doc` to a `String` using the Wadler-Lindig algorithm.
///
/// The algorithm processes a work stack of `(indent, mode, doc)` triples.
/// `Group` nodes attempt flat mode by checking whether the group fits within
/// the remaining line width (`fits` lookahead). If it doesn't fit, the group
/// switches to break mode.
///
/// # Examples
///
/// ```
/// use rlsp_fmt::{concat, group, indent, line, text, format, FormatOptions};
///
/// let doc = group(concat(vec![
///     text("["),
///     indent(concat(vec![line(), text("a"), text(","), line(), text("b")])),
///     line(),
///     text("]"),
/// ]));
///
/// // Wide enough: rendered flat on one line.
/// let wide = format(&doc, &FormatOptions::default());
/// assert_eq!(wide, "[ a, b ]");
///
/// // Narrow: rendered across multiple lines.
/// let opts = FormatOptions { print_width: 5, ..Default::default() };
/// assert!(format(&doc, &opts).contains('\n'));
/// ```
#[must_use]
pub fn format(doc: &Doc, options: &FormatOptions) -> String {
    let mut output = String::new();
    let mut col: usize = 0; // current column position

    let mut stack: Vec<Work<'_>> = vec![Work {
        indent: 0,
        mode: Mode::Break,
        doc,
    }];

    while let Some(Work { indent, mode, doc }) = stack.pop() {
        match doc {
            Doc::Text(s) => {
                output.push_str(s);
                col += s.len();
            }
            Doc::HardLine => {
                output.push('\n');
                push_indent(&mut output, indent, options);
                col = indent_width(indent, options);
            }
            Doc::Line => match mode {
                Mode::Flat => {
                    output.push(' ');
                    col += 1;
                }
                Mode::Break => {
                    output.push('\n');
                    push_indent(&mut output, indent, options);
                    col = indent_width(indent, options);
                }
            },
            Doc::Indent(inner) => {
                stack.push(Work {
                    indent: indent + 1,
                    mode,
                    doc: inner,
                });
            }
            Doc::Concat(docs) => {
                // Push in reverse so the first element is processed first.
                for child in docs.iter().rev() {
                    stack.push(Work {
                        indent,
                        mode,
                        doc: child,
                    });
                }
            }
            Doc::Group(inner) => {
                // Try flat mode first; fall back to break if it doesn't fit.
                let effective_mode =
                    if mode == Mode::Flat || fits(inner, options.print_width.saturating_sub(col)) {
                        Mode::Flat
                    } else {
                        Mode::Break
                    };
                stack.push(Work {
                    indent,
                    mode: effective_mode,
                    doc: inner,
                });
            }
            Doc::FlatAlt { flat, break_ } => {
                let chosen = match mode {
                    Mode::Flat => flat.as_ref(),
                    Mode::Break => break_.as_ref(),
                };
                stack.push(Work {
                    indent,
                    mode,
                    doc: chosen,
                });
            }
        }
    }

    output
}

/// Returns `true` if `doc` fits within `remaining` columns when rendered flat.
///
/// The lookahead uses its own flat-mode stack. It returns `false` as soon as
/// any content would exceed `remaining`, or when a `HardLine` is encountered
/// (which forces a break regardless of mode).
fn fits(doc: &Doc, remaining: usize) -> bool {
    // Track remaining space using a saturating sentinel: if consumed > 0 after
    // saturation we know it didn't fit.
    let mut rem: Option<usize> = Some(remaining);
    let mut stack: Vec<&Doc> = vec![doc];

    while let Some(doc) = stack.pop() {
        if rem.is_none() {
            return false;
        }
        match doc {
            Doc::Text(s) => {
                rem = rem.and_then(|r| r.checked_sub(s.len()));
            }
            Doc::HardLine => {
                // Hard line always breaks — doesn't fit in flat lookahead.
                return false;
            }
            Doc::Line => {
                // In flat mode, Line renders as a single space.
                rem = rem.and_then(|r| r.checked_sub(1));
            }
            Doc::Indent(inner) | Doc::Group(inner) => {
                stack.push(inner);
            }
            Doc::Concat(docs) => {
                for child in docs.iter().rev() {
                    stack.push(child);
                }
            }
            Doc::FlatAlt { flat, .. } => {
                // Lookahead is always flat — use the flat variant.
                stack.push(flat.as_ref());
            }
        }
    }

    rem.is_some()
}

/// Write the indentation string for a given indent level to `out`.
fn push_indent(out: &mut String, indent: usize, options: &FormatOptions) {
    if options.use_tabs {
        for _ in 0..indent {
            out.push('\t');
        }
    } else {
        let spaces = indent * options.tab_width;
        for _ in 0..spaces {
            out.push(' ');
        }
    }
}

/// Return the column width of an indentation level.
const fn indent_width(indent: usize, options: &FormatOptions) -> usize {
    if options.use_tabs {
        // Tabs are visually ambiguous; count as 1 column each for fit checks.
        indent
    } else {
        indent * options.tab_width
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::FormatOptions;
    use crate::ir::{concat, flat_alt, group, hard_line, indent, join, line, text};

    fn opts(print_width: usize) -> FormatOptions {
        FormatOptions {
            print_width,
            ..Default::default()
        }
    }

    // Simple text renders as-is.
    #[test]
    fn simple_text() {
        let doc = text("hello");
        assert_eq!(format(&doc, &opts(80)), "hello");
    }

    // Group that fits on one line → flat mode (single line).
    #[test]
    fn group_fits_flat() {
        let doc = group(concat(vec![
            text("["),
            indent(concat(vec![
                line(),
                text("a"),
                text(","),
                line(),
                text("b"),
            ])),
            line(),
            text("]"),
        ]));
        let result = format(&doc, &opts(80));
        // In flat mode, Line → space
        assert_eq!(result, "[ a, b ]");
    }

    // Group that doesn't fit → break mode (multi-line).
    #[test]
    fn group_breaks_when_too_wide() {
        let doc = group(concat(vec![
            text("["),
            indent(concat(vec![
                line(),
                text("long_item_a"),
                text(","),
                line(),
                text("long_item_b"),
            ])),
            line(),
            text("]"),
        ]));
        let result = format(&doc, &opts(10));
        // In break mode, Line → newline + indent
        assert!(
            result.contains('\n'),
            "expected newline in result: {result:?}"
        );
    }

    // Nested groups: inner group that fits stays flat even when outer breaks.
    #[test]
    fn nested_groups_independent() {
        // The inner group ("a b" = 3 chars when flat) is placed on a new
        // indented line when the outer breaks. At width 5 the outer cannot fit
        // on one line ("prefix: a b" = 12 chars), so it breaks. The inner
        // group starts fresh at col=2 (indent), where 3 chars fit → flat.
        let inner = group(concat(vec![text("a"), line(), text("b")]));
        let outer = group(concat(vec![
            text("prefix:"),
            indent(concat(vec![line(), inner])),
        ]));
        // Wide: everything fits on one line.
        assert_eq!(format(&outer, &opts(80)), "prefix: a b");
        // Narrow: outer breaks, inner stays flat on the indented line.
        let result = format(&outer, &opts(5));
        assert!(result.contains('\n'), "expected outer to break: {result:?}");
        assert!(
            result.contains("a b"),
            "expected inner to stay flat: {result:?}"
        );
    }

    // Indent increases indentation in break mode.
    #[test]
    fn indent_increases_indentation() {
        let doc = group(concat(vec![
            text("key:"),
            indent(concat(vec![line(), text("value")])),
        ]));
        let result = format(&doc, &opts(5));
        // Should break and indent "value" by tab_width (default 2)
        assert_eq!(result, "key:\n  value");
    }

    // HardLine forces break even inside a flat group.
    #[test]
    fn hard_line_forces_break_in_flat() {
        let doc = group(concat(vec![text("a"), hard_line(), text("b")]));
        let result = format(&doc, &opts(80));
        assert_eq!(result, "a\nb");
    }

    // FlatAlt uses flat variant in flat mode and break variant in break mode.
    #[test]
    fn flat_alt_modes() {
        let doc_flat = group(flat_alt(text("flat"), text("break")));
        assert_eq!(format(&doc_flat, &opts(80)), "flat");

        // Force break by making the group too wide.
        let doc_break = group(concat(vec![
            text("very_long_prefix_that_will_not_fit_on_one_line"),
            flat_alt(text("_flat"), text("_break")),
        ]));
        let result = format(&doc_break, &opts(10));
        assert!(result.ends_with("_break"), "result: {result:?}");
    }

    // join() intersperse separator between items.
    #[test]
    fn join_intersperse() {
        let sep = text(", ");
        let items = vec![text("a"), text("b"), text("c")];
        let doc = join(&sep, items);
        assert_eq!(format(&doc, &opts(80)), "a, b, c");
    }

    // join() with empty list returns empty.
    #[test]
    fn join_empty() {
        let sep = text(", ");
        let doc = join(&sep, vec![]);
        assert_eq!(format(&doc, &opts(80)), "");
    }

    // use_tabs produces tab characters.
    #[test]
    fn use_tabs() {
        let doc = group(concat(vec![
            text("key:"),
            indent(concat(vec![line(), text("value")])),
        ]));
        let options = FormatOptions {
            print_width: 5,
            use_tabs: true,
            tab_width: 4,
        };
        let result = format(&doc, &options);
        assert_eq!(result, "key:\n\tvalue");
    }

    // Various print_width settings.
    #[test]
    fn print_width_controls_breaks() {
        let items = vec![text("item_one"), text("item_two"), text("item_three")];
        let sep = concat(vec![text(","), line()]);
        let doc = group(concat(vec![
            text("["),
            indent(concat(vec![line(), join(&sep, items)])),
            line(),
            text("]"),
        ]));

        // Wide enough to fit on one line.
        let wide = format(&doc, &opts(80));
        assert!(!wide.contains('\n'));

        // Narrow: must break.
        let narrow = format(&doc, &opts(10));
        assert!(narrow.contains('\n'));
    }
}
