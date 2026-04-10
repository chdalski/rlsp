// SPDX-License-Identifier: MIT

use std::borrow::Cow;

use crate::error::Error;
use crate::event::Chomp;
use crate::lines::BreakType;
use crate::pos::{Pos, Span};

use super::{Lexer, pos_after_line};

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
    #[allow(clippy::too_many_lines)]
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
#[allow(clippy::too_many_lines)]
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
