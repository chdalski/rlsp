// SPDX-License-Identifier: MIT

use std::borrow::Cow;

use crate::error::Error;
use crate::event::Chomp;
use crate::lines::BreakType;
use crate::pos::{Pos, Span};

use super::Lexer;
use crate::lines::pos_after_line;

/// Return type of [`Lexer::try_consume_literal_block_scalar`].
///
/// `None` — not a literal block scalar.
/// `Some(Ok(...))` — successfully tokenized `(value, chomp, span)`.
/// `Some(Err(...))` — parse error.
pub(super) type LiteralBlockResult<'a> = Option<Result<(Cow<'a, str>, Chomp, Span), Error>>;

impl<'input> Lexer<'input> {
    /// Try to tokenize a literal block scalar (`|`) starting at the current line.
    ///
    /// Implements YAML 1.2 §8.1.2 `c-l+literal` in block context.  The caller
    /// supplies `parent_indent` — the indentation level of the enclosing block
    /// node (`n` in the spec).
    ///
    /// Returns:
    /// - `None` — the current line does not start with `|` (not a literal block scalar).
    /// - `Some(Ok((value, chomp, span)))` — successfully tokenized.
    /// - `Some(Err(e))` — started parsing (opening `|` seen) but hit a hard error
    ///   (e.g. invalid indicator character, tab in indentation).
    ///
    /// **Borrow contract:** Always returns `Cow::Owned` — the content is
    /// assembled from stripped lines and does not exist contiguously in input.
    ///
    /// **Span:** Covers from the `|` through the last consumed line terminator.
    #[expect(
        clippy::too_many_lines,
        reason = "match-on-event-type; splitting would obscure flow"
    )]
    pub fn try_consume_literal_block_scalar(
        &mut self,
        parent_indent: usize,
    ) -> LiteralBlockResult<'input> {
        // Check the current line starts with `|`.
        let first_line = self.buf.peek_next()?;
        let content = first_line.content.trim_start_matches([' ', '\t']);
        if !content.starts_with('|') {
            return None;
        }

        // Record the position of the `|` for the span start.
        let leading_bytes = first_line.content.len() - content.len();
        let leading_chars = crate::pos::column_at(first_line.content, leading_bytes);
        let pipe_pos = Pos {
            byte_offset: first_line.offset + leading_bytes,
            line: first_line.pos.line,
            column: first_line.pos.column + leading_chars,
        };

        // Consume the header line.
        // SAFETY: LineBuffer guarantees consume returns Some when peek returned
        // Some on the same instance (single-threaded, no interleaving).
        let Some(header_line) = self.buf.consume_next() else {
            unreachable!("peek returned Some but consume returned None")
        };
        self.current_pos = pos_after_line(&header_line);

        // Parse the header: `|` [indent-indicator] [chomp-indicator] [comment]
        // Indicators can appear in either order: `|+2` or `|2+`.
        let after_pipe = &content[1..]; // everything after `|`
        let (chomp, explicit_indent, header_err) = parse_block_header(after_pipe, pipe_pos);
        if let Some(e) = header_err {
            return Some(Err(e));
        }

        // Determine content indent.
        // If explicit indicator given: content_indent = parent_indent + explicit_indent.
        // Otherwise: auto-detect by scanning forward to the first non-blank content line.
        // `content_indent_known` is true when content_indent is derived from an actual
        // non-blank line (or an explicit indicator); false when it falls back to the
        // default (parent_indent + 1) because no content lines exist.  The spec rule
        // "leading empty lines must not exceed the first non-empty line's indent" only
        // applies when there IS a first non-empty line (content_indent_known = true).
        // `effective_min` is `parent_indent + 1` when parent_indent is a real
        // value, or `1` when parent_indent == usize::MAX (root-level sentinel,
        // meaning the scalar has no enclosing collection and body lines may
        // start at column 0 per YAML spec §8.1: body indent > -1).
        let effective_min = if parent_indent == usize::MAX {
            1_usize
        } else {
            parent_indent + 1
        };

        let (content_indent, content_indent_known): (usize, bool) =
            if let Some(explicit) = explicit_indent {
                // Explicit indent indicator: content_indent = parent_indent + indicator.
                // For root level (usize::MAX), treat parent as 0 for arithmetic.
                let base = if parent_indent == usize::MAX {
                    0
                } else {
                    parent_indent
                };
                (base + explicit, true)
            } else {
                // Use peek_until_dedent to scan for the first non-blank line and
                // read its indent.  base_indent = parent_indent so we stop at
                // any line with indent <= parent_indent (usize::MAX means no limit).
                let lookahead = self.buf.peek_until_dedent(parent_indent);
                // Find the first non-blank line's indent.
                lookahead
                    .iter()
                    .find(|l| !l.content.trim_matches([' ', '\t']).is_empty())
                    .map_or((effective_min, false), |l| (l.indent, true))
            };

        // Collect content lines.
        let mut out = String::new();
        // Count pending transparent blank lines (not yet pushed, for chomping).
        // These are lines with indent < content_indent (or truly empty lines).
        let mut trailing_newlines: usize = 0;
        // Track whether we've seen the first line with actual non-whitespace
        // characters.  Per YAML 1.2 §8.1.1.1: "It is an error for any of the
        // leading empty lines to contain more spaces than the first non-empty
        // line."  A spaces-only line with indent > content_indent is therefore
        // an error if it precedes any real content.
        let mut before_first_real_content = true;

        loop {
            let Some(next) = self.buf.peek_next() else {
                break;
            };

            let line_content = next.content;

            // Tab at the very start of a line means the line uses a tab as
            // indentation, which is a YAML error.
            if line_content.starts_with('\t') {
                let tab_pos = next.pos;
                // SAFETY: peek succeeded above; LineBuffer invariant.
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume failed")
                };
                self.current_pos = pos_after_line(&consumed);
                return Some(Err(Error {
                    pos: tab_pos,
                    message: "tab character is not valid indentation in a block scalar".to_owned(),
                }));
            }

            // Classify this line:
            // - If indent >= content_indent: content line (may be spaces-only
            //   after the indent prefix, but that's still content).
            // - Otherwise (indent < content_indent): transparent blank — counts
            //   as a newline but does not terminate if the line is whitespace-only
            //   (per spec `l-empty(n,c)`). If it has non-whitespace characters,
            //   it's a dedent terminator.
            // A line is a content line if:
            // 1. Its indent >= content_indent, AND
            // 2. After stripping content_indent leading spaces, at least one
            //    nb-char (non-break char, including spaces) remains.
            //
            // A line is blank (l-empty) if it has indent < content_indent, or
            // if after stripping content_indent spaces the remaining content is
            // completely empty. In the blank case we check for non-whitespace
            // to decide between transparent (blank → newline) vs terminator.
            let after_indent = if next.indent >= content_indent {
                line_content.get(content_indent..).unwrap_or("")
            } else {
                ""
            };

            let is_content_line = next.indent >= content_indent && !after_indent.is_empty();

            if is_content_line {
                // Leading all-whitespace lines with indent > content_indent are
                // errors (spec §8.1.1.1: leading empty lines must not exceed the
                // first non-empty line's indentation).  Only applies when
                // content_indent was derived from an actual non-blank line or an
                // explicit indicator (content_indent_known = true).
                let has_real_content = !after_indent.trim_end_matches([' ', '\t']).is_empty();
                if content_indent_known
                    && before_first_real_content
                    && !has_real_content
                    && next.indent > content_indent
                {
                    let blank_pos = next.pos;
                    let Some(consumed) = self.buf.consume_next() else {
                        unreachable!("consume over-indented blank failed")
                    };
                    self.current_pos = pos_after_line(&consumed);
                    return Some(Err(Error {
                        pos: blank_pos,
                        message: "block scalar blank line has more indentation than the content"
                            .to_owned(),
                    }));
                }
                if has_real_content {
                    before_first_real_content = false;
                }

                // Content line. Flush any pending blank lines first.
                for _ in 0..trailing_newlines {
                    out.push('\n');
                }
                trailing_newlines = 0;

                // SAFETY: peek succeeded on this loop iteration; LineBuffer invariant.
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume content line failed")
                };
                self.current_pos = pos_after_line(&consumed);

                out.push_str(after_indent);
                // Only push a newline if the physical line had one.
                // A line ending with BreakType::Eof means the input ended
                // without a trailing newline — no b-as-line-feed is emitted.
                if consumed.break_type != BreakType::Eof {
                    out.push('\n');
                }
            } else {
                // Blank line (indent < content_indent, or content after indent
                // is empty). Check whether it terminates the scalar.
                let trimmed = line_content.trim_matches([' ', '\t']);
                if !trimmed.is_empty() {
                    // Non-whitespace at dedented position: terminates the scalar.
                    break;
                }
                // Whitespace-only line: transparent (l-empty). Count as newline.
                // SAFETY: peek succeeded on this loop iteration; LineBuffer invariant.
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume blank line failed")
                };
                self.current_pos = pos_after_line(&consumed);
                trailing_newlines += 1;
            }
        }

        // Apply chomping to the trailing newlines.
        // At this point `out` ends with `\n` from the last content line (if any),
        // and `trailing_newlines` counts blank lines following that last content line.
        let value = apply_chomping(out, trailing_newlines, chomp);

        let span = Span {
            start: pipe_pos,
            end: self.current_pos,
        };

        Some(Ok((Cow::Owned(value), chomp, span)))
    }

    /// Try to tokenize a folded block scalar (`>`) starting at the current line.
    ///
    /// Implements YAML 1.2 §8.1.3 `c-l+folded` in block context.  The caller
    /// supplies `parent_indent` — the indentation level of the enclosing block
    /// node (`n` in the spec).
    ///
    /// **Borrow contract:** Always returns `Cow::Owned` — folding assembles a
    /// transformed string that does not exist contiguously in the input.
    ///
    /// **Security:** Collection is O(n) in input size; no amplification.
    /// `peek_until_dedent` may scan O(n) ahead — pre-existing constraint shared
    /// with literal scalars.
    pub fn try_consume_folded_block_scalar(
        &mut self,
        parent_indent: usize,
    ) -> LiteralBlockResult<'input> {
        let first_line = self.buf.peek_next()?;
        let content = first_line.content.trim_start_matches([' ', '\t']);
        if !content.starts_with('>') {
            return None;
        }

        let leading_bytes = first_line.content.len() - content.len();
        let leading_chars = crate::pos::column_at(first_line.content, leading_bytes);
        let gt_pos = Pos {
            byte_offset: first_line.offset + leading_bytes,
            line: first_line.pos.line,
            column: first_line.pos.column + leading_chars,
        };

        // SAFETY: LineBuffer guarantees consume returns Some when peek returned
        // Some on the same instance (single-threaded, no interleaving).
        let Some(header_line) = self.buf.consume_next() else {
            unreachable!("peek returned Some but consume returned None")
        };
        self.current_pos = pos_after_line(&header_line);

        // Parse the header — reuse `parse_block_header`; works identically for `>` and `|`.
        let after_gt = &content[1..];
        let (chomp, explicit_indent, header_err) = parse_block_header(after_gt, gt_pos);
        if let Some(e) = header_err {
            return Some(Err(e));
        }

        let effective_min = if parent_indent == usize::MAX {
            1_usize
        } else {
            parent_indent + 1
        };
        let content_indent: usize = if let Some(explicit) = explicit_indent {
            let base = if parent_indent == usize::MAX {
                0
            } else {
                parent_indent
            };
            base + explicit
        } else {
            let lookahead = self.buf.peek_until_dedent(parent_indent);
            lookahead
                .iter()
                .find(|l| !l.content.trim_matches([' ', '\t']).is_empty())
                .map_or(effective_min, |l| l.indent)
        };

        let (content_result, trailing_newlines) = self.collect_folded_lines(content_indent);
        let folded = match content_result {
            Ok(s) => s,
            Err(e) => return Some(Err(e)),
        };
        let value = apply_chomping(folded, trailing_newlines, chomp);
        let span = Span {
            start: gt_pos,
            end: self.current_pos,
        };
        Some(Ok((Cow::Owned(value), chomp, span)))
    }

    /// Collect and fold content lines for a folded block scalar.
    ///
    /// Returns `(content, trailing_blank_count)`.
    ///
    /// The physical line break after each content line is deferred — it becomes
    /// the inter-line separator (space, `\n`, or N newlines) when the next line
    /// is classified, per YAML 1.2 §8.1.3 folding rules:
    ///
    /// - Single break, both lines equally indented → space.
    /// - Single break surrounding a more-indented line → `\n` (preserved).
    /// - N blank lines between non-blank lines → N `\n`s.
    fn collect_folded_lines(&mut self, content_indent: usize) -> (Result<String, Error>, usize) {
        let mut out = String::new();
        let mut trailing_newlines: usize = 0;
        let mut last_had_break = false;
        let mut prev_more_indented = false;
        let mut has_content = false;

        loop {
            let Some(next) = self.buf.peek_next() else {
                break;
            };

            let line_content = next.content;

            if line_content.starts_with('\t') {
                let tab_pos = next.pos;
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume failed")
                };
                self.current_pos = pos_after_line(&consumed);
                return (
                    Err(Error {
                        pos: tab_pos,
                        message: "tab character is not valid indentation in a block scalar"
                            .to_owned(),
                    }),
                    0,
                );
            }

            let after_indent = if next.indent >= content_indent {
                line_content.get(content_indent..).unwrap_or("")
            } else {
                ""
            };
            // A content line must have a non-whitespace character after the indent.
            let is_content_line = next.indent >= content_indent
                && !after_indent.trim_end_matches([' ', '\t']).is_empty();

            if is_content_line {
                let is_more_indented = next.indent > content_indent;
                if has_content {
                    if trailing_newlines > 0 {
                        // N blank lines → N newlines (first break discarded).
                        for _ in 0..trailing_newlines {
                            out.push('\n');
                        }
                    } else if prev_more_indented || is_more_indented {
                        // Break surrounding a more-indented line is preserved.
                        out.push('\n');
                    } else {
                        // Single break between equally-indented lines → space.
                        out.push(' ');
                    }
                } else {
                    // Leading blank lines before first content line.
                    for _ in 0..trailing_newlines {
                        out.push('\n');
                    }
                }
                trailing_newlines = 0;

                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume content line failed")
                };
                self.current_pos = pos_after_line(&consumed);
                out.push_str(after_indent);
                // Defer the physical break — decided as separator by the next line.
                last_had_break = consumed.break_type != BreakType::Eof;
                prev_more_indented = is_more_indented;
                has_content = true;
            } else {
                let trimmed = line_content.trim_matches([' ', '\t']);
                if !trimmed.is_empty() {
                    break; // Dedented non-whitespace terminates the scalar.
                }
                // Whitespace-only blank line: validate l-empty indentation constraint.
                // Per YAML 1.2 §8.1.1 rule 175, a blank line must have at most
                // content_indent leading spaces.  More spaces is a parse error.
                if next.indent > content_indent {
                    let blank_pos = next.pos;
                    let Some(consumed) = self.buf.consume_next() else {
                        unreachable!("consume over-indented blank failed")
                    };
                    self.current_pos = pos_after_line(&consumed);
                    return (
                        Err(Error {
                            pos: blank_pos,
                            message:
                                "block scalar blank line has more indentation than the content"
                                    .to_owned(),
                        }),
                        0,
                    );
                }
                let Some(consumed) = self.buf.consume_next() else {
                    unreachable!("consume blank line failed")
                };
                self.current_pos = pos_after_line(&consumed);
                trailing_newlines += 1;
            }
        }

        // Append the final content line's physical break so `apply_chomping`
        // sees the canonical `\n`-terminated content.
        if has_content && last_had_break {
            out.push('\n');
        }

        (Ok(out), trailing_newlines)
    }
}

