// SPDX-License-Identifier: MIT

//! Document-mode stepper.
//!
//! Contains `step_in_document`, the main dispatcher called by `Iterator::next`
//! when the parser is in `IterState::InDocument`.

use super::directive_scope::DirectiveScope;
use super::line_mapping::{find_value_indicator_offset, inline_contains_mapping_key};
use super::properties::{scan_anchor_name, scan_tag};
use super::state::{
    CollectionEntry, IterState, MappingPhase, PendingAnchor, PendingTag, StepResult,
};
use crate::error::Error;
use crate::event::Event;
use crate::pos::{Pos, Span};
use crate::{EventIter, marker_span, zero_span};

impl<'input> EventIter<'input> {
    /// Handle one iteration step in the `InDocument` state.
    #[allow(clippy::too_many_lines)]
    pub(in crate::event_iter) fn step_in_document(&mut self) -> StepResult<'input> {
        match self.skip_and_collect_comments_in_doc() {
            Ok(()) => {}
            Err(e) => {
                self.state = IterState::Done;
                return StepResult::Yield(Err(e));
            }
        }
        // If comments were queued, drain them before checking document state.
        if !self.queue.is_empty() {
            return StepResult::Continue;
        }

        // ---- Tab indentation check ----
        //
        // YAML 1.2 §6.1: tabs cannot be used for indentation in block context.
        // Only lines whose VERY FIRST character is `\t` (no leading spaces) are
        // using a tab as the indentation character and must be rejected.
        //
        // Exceptions: `\t[`, `\t{`, `\t]`, `\t}` are allowed because flow
        // collection delimiters can follow tabs (YAML test suite 6CA3, Q5MG).
        // Lines like `  \tx` have SPACES as indentation; the tab is content.
        if let Some(line) = self.lexer.peek_next_line() {
            if line.content.starts_with('\t') {
                // First char is a tab — check what the first non-tab character
                // is.  Flow collection delimiters are allowed after leading tabs.
                let first_non_tab = line.content.trim_start_matches('\t').chars().next();
                if !matches!(first_non_tab, Some('[' | '{' | ']' | '}')) {
                    let err_pos = line.pos;
                    self.state = IterState::Done;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "tabs are not allowed as indentation (YAML 1.2 §6.1)".into(),
                    }));
                }
            }
        }

        // ---- Document / stream boundaries ----

        if self.lexer.at_eof() && !self.lexer.has_inline_scalar() {
            let end = self.lexer.drain_to_end();
            self.close_all_collections(end);
            self.queue
                .push_back((Event::DocumentEnd { explicit: false }, zero_span(end)));
            self.queue.push_back((Event::StreamEnd, zero_span(end)));
            self.state = IterState::Done;
            return StepResult::Continue;
        }
        if self.lexer.is_document_end() {
            let pos = self.lexer.current_pos();
            self.close_all_collections(pos);
            let (marker_pos, _) = self.lexer.consume_marker_line(true);
            if let Some(e) = self.lexer.marker_inline_error.take() {
                self.state = IterState::Done;
                return StepResult::Yield(Err(e));
            }
            // Reset directive scope at the document boundary so directives from
            // this document do not leak into the next one.
            self.directive_scope = DirectiveScope::default();
            self.state = IterState::BetweenDocs;
            self.queue.push_back((
                Event::DocumentEnd { explicit: true },
                marker_span(marker_pos),
            ));
            self.drain_trailing_comment();
            return StepResult::Continue;
        }
        if self.lexer.is_directives_end() {
            let pos = self.lexer.current_pos();
            self.close_all_collections(pos);
            let (marker_pos, _) = self.lexer.consume_marker_line(false);
            if let Some(e) = self.lexer.marker_inline_error.take() {
                self.state = IterState::Done;
                return StepResult::Yield(Err(e));
            }
            // A bare `---` inside a document implicitly ends the current document
            // and starts a new one without a preamble.  Reset the directive scope
            // here since consume_preamble_between_docs will not be called for this
            // transition.
            self.directive_scope = DirectiveScope::default();
            // Validate any inline tag on this `---` line against the new
            // document's (empty) directive scope.  Tags defined in the previous
            // document do not carry over (YAML §9.2), so an undefined handle
            // must fail immediately.
            if let Some((tag_val, tag_pos)) = self.lexer.peek_inline_scalar() {
                if tag_val.starts_with('!') {
                    if let Err(e) = self.directive_scope.resolve_tag(tag_val, tag_pos) {
                        self.lexer.drain_inline_scalar();
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(e));
                    }
                }
            }
            self.state = IterState::InDocument;
            self.root_node_emitted = false;
            self.queue.push_back((
                Event::DocumentEnd { explicit: false },
                zero_span(marker_pos),
            ));
            self.queue.push_back((
                Event::DocumentStart {
                    explicit: true,
                    version: None,
                    tag_directives: Vec::new(),
                },
                marker_span(marker_pos),
            ));
            self.drain_trailing_comment();
            return StepResult::Continue;
        }

        // ---- Directive lines (`%YAML`/`%TAG`) inside document body ----
        //
        // YAML 1.2 §9.2: directives can only appear in the preamble (before
        // `---`).  A `%YAML` or `%TAG` line inside a document body, followed
        // by `---`, indicates the author forgot to close the previous document
        // with `...` before writing the next document's preamble.
        //
        // We only fire the error when:
        //   1. The current line starts with `%YAML ` or `%TAG ` (a genuine
        //      YAML directive keyword, not arbitrary content like `%!PS-Adobe`).
        //   2. The following line is a `---` document-start marker.
        //
        // This avoids false positives when `%` appears as content in plain
        // scalars (XLQ9) or inside block scalar bodies (M7A3, W4TN).
        if let Some(line) = self.lexer.peek_next_line() {
            let is_yaml_directive =
                line.content.starts_with("%YAML ") || line.content.starts_with("%TAG ");
            if is_yaml_directive {
                let next_is_doc_start = self.lexer.peek_second_line().is_some_and(|l| {
                    l.content == "---"
                        || l.content.starts_with("--- ")
                        || l.content.starts_with("---\t")
                });
                if next_is_doc_start {
                    let err_pos = line.pos;
                    self.state = IterState::Done;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message:
                            "directive '%' is only valid before the document-start marker '---'"
                                .into(),
                    }));
                }
            }
        }

        // ---- Root-node guard ----
        //
        // A YAML document contains exactly one root node.  Once the root has
        // been fully emitted (`root_node_emitted = true`) and the collection
        // stack is empty, any further non-comment, non-blank content is invalid.
        if self.root_node_emitted && self.coll_stack.is_empty() && !self.lexer.has_inline_scalar() {
            if let Some(line) = self.lexer.peek_next_line() {
                let err_pos = line.pos;
                self.state = IterState::Done;
                self.lexer.consume_line();
                return StepResult::Yield(Err(Error {
                    pos: err_pos,
                    message: "unexpected content after document root node".into(),
                }));
            }
        }

        // ---- Alias node: `*name` is a complete node ----

        if let Some(peek) = self.lexer.peek_next_line() {
            let content: &'input str = peek.content;
            let line_pos = peek.pos;
            let line_break_type = peek.break_type;
            let trimmed = content.trim_start_matches(' ');
            if let Some(after_star) = trimmed.strip_prefix('*') {
                let leading = content.len() - trimmed.len();
                let star_pos = Pos {
                    byte_offset: line_pos.byte_offset + leading,
                    line: line_pos.line,
                    column: line_pos.column + leading,
                };
                // YAML 1.2 §7.1: alias nodes cannot have properties (anchor or tag).
                if self.pending_tag.is_some() {
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: star_pos,
                        message: "alias node cannot have a tag property".into(),
                    }));
                }
                // An Inline anchor preceding `*alias` is an error — it would annotate
                // the alias node, which is illegal.  A Standalone anchor belongs to
                // the surrounding collection, not the alias, so it is not an error here.
                if matches!(self.pending_anchor, Some(PendingAnchor::Inline(_))) {
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: star_pos,
                        message: "alias node cannot have an anchor property".into(),
                    }));
                }
                match scan_anchor_name(after_star, star_pos) {
                    Err(e) => {
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(e));
                    }
                    Ok(name) => {
                        let name_char_count = name.chars().count();
                        // Build alias span: from `*` through end of name.
                        let alias_end = Pos {
                            byte_offset: star_pos.byte_offset + 1 + name.len(),
                            line: star_pos.line,
                            column: star_pos.column + 1 + name_char_count,
                        };
                        let alias_span = Span {
                            start: star_pos,
                            end: alias_end,
                        };
                        // Compute remaining content after the alias name, before
                        // consuming the line (which would invalidate the borrow).
                        let after_name = &after_star[name.len()..];
                        let remaining: &'input str = after_name.trim_start_matches([' ', '\t']);
                        let spaces = after_name.len() - remaining.len();
                        let had_remaining = !remaining.is_empty();
                        let rem_byte_offset = star_pos.byte_offset + 1 + name.len() + spaces;
                        let rem_col = star_pos.column + 1 + name_char_count + spaces;
                        self.lexer.consume_line();
                        if had_remaining {
                            let rem_pos = Pos {
                                byte_offset: rem_byte_offset,
                                line: star_pos.line,
                                column: rem_col,
                            };
                            let synthetic = crate::lines::Line {
                                content: remaining,
                                offset: rem_byte_offset,
                                indent: rem_col,
                                break_type: line_break_type,
                                pos: rem_pos,
                            };
                            self.lexer.prepend_inline_line(synthetic);
                        }
                        self.tick_mapping_phase_after_scalar();
                        return StepResult::Yield(Ok((Event::Alias { name }, alias_span)));
                    }
                }
            }
        }

        // ---- Tag: `!tag`, `!!tag`, `!<uri>`, or `!` — attach to next node ----

        if let Some(peek) = self.lexer.peek_next_line() {
            let content: &'input str = peek.content;
            let line_pos = peek.pos;
            let line_indent = peek.indent;
            let line_break_type = peek.break_type;
            let trimmed = content.trim_start_matches(' ');
            if trimmed.starts_with('!') {
                let leading = content.len() - trimmed.len();
                let bang_pos = Pos {
                    byte_offset: line_pos.byte_offset + leading,
                    line: line_pos.line,
                    column: line_pos.column + leading,
                };
                // `tag_start` starts at the `!`; `after_bang` is everything after it.
                let tag_start: &'input str = &content[leading..];
                let after_bang: &'input str = &content[leading + 1..];
                match scan_tag(after_bang, tag_start, bang_pos) {
                    Err(e) => {
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(e));
                    }
                    Ok((tag_slice, advance_past_bang)) => {
                        // Total bytes consumed for the tag token: 1 (`!`) + advance.
                        let tag_token_bytes = 1 + advance_past_bang;
                        let after_tag = &trimmed[tag_token_bytes..];
                        let inline: &'input str = after_tag.trim_start_matches([' ', '\t']);
                        let spaces = after_tag.len() - inline.len();
                        let had_inline = !inline.is_empty();
                        // YAML 1.2 §6.8.1: a tag property must be separated from
                        // the following node content by `s-separate` when the first
                        // character after the tag could be confused with a tag
                        // continuation or creates structural ambiguity:
                        // - `!` starts another tag property
                        // - flow indicators (`,`, `[`, `]`, `{`, `}`) cause
                        //   structural confusion (e.g. `!!str,`)
                        // - `%` may be a valid percent-encoded continuation that
                        //   should have been part of the tag, or an invalid
                        //   percent-sequence that makes the input unparseable
                        // When the tag scanner stopped at a plain non-tag char like
                        // `<`, the tag ended naturally and the content is the value
                        // (e.g. `!foo<bar val` → tag=`!foo`, scalar=`<bar val`).
                        if had_inline && spaces == 0 {
                            let first = inline.chars().next().unwrap_or('\0');
                            if first == '!'
                                || first == '%'
                                || matches!(first, ',' | '[' | ']' | '{' | '}')
                            {
                                self.state = IterState::Done;
                                return StepResult::Yield(Err(Error {
                                    pos: bang_pos,
                                    message:
                                        "tag must be separated from node content by whitespace"
                                            .into(),
                                }));
                            }
                        }
                        let inline_offset =
                            line_pos.byte_offset + leading + tag_token_bytes + spaces;
                        let inline_col = line_pos.column + leading + tag_token_bytes + spaces;
                        // Duplicate tags on the same node are an error.
                        // Exception: if the existing tag is collection-level
                        // (Standalone variant) and the new tag has inline content
                        // that is (or contains) a mapping key line, they apply to
                        // different nodes (collection vs. key scalar).
                        if self.pending_tag.is_some() {
                            let is_different_node =
                                matches!(self.pending_tag, Some(PendingTag::Standalone(_)))
                                    && had_inline
                                    && inline_contains_mapping_key(inline);
                            if !is_different_node {
                                self.state = IterState::Done;
                                return StepResult::Yield(Err(Error {
                                    pos: bang_pos,
                                    message: "a node may not have more than one tag".into(),
                                }));
                            }
                        }
                        // Resolve tag handle against directive scope at scan time.
                        let resolved_tag =
                            match self.directive_scope.resolve_tag(tag_slice, bang_pos) {
                                Ok(t) => t,
                                Err(e) => {
                                    self.state = IterState::Done;
                                    return StepResult::Yield(Err(e));
                                }
                            };
                        self.lexer.consume_line();
                        if had_inline {
                            self.pending_tag = Some(PendingTag::Inline(resolved_tag));
                            // Record the original physical line's indent so that
                            // handle_mapping_entry can open the mapping at the correct
                            // indent when the key is on a synthetic (offset) line.
                            // Only set when the inline content is (or leads to) a
                            // mapping key — if it's a plain value, there is no
                            // handle_mapping_entry call to consume this, and leaving
                            // it set would corrupt the next unrelated mapping entry.
                            if self.property_origin_indent.is_none()
                                && inline_contains_mapping_key(inline)
                            {
                                self.property_origin_indent = Some(line_indent);
                            }
                            let inline_pos = Pos {
                                byte_offset: inline_offset,
                                line: line_pos.line,
                                column: inline_col,
                            };
                            let synthetic = crate::lines::Line {
                                content: inline,
                                offset: inline_offset,
                                indent: inline_col,
                                break_type: line_break_type,
                                pos: inline_pos,
                            };
                            self.lexer.prepend_inline_line(synthetic);
                        } else {
                            // Standalone tag line — applies to whatever node comes next.
                            // Validate: the tag must be indented enough for this context.
                            let min = self.min_standalone_property_indent();
                            if line_indent < min {
                                self.state = IterState::Done;
                                return StepResult::Yield(Err(Error {
                                    pos: bang_pos,
                                    message:
                                        "node property is not indented enough for this context"
                                            .into(),
                                }));
                            }
                            self.pending_tag = Some(PendingTag::Standalone(resolved_tag));
                        }
                        return StepResult::Continue;
                    }
                }
            }
        }

        // ---- Anchor: `&name` — attach to the next node ----

        if let Some(peek) = self.lexer.peek_next_line() {
            let content: &'input str = peek.content;
            let line_pos = peek.pos;
            let line_indent = peek.indent;
            let line_break_type = peek.break_type;
            let trimmed = content.trim_start_matches(' ');
            if let Some(after_amp) = trimmed.strip_prefix('&') {
                // We only look for `&` at the start of the trimmed line.
                // Tags (`!`) before `&` are handled in Task 17.
                //
                // IMPORTANT for Task 17: when implementing tag-skip, the skip
                // logic must consume the *full* tag token (all `ns-anchor-char`
                // bytes after `!`), not just the `!` character alone.  The `!`
                // character is itself a valid `ns-anchor-char`, so skipping
                // only `!` and then re-entering anchor detection would silently
                // include the tag body in the anchor name.  Example: `!tag &a`
                // — skip must advance past `tag` before looking for `&a`.
                let leading = content.len() - trimmed.len();
                let amp_pos = Pos {
                    byte_offset: line_pos.byte_offset + leading,
                    line: line_pos.line,
                    column: line_pos.column + leading,
                };
                match scan_anchor_name(after_amp, amp_pos) {
                    Err(e) => {
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(e));
                    }
                    Ok(name) => {
                        // Determine what follows the anchor name on this line,
                        // before consuming the line (borrow ends here).
                        let after_name = &after_amp[name.len()..];
                        let inline: &'input str = after_name.trim_start_matches([' ', '\t']);
                        let spaces = after_name.len() - inline.len();
                        let had_inline = !inline.is_empty();
                        let name_char_count = name.chars().count();
                        let inline_offset =
                            line_pos.byte_offset + leading + 1 + name.len() + spaces;
                        let inline_col = line_pos.column + leading + 1 + name_char_count + spaces;
                        // Duplicate anchors on the same node are an error.
                        //
                        // Case 1: existing anchor is inline (Inline variant) and no
                        // collection tag is pending — both this and the existing anchor
                        // are for the same item-level node.
                        //
                        // Case 2: existing anchor is standalone (Standalone variant)
                        // and the new anchor has inline content that is NOT a collection
                        // opener ([, {) or property (!, &) — both anchors apply to the
                        // same scalar node.
                        let amp_pos2 = amp_pos;
                        let has_standalone_tag =
                            matches!(self.pending_tag, Some(PendingTag::Standalone(_)));
                        let is_duplicate =
                            if matches!(self.pending_anchor, Some(PendingAnchor::Inline(_)))
                                && !has_standalone_tag
                            {
                                true
                            } else if matches!(
                                self.pending_anchor,
                                Some(PendingAnchor::Standalone(_))
                            ) && had_inline
                                && !has_standalone_tag
                            {
                                // The existing anchor is collection-level, but the new anchor
                                // has inline content.  If that content is a mapping key line
                                // (contains `: ` etc.), the new anchor is for the key and the
                                // existing anchor is for the mapping — different nodes, no error.
                                // If the inline is a plain scalar (no key indicator), both
                                // anchors apply to the same scalar node — error.
                                let first_ch = inline.chars().next();
                                // If inline starts with a collection/property opener, treat as
                                // different node — no error.
                                let starts_with_opener = matches!(
                                    first_ch,
                                    Some('[' | '{' | '!' | '&' | '*' | '|' | '>')
                                );
                                // If inline contains a mapping key indicator (`: `), the new
                                // anchor is for a key — different node from the collection.
                                let is_mapping_key = find_value_indicator_offset(inline).is_some();
                                !starts_with_opener && !is_mapping_key
                            } else {
                                false
                            };
                        if is_duplicate {
                            self.state = IterState::Done;
                            return StepResult::Yield(Err(Error {
                                pos: amp_pos2,
                                message: "a node may not have more than one anchor".into(),
                            }));
                        }
                        self.lexer.consume_line();
                        if had_inline {
                            // Detect illegal inline block sequence: `&anchor - item`
                            // is invalid — a block sequence indicator cannot appear
                            // inline after an anchor property in block context.
                            let is_seq = inline.strip_prefix('-').is_some_and(|rest| {
                                rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')
                            });
                            if is_seq {
                                self.state = IterState::Done;
                                let seq_pos = Pos {
                                    byte_offset: inline_offset,
                                    line: line_pos.line,
                                    column: inline_col,
                                };
                                return StepResult::Yield(Err(Error {
                                    pos: seq_pos,
                                    message:
                                        "block sequence indicator cannot appear inline after a node property"
                                            .into(),
                                }));
                            }
                            // Inline content after anchor — anchor applies to the
                            // inline node (scalar or key), not to any enclosing
                            // collection opened on this same line.
                            self.pending_anchor = Some(PendingAnchor::Inline(name));
                            // Record the original physical line's indent so that
                            // handle_mapping_entry can open the mapping at the correct
                            // indent when the key is on a synthetic (offset) line.
                            // Only set when the inline content leads to a mapping key;
                            // value-context anchors must not corrupt the next entry.
                            if self.property_origin_indent.is_none()
                                && inline_contains_mapping_key(inline)
                            {
                                self.property_origin_indent = Some(line_indent);
                            }
                            let inline_pos = Pos {
                                byte_offset: inline_offset,
                                line: line_pos.line,
                                column: inline_col,
                            };
                            let synthetic = crate::lines::Line {
                                content: inline,
                                offset: inline_offset,
                                indent: inline_col,
                                break_type: line_break_type,
                                pos: inline_pos,
                            };
                            self.lexer.prepend_inline_line(synthetic);
                        } else {
                            // Standalone anchor line — anchor applies to whatever
                            // node comes next (collection or scalar).
                            // Validate: the anchor must be indented enough for this context.
                            let min = self.min_standalone_property_indent();
                            if line_indent < min {
                                self.state = IterState::Done;
                                let err_pos = amp_pos;
                                return StepResult::Yield(Err(Error {
                                    pos: err_pos,
                                    message:
                                        "node property is not indented enough for this context"
                                            .into(),
                                }));
                            }
                            self.pending_anchor = Some(PendingAnchor::Standalone(name));
                        }
                        // Let the next iteration handle whatever follows.
                        return StepResult::Continue;
                    }
                }
            }
        }

        // ---- Flow collection detection: `[` or `{` starts a flow collection ----
        // Stray closing flow indicators (`]`, `}`) in block context are errors.

        if let Some(line) = self.lexer.peek_next_line() {
            let trimmed = line.content.trim_start_matches(' ');
            if trimmed.starts_with('[') || trimmed.starts_with('{') {
                return self.handle_flow_collection();
            }
            if trimmed.starts_with(']') || trimmed.starts_with('}') {
                let err_pos = line.pos;
                let ch = trimmed.chars().next().unwrap_or(']');
                self.state = IterState::Done;
                self.lexer.consume_line();
                return StepResult::Yield(Err(Error {
                    pos: err_pos,
                    message: format!("unexpected '{ch}' outside flow collection"),
                }));
            }
        }

        // ---- Block sequence / mapping entry detection ----

        if let Some((dash_indent, dash_pos)) = self.peek_sequence_entry() {
            return self.handle_sequence_entry(dash_indent, dash_pos);
        }
        if let Some((key_indent, key_pos)) = self.peek_mapping_entry() {
            return self.handle_mapping_entry(key_indent, key_pos);
        }

        // ---- Dedent: close collections more deeply nested than the current line ----

        if let Some(line) = self.lexer.peek_next_line() {
            let line_indent = line.indent;
            let close_pos = self.lexer.current_pos();
            // Record the minimum indent across all open collections before
            // closing. A root collection has indent 0. If the minimum indent
            // before closure was 0 and the stack empties, the root node is
            // complete. When a tag-inline mapping opens at a column > 0 (a
            // pre-existing indent-tracking limitation), closing it must not
            // prematurely mark the root as emitted.
            let min_indent_before = self.coll_stack.iter().map(|e| e.indent()).min();
            self.close_collections_at_or_above(line_indent.saturating_add(1), close_pos);
            // If closing collections emptied the stack, the root node is
            // complete — but only if the outermost collection was at indent 0
            // (a true root collection, not a spuriously-indented inline tag).
            if self.coll_stack.is_empty() && !self.queue.is_empty() && min_indent_before == Some(0)
            {
                self.root_node_emitted = true;
            }
            if !self.queue.is_empty() {
                return StepResult::Continue;
            }
        }

        // ---- Block structure validity checks ----
        //
        // After closing deeper collections and before consuming a scalar,
        // validate that the current line's indentation is consistent with
        // the innermost open block collection.
        //
        // For block sequences: the only valid content at the sequence's own
        // indent level is `- ` (handled by peek_sequence_entry above).
        // Any other content at that indent level is invalid YAML.
        //
        // For block mappings in Key phase: the only valid content at the
        // mapping's indent level is a mapping entry (handled by
        // peek_mapping_entry above). A plain scalar without `: ` is not
        // a valid implicit mapping key.
        if let Some(line) = self.lexer.peek_next_line() {
            let line_indent = line.indent;
            match self.coll_stack.last() {
                Some(&CollectionEntry::Sequence(seq_indent, _)) if line_indent == seq_indent => {
                    // Content at the sequence indent level that is NOT `- ` is
                    // invalid. peek_sequence_entry already returned None, so this
                    // line is not a sequence entry.
                    let err_pos = line.pos;
                    self.state = IterState::Done;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "invalid content at block sequence indent level: expected '- '"
                            .into(),
                    }));
                }
                Some(&CollectionEntry::Mapping(map_indent, MappingPhase::Key, _))
                    if line_indent == map_indent =>
                {
                    let err_pos = line.pos;
                    self.state = IterState::Done;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message:
                            "invalid content at block mapping indent level: expected mapping key"
                                .into(),
                    }));
                }
                // Content more deeply indented than the mapping key level is only
                // valid as an explicit-key continuation (explicit_key_pending=true)
                // or as the very first key (has_had_value=false — the first key may
                // be at any indent >= map_indent).  After at least one key-value pair
                // has been processed (has_had_value=true) with no explicit-key pending,
                // deeper content that is not a valid mapping key is an error.
                Some(&CollectionEntry::Mapping(map_indent, MappingPhase::Key, true))
                    if line_indent > map_indent
                        && !self.explicit_key_pending
                        && !self.lexer.is_next_line_synthetic() =>
                {
                    let err_pos = line.pos;
                    self.state = IterState::Done;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "unexpected indented content after mapping value".into(),
                    }));
                }
                _ => {}
            }
        }

        // ---- Scalars ----

        // `block_parent_indent` — the indent of the enclosing block context;
        // block scalars (`|`, `>`) must have content lines more indented than
        // this value.  For a block scalar embedded as inline content after `? `
        // or `- `, the enclosing block's indent is the *collection's* indent,
        // not the column of the inline `|`/`>` token.
        //
        // `plain_parent_indent` — the enclosing block's indent level.
        // Plain scalar continuation lines must be indented strictly more than
        // `plain_parent_indent` (YAML 1.2), with a special exception for
        // tab-indented lines when `plain_parent_indent == 0` (the tab provides
        // the s-separate-in-line separator required by s-flow-folded(0)).
        // Use usize::MAX as a sentinel for "root level" — the root node has no
        // parent collection, so block scalar body lines may start at column 0
        // (equivalent to a parent indent of -1 in the YAML spec).
        let block_parent_indent = self.coll_stack.last().map_or(usize::MAX, |e| e.indent());
        let plain_parent_indent = self.coll_stack.last().map_or(0, |e| e.indent());
        // Capture whether an inline scalar (from `--- text`) was pending before
        // the scalar dispatch call.  If it was, the emitted plain scalar came
        // from the `---` marker line and is NOT necessarily the complete root
        // node — the lexer emits `--- >` / `--- |` / `--- "text` inline content
        // as a plain scalar, but the actual node body follows on subsequent
        // lines.  Marking root_node_emitted in those cases would incorrectly
        // reject the body lines as "content after root node".
        let had_inline_scalar = self.lexer.has_inline_scalar();
        match self.try_consume_scalar(plain_parent_indent, block_parent_indent) {
            Ok(Some(event)) => {
                self.tick_mapping_phase_after_scalar();
                // Drain any trailing comment detected on the scalar's line.
                self.drain_trailing_comment();
                // A scalar emitted at the document root (no open collection)
                // is the complete root node — unless it came from inline
                // content after `---` (had_inline_scalar), in which case the
                // body on subsequent lines is part of the same node.
                if self.coll_stack.is_empty() && !had_inline_scalar {
                    self.root_node_emitted = true;
                }
                return StepResult::Yield(Ok(event));
            }
            Err(e) => {
                self.state = IterState::Done;
                return StepResult::Yield(Err(e));
            }
            Ok(None) => {}
        }

        // Check for invalid characters at the start of an unrecognised line.
        // A line that starts with a character that is neither whitespace nor a
        // valid YAML ns-char (e.g. NUL U+0000 or mid-stream BOM U+FEFF) is a
        // parse error.
        if let Some(line) = self.lexer.peek_next_line() {
            let first_ch = line.content.chars().next();
            if let Some(ch) = first_ch {
                if ch != ' ' && ch != '\t' && !crate::lexer::is_ns_char(ch) {
                    let err_pos = line.pos;
                    self.state = IterState::Done;
                    self.lexer.consume_line();
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: format!("invalid character U+{:04X} in document", ch as u32),
                    }));
                }
            }
        }

        // Fallback: unrecognised content line — consume and loop.
        self.lexer.consume_line();
        StepResult::Continue
    }
}