/// Parse the block scalar header following the `|` character.
///
/// `after_pipe` is the slice starting immediately after `|`.
/// Returns `(chomp, explicit_indent, error)`.
///
/// - `explicit_indent` is `Some(n)` for `|n` or `None` for auto-detect.
/// - Error is `Some(Error)` for invalid indicator characters.
#[expect(
    clippy::too_many_lines,
    reason = "match-on-event-type; splitting would obscure flow"
)]
pub(super) fn parse_block_header(
    after_pipe: &str,
    pipe_pos: Pos,
) -> (Chomp, Option<usize>, Option<Error>) {
    // Try to parse indicator characters. They can appear in either order:
    // indent-then-chomp or chomp-then-indent.
    let mut chomp: Option<Chomp> = None;
    let mut explicit_indent: Option<usize> = None;
    let mut pos = pipe_pos.advance('|');
    let mut byte_offset: usize = 0;

    // We track how many indicator chars we consumed to detect `|99` (two digits).
    loop {
        let remaining = &after_pipe[byte_offset..];
        match remaining.chars().next() {
            None | Some(' ' | '\t' | '\n' | '\r') => {
                // End of indicators: whitespace or line end.
                break;
            }
            Some('#') => {
                // `#` immediately after indicator (no whitespace) is an error.
                return (
                    Chomp::Clip,
                    None,
                    Some(Error {
                        pos,
                        message:
                            "comment after block scalar indicator requires at least one space before '#'"
                                .to_owned(),
                    }),
                );
            }
            Some(ch) => {
                if ch == '+' {
                    if chomp.is_some() {
                        return (
                            Chomp::Clip,
                            None,
                            Some(Error {
                                pos,
                                message: "duplicate chomp indicator in block scalar header"
                                    .to_owned(),
                            }),
                        );
                    }
                    chomp = Some(Chomp::Keep);
                    byte_offset += ch.len_utf8();
                    pos = pos.advance(ch);
                } else if ch == '-' {
                    if chomp.is_some() {
                        return (
                            Chomp::Clip,
                            None,
                            Some(Error {
                                pos,
                                message: "duplicate chomp indicator in block scalar header"
                                    .to_owned(),
                            }),
                        );
                    }
                    chomp = Some(Chomp::Strip);
                    byte_offset += ch.len_utf8();
                    pos = pos.advance(ch);
                } else if ch.is_ascii_digit() {
                    if ch == '0' {
                        return (
                            Chomp::Clip,
                            None,
                            Some(Error {
                                pos,
                                message: "indent indicator '0' is not valid in block scalar header"
                                    .to_owned(),
                            }),
                        );
                    }
                    if explicit_indent.is_some() {
                        return (
                            Chomp::Clip,
                            None,
                            Some(Error {
                                pos,
                                message: "duplicate indent indicator in block scalar header"
                                    .to_owned(),
                            }),
                        );
                    }
                    explicit_indent = Some(ch as usize - '0' as usize);
                    byte_offset += ch.len_utf8();
                    pos = pos.advance(ch);
                } else {
                    // Invalid indicator character.
                    return (
                        Chomp::Clip,
                        None,
                        Some(Error {
                            pos,
                            message: format!("invalid block scalar indicator character '{ch}'"),
                        }),
                    );
                }
            }
        }
    }

    // After indicators, only optional whitespace followed by optional comment
    // (or end of line) is allowed.  Non-whitespace, non-comment content is invalid.
    let remaining = &after_pipe[byte_offset..];
    let after_ws = remaining.trim_start_matches([' ', '\t']);
    if !after_ws.is_empty() && !after_ws.starts_with('#') {
        // Non-comment, non-whitespace content after indicators.
        return (
            Chomp::Clip,
            None,
            Some(Error {
                pos,
                message: "invalid content after block scalar indicator".to_owned(),
            }),
        );
    }

    (chomp.unwrap_or(Chomp::Clip), explicit_indent, None)
}

/// Apply chomping rules to the assembled scalar content.
///
/// `content` is the raw assembled content (ends with `\n` from the last
/// content line, if any content exists).
/// `trailing_blank_count` is the number of blank lines that followed the last
/// content line.
///
/// Chomping rules (spec §8.1.1.2):
/// - Strip: remove all trailing newlines (the `\n` from the last content line
///   and any blank lines).
/// - Clip: keep exactly one trailing newline (the `\n` from the last content
///   line).  If content is empty, result is "".
/// - Keep: keep the `\n` from the last content line plus all blank lines.
pub(super) fn apply_chomping(
    mut content: String,
    trailing_blank_count: usize,
    chomp: Chomp,
) -> String {
    match chomp {
        Chomp::Strip => {
            // Remove the trailing `\n` added after the last content line,
            // plus any blank lines.
            if content.ends_with('\n') {
                content.pop();
            }
            // content already has no trailing blanks (they were counted separately).
        }
        Chomp::Clip => {
            // Keep exactly one trailing `\n`.  The content already ends with `\n`
            // (if non-empty) from the last content line — that's the one to keep.
            // Blank lines are discarded.
        }
        Chomp::Keep => {
            // Keep the trailing `\n` from the last content line plus all blank lines.
            for _ in 0..trailing_blank_count {
                content.push('\n');
            }
        }
    }
    content
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use rstest::rstest;

    use super::*;
    use crate::event::Chomp;

    fn make_lexer(input: &str) -> Lexer<'_> {
        Lexer::new(input)
    }

    // -----------------------------------------------------------------------
    // Group H — try_consume_literal_block_scalar unit tests (Task 8)
    // -----------------------------------------------------------------------
    //
    // Helpers for literal block scalar tests.

    /// Parse a literal block scalar from `input`, returning Ok((value, chomp)).
    /// Panics if the result is None or Err.
    fn lit_ok(input: &str) -> (String, Chomp) {
        let mut lex = make_lexer(input);
        let result = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some, got None"));
        let (cow, chomp, _span) =
            result.unwrap_or_else(|e| unreachable!("expected Ok, got Err: {e}"));
        (cow.into_owned(), chomp)
    }

    /// Parse a literal block scalar from `input`, expecting an error.
    fn lit_err(input: &str) -> crate::error::Error {
        let mut lex = make_lexer(input);
        let result = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some, got None"));
        match result {
            Err(e) => e,
            Ok(_) => unreachable!("expected Err, got Ok"),
        }
    }

    /// Try a literal block scalar; returns None if not a block scalar.
    fn lit_none(input: &str) -> bool {
        let mut lex = make_lexer(input);
        lex.try_consume_literal_block_scalar(0).is_none()
    }

    // -----------------------------------------------------------------------
    // Group H-A: Header parsing — happy path
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::no_indicators_yields_clip("|\n  hello\n", Chomp::Clip)]
    #[case::minus_yields_strip("|-\n  hello\n", Chomp::Strip)]
    #[case::plus_yields_keep("|+\n  hello\n", Chomp::Keep)]
    fn literal_header_chomp_only(#[case] input: &str, #[case] expected: Chomp) {
        let (_, chomp) = lit_ok(input);
        assert_eq!(chomp, expected);
    }

    // UT-LB-A4: `|2` → explicit indent 2 (relative to parent=0)
    #[test]
    fn literal_header_explicit_indent_2() {
        let (val, _) = lit_ok("|2\n  hello\n");
        assert_eq!(val, "hello\n");
    }

    #[rstest]
    #[case::minus_indent_2("|-2\n  hello\n", "hello", Chomp::Strip)]
    #[case::indent_2_then_minus("|2-\n  hello\n", "hello", Chomp::Strip)]
    #[case::plus_indent_2("|+2\n  hello\n\n", "hello\n\n", Chomp::Keep)]
    #[case::indent_2_then_plus("|2+\n  hello\n\n", "hello\n\n", Chomp::Keep)]
    #[case::with_comment_yields_clip("| # this is a comment\n  hello\n", "hello\n", Chomp::Clip)]
    #[case::leading_spaces_before_pipe("  |\n    hello\n", "hello\n", Chomp::Clip)]
    #[case::space_then_comment_gives_clip("|  # comment\n  hello\n", "hello\n", Chomp::Clip)]
    #[case::explicit_indent_nine("|9\n         foo\n", "foo\n", Chomp::Clip)]
    #[case::empty_scalar_clip_yields_empty("|\n", "", Chomp::Clip)]
    fn literal_header_val_and_chomp(
        #[case] input: &str,
        #[case] expected_val: &str,
        #[case] expected_chomp: Chomp,
    ) {
        let (val, chomp) = lit_ok(input);
        assert_eq!(val, expected_val);
        assert_eq!(chomp, expected_chomp);
    }

    // UT-LB-A10: returns None for non-`|` input
    #[test]
    fn literal_block_returns_none_for_non_pipe() {
        assert!(lit_none("hello\n"));
    }

    // UT-LB-A11: returns None for empty input
    #[test]
    fn literal_block_returns_none_for_empty_input() {
        assert!(lit_none(""));
    }

    // -----------------------------------------------------------------------
    // Group H-B: Header parsing — errors
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::invalid_indicator_exclamation("|!\n  hello\n", "invalid")]
    #[case::zero_indent_forbidden("|0\n  hello\n", "'0'")]
    #[case::duplicate_indent_digit("|99\n  hello\n", "duplicate")]
    #[case::duplicate_chomp_keep("|++\n  hello\n", "duplicate")]
    #[case::duplicate_chomp_strip("|--\n  hello\n", "duplicate")]
    #[case::mixed_chomp_indicators("|+-\n  hello\n", "duplicate")]
    #[case::invalid_char_after_digit("|2!\n  hello\n", "invalid")]
    fn literal_header_errors(#[case] input: &str, #[case] expected_substring: &str) {
        let e = lit_err(input);
        assert!(
            e.message.contains(expected_substring),
            "expected message containing {expected_substring:?}, got: {}",
            e.message
        );
    }

    // -----------------------------------------------------------------------
    // Group H-C: Clip content collection
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::single_line("|\n  hello\n", "hello\n")]
    #[case::multi_line("|\n  foo\n  bar\n", "foo\nbar\n")]
    #[case::blank_line_in_content("|\n  foo\n\n  bar\n", "foo\n\nbar\n")]
    // Per YAML 1.2 §8.1.2, blank lines before the first content line
    // are included as newlines via l-empty.  A completely empty line
    // has s-indent(0) which satisfies l-empty(n,BLOCK-IN) for any n>0.
    #[case::leading_blank_before_first_content("|\n\n  foo\n", "\nfoo\n")]
    #[case::two_interior_blank_lines_preserved("|\n  foo\n\n\n  bar\n", "foo\n\n\nbar\n")]
    #[case::empty_scalar_with_trailing_blank_still_empty("|\n\n", "")]
    #[case::trailing_blank_clips_to_one("|\n  foo\n\n", "foo\n")]
    #[case::two_trailing_blanks_dropped("|\n  foo\n\n\n", "foo\n")]
    // "|\n  foo\n   bar\n" with content_indent=2: bar has 1 extra space
    #[case::extra_indent_preserves_spaces("|\n  foo\n   bar\n", "foo\n bar\n")]
    // "|\n  foo" — no final newline; no b-as-line-feed, so value is "foo".
    #[case::eof_without_trailing_newline("|\n  foo", "foo")]
    fn literal_clip_content_val(#[case] input: &str, #[case] expected_val: &str) {
        let (val, _) = lit_ok(input);
        assert_eq!(val, expected_val);
    }

    // UT-LB-C8: content terminated by dedent
    #[test]
    fn literal_content_terminated_by_dedent() {
        let mut lex = make_lexer("|\n  foo\nkey: val\n");
        let result = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert_eq!(result.0.as_ref(), "foo\n");
        // `key: val` should still be in the buffer.
        let remaining = lex.buf.peek_next().map(|l| l.content);
        assert_eq!(remaining, Some("key: val"));
    }

    // -----------------------------------------------------------------------
    // Group H-D: Strip and Keep chomping
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::strip_no_trailing_newline("|-\n  foo\n", "foo", Chomp::Strip)]
    #[case::strip_empty_scalar("|-\n", "", Chomp::Strip)]
    #[case::keep_all_trailing_newlines("|+\n  foo\n\n\n", "foo\n\n\n", Chomp::Keep)]
    #[case::keep_empty_scalar("|+\n", "", Chomp::Keep)]
    fn literal_chomp_val_and_chomp(
        #[case] input: &str,
        #[case] expected_val: &str,
        #[case] expected_chomp: Chomp,
    ) {
        let (val, chomp) = lit_ok(input);
        assert_eq!(val, expected_val);
        assert_eq!(chomp, expected_chomp);
    }

    #[rstest]
    #[case::strip_trailing_blanks_removes_all("|-\n  foo\n\n\n", "foo")]
    #[case::keep_single_trailing_newline("|+\n  foo\n", "foo\n")]
    #[case::clip_no_trailing_blank("|\n  foo\n", "foo\n")]
    #[case::clip_multiple_trailing_blanks("|\n  foo\n\n\n\n", "foo\n")]
    #[case::strip_multiline_removes_last_newline("|-\n  foo\n  bar\n", "foo\nbar")]
    #[case::keep_multiline_preserves_all_trailing("|+\n  foo\n  bar\n\n", "foo\nbar\n\n")]
    #[case::keep_only_blanks_produces_newlines("|+\n\n\n", "\n\n")]
    #[case::strip_only_blanks_produces_empty("|-\n\n\n", "")]
    #[case::clip_only_blanks_produces_empty("|\n\n\n", "")]
    fn literal_chomp_val_only(#[case] input: &str, #[case] expected_val: &str) {
        let (val, _) = lit_ok(input);
        assert_eq!(val, expected_val);
    }

    // -----------------------------------------------------------------------
    // Group H-E: Explicit indent indicator
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::indent_2_parent_0("|2\n  foo\n", "foo\n")]
    #[case::indent_2_extra_spaces_preserved("|2\n   foo\n", " foo\n")]
    // content_indent=4, foo has indent=2 < 4 → empty scalar
    #[case::indent_content_insufficient_yields_empty("|4\n  foo\n", "")]
    #[case::indent_1_parent_0("|1\n foo\n", "foo\n")]
    fn literal_explicit_indent_val(#[case] input: &str, #[case] expected_val: &str) {
        let (val, _) = lit_ok(input);
        assert_eq!(val, expected_val);
    }

    // UT-LB-E5: explicit indent 2 relative to parent_indent=2 → content_indent=4
    #[test]
    fn explicit_indent_relative_to_parent() {
        let mut lex = make_lexer("|2\n    foo\n");
        let result = lex
            .try_consume_literal_block_scalar(2)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        // parent_indent=2 + explicit=2 = content_indent=4; "    foo" has indent=4
        assert_eq!(result.0.as_ref(), "foo\n");
    }

    // -----------------------------------------------------------------------
    // Group H-F: Termination/boundary conditions
    // -----------------------------------------------------------------------

    // UT-LB-F1: block scalar followed by non-blank content at col 0
    #[test]
    fn literal_stops_at_dedented_non_blank() {
        let mut lex = make_lexer("|\n  foo\nnext line\n");
        let (val, _, _) = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert_eq!(val.as_ref(), "foo\n");
        let remaining = lex.buf.peek_next().map(|l| l.content);
        assert_eq!(remaining, Some("next line"));
    }

    // UT-LB-F2: block scalar at EOF (no lines at all after header)
    #[test]
    fn literal_at_eof_after_header_yields_empty() {
        let (val, _) = lit_ok("|\n");
        assert_eq!(val, "");
    }

    // UT-LB-F3: span start is at position of `|`
    #[test]
    fn literal_span_start_at_pipe() {
        let mut lex = make_lexer("|\n  hello\n");
        let (_, _, span) = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert_eq!(span.start.byte_offset, 0);
        assert_eq!(span.start.column, 0);
    }

    // UT-LB-F4: span end after all consumed lines
    #[test]
    fn literal_span_end_after_content_lines() {
        // "|\n  hello\n" = 10 bytes
        let mut lex = make_lexer("|\n  hello\n");
        let (_, _, span) = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert_eq!(span.end.byte_offset, 10);
    }

    // UT-LB-F5: span end covers trailing blanks that are consumed
    #[test]
    fn literal_span_end_covers_trailing_blanks() {
        // "|\n  foo\n\n" = 9 bytes (|=1, \n=1, space=1, space=1, foo=3, \n=1, \n=1)
        // trailing blank is consumed even under Clip
        let mut lex = make_lexer("|\n  foo\n\n");
        let (_, _, span) = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert_eq!(span.end.byte_offset, 9);
    }

    // -----------------------------------------------------------------------
    // Group H-G: Tab handling
    // -----------------------------------------------------------------------

    // UT-LB-G1: tab as first char of content line → error
    #[test]
    fn literal_tab_as_indentation_is_error() {
        let e = lit_err("|\n\tfoo\n");
        assert!(
            e.message.contains("tab"),
            "expected tab error, got: {}",
            e.message
        );
    }

    // UT-LB-G2: tab after content-indent spaces → preserved in value
    #[test]
    fn literal_tab_after_spaces_is_preserved() {
        // "|\n  \tfoo\n": content_indent=2 (from `  \t`), after stripping 2 spaces: "\tfoo"
        // The tab is after the indent spaces — it's content.
        let (val, _) = lit_ok("|\n  \tfoo\n");
        assert_eq!(val, "\tfoo\n");
    }

    // -----------------------------------------------------------------------
    // Group H-H: UTF-8 and special content
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::multibyte_utf8("|\n  héllo\n", "héllo\n")]
    // Backslashes are not escape sequences in literal block scalars.
    #[case::backslash_preserved_verbatim("|\n  foo\\bar\n", "foo\\bar\n")]
    fn literal_special_content_val(#[case] input: &str, #[case] expected_val: &str) {
        let (val, _) = lit_ok(input);
        assert_eq!(val, expected_val);
    }

    // UT-LB-H3: result is always Cow::Owned
    #[test]
    fn literal_result_is_always_cow_owned() {
        let mut lex = make_lexer("|\n  hello\n");
        let (cow, _, _) = lex
            .try_consume_literal_block_scalar(0)
            .unwrap_or_else(|| unreachable!("expected Some"))
            .unwrap_or_else(|e| unreachable!("expected Ok, got {e}"));
        assert!(
            matches!(cow, Cow::Owned(_)),
            "literal block scalars must always produce Cow::Owned"
        );
    }
}
