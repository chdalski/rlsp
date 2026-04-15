// SPDX-License-Identifier: MIT

//! Flow-collection state machine.
//!
//! Contains `handle_flow_collection`, the single largest method in the
//! codebase. The function declares a local `FlowFrame` enum and contains
//! an explicit "repetition is intentional" design note — both must be
//! preserved verbatim through any future edits. See the design note in
//! the function body for the rationale.

use super::properties::{scan_anchor_name, scan_tag};
use super::state::{
    CollectionEntry, FlowMappingPhase, IterState, MappingPhase, PendingAnchor, PendingTag,
    StepResult,
};
use crate::error::Error;
use crate::event::{CollectionStyle, Event, ScalarStyle};
use crate::limits::MAX_COLLECTION_DEPTH;
use crate::pos::{Pos, Span};
use crate::{EventIter, empty_scalar_event, zero_span};

impl<'input> EventIter<'input> {
    /// Handle a flow collection (`[...]` or `{...}`) starting on the current line.
    ///
    /// This method reads the complete flow collection — potentially spanning
    /// multiple physical lines — and pushes all events (SequenceStart/End,
    /// MappingStart/End, Scalar) to `self.queue`.  It returns when the
    /// outermost closing delimiter (`]` or `}`) is consumed.
    ///
    /// ## Security invariants
    ///
    /// - **No recursion:** the parser uses an explicit `Vec<FlowFrame>` stack
    ///   rather than recursive function calls, preventing stack overflow on
    ///   deeply nested input.
    /// - **Unified depth limit:** each new nested collection checks against
    ///   `MAX_COLLECTION_DEPTH` using the same `coll_stack.len()` counter as
    ///   block collections, so flow and block nesting depths are additive.
    /// - **Incremental parsing:** content is processed line-by-line; no
    ///   `String` buffer holds the entire flow body.
    /// - **Unterminated collection:** reaching EOF without the matching closing
    ///   delimiter returns `Err`.
    #[expect(
        clippy::too_many_lines,
        reason = "match-on-event-type; splitting would obscure flow"
    )]
    pub(crate) fn handle_flow_collection(&mut self) -> StepResult<'input> {
        use crate::lexer::scan_plain_line_flow;
        use std::borrow::Cow;

        // -----------------------------------------------------------------------
        // Local types for the explicit flow-parser stack.
        // -----------------------------------------------------------------------

        /// One frame on the explicit flow-parser stack.
        #[derive(Clone, Copy)]
        enum FlowFrame {
            /// An open `[...]` sequence.
            ///
            /// `has_value` is `false` immediately after opening and immediately
            /// after each comma; it becomes `true` when a scalar or nested
            /// collection is emitted.  A comma arriving when `has_value` is
            /// `false` is a leading comma error.
            ///
            /// `after_colon` is `true` when we have just consumed a `:` value
            /// separator in a single-pair implicit mapping context.  In this
            /// state a new scalar or collection is the value of the single-pair
            /// mapping — not a new entry — so the missing-comma check must not
            /// fire.
            ///
            /// `last_was_plain` is `true` when the most recent emitted item was
            /// a plain scalar.  Plain scalars may span multiple lines in flow
            /// context, so the missing-comma check must not fire after a plain
            /// scalar (the next line's content may be a continuation).
            ///
            /// `in_implicit_map` is `true` when a `:` value separator has been
            /// consumed inside this sequence entry — meaning a `MappingStart`
            /// was inserted before the key scalar and a `MappingEnd` must be
            /// emitted before the next `,` or `]`.
            ///
            /// `key_start_idx` records the index in `events` where the current
            /// key scalar/collection starts. When `:` arrives, `MappingStart`
            /// is inserted at this position.
            Sequence {
                has_value: bool,
                after_colon: bool,
                last_was_plain: bool,
                in_implicit_map: bool,
                key_start_idx: usize,
            },
            /// An open `{...}` mapping.
            ///
            /// `has_value` tracks the same invariant as in `Sequence` but for
            /// the mapping as a whole (not per key/value pair).
            ///
            /// `after_colon` is `true` after the `:` value separator has been
            /// consumed in Value phase.  It prevents a plain-scalar value that
            /// starts with `:` (e.g. `{x: :x}`) from being mis-parsed as a
            /// second value separator.  Reset to `false` when transitioning back
            /// to Key phase (comma or value-scalar emission).
            ///
            /// `last_was_plain` mirrors the same concept as in `Sequence`: when
            /// the most recent emitted item was a plain scalar, the next line
            /// may be a multi-line continuation, so indicator-start validation
            /// must be deferred until we know whether it is a continuation.
            ///
            /// `key_continuation` is `true` when a multi-line plain key was
            /// extended across a line break and the phase was reverted to Key
            /// (from Value) to recognise the `:` separator.  When a `,` arrives
            /// in this state the key is complete but has no value — a null value
            /// must be emitted before the comma resets the frame.
            ///
            /// `explicit_key_pending` is `true` after a `?` explicit-key
            /// indicator is consumed but before any key content arrives.  When
            /// `}` arrives in Key phase with this flag set, a null key and null
            /// value must be emitted (e.g. `{? explicit: entry, ?}`).
            Mapping {
                phase: FlowMappingPhase,
                has_value: bool,
                after_colon: bool,
                last_was_plain: bool,
                key_continuation: bool,
                explicit_key_pending: bool,
            },
        }

        // Design note — phase-advance pattern
        //
        // Four sites below repeat the same `if let Some(frame) = flow_stack.last_mut()
        // { match frame { Sequence { has_value } => ... Mapping { phase, has_value } =>
        // ... } }` shape.  Extracting a helper function would require moving `FlowFrame`
        // and `FlowMappingPhase` to module scope — adding module-level types whose sole
        // purpose is to enable this refactor adds more complexity than the duplication
        // costs.  Each site is 6–8 lines and clearly labelled by its comment; the
        // repetition is intentional and stable.

        // -----------------------------------------------------------------------
        // Buffer-management invariant
        // -----------------------------------------------------------------------
        //
        // The line buffer always holds the current line un-consumed.  We peek to
        // read content and only consume the line when we need to advance past it
        // (end-of-line or quoted-scalar delegation).
        //
        // `cur_content` / `cur_base_pos` always mirror what `peek_next_line()`
        // returns.  After any call that changes the buffer (consume_line /
        // prepend_inline_line), we immediately re-sync via peek.
        //
        // Helper: advance `pos` over `content[..byte_len]`, one char at a time.

        let abs_pos = |base: Pos, content: &str, i: usize| -> Pos {
            crate::pos::advance_within_line(base, &content[..i])
        };

        // -----------------------------------------------------------------------
        // Initialise: read the current line, locate the opening delimiter.
        // -----------------------------------------------------------------------

        // SAFETY: caller verified via peek in step_in_document.
        let Some(first_line) = self.lexer.peek_next_line() else {
            unreachable!("handle_flow_collection called without a pending line")
        };

        // Strip both leading spaces and tabs so that tab-indented flow collections
        // (e.g. `\t[...]`) position pos_in_line directly at the `[` or `{`,
        // bypassing the tab-indent error check in the main loop (YAML test
        // suite 6CA3, Q5MG).
        let leading = first_line.content.len()
            - first_line
                .content
                .trim_start_matches(|c| c == ' ' || c == '\t')
                .len();
        // The physical line number where the outermost flow collection opened.
        // Used to detect multi-line flow keys (C2SP).
        let start_line = first_line.pos.line;
        // The physical line number of the most recent emitted value (scalar or
        // inner-collection close).  Used to detect multi-line implicit keys (DK4H):
        // a `:` value separator on a different line than the preceding key is invalid.
        let mut last_token_line = first_line.pos.line;
        // Saved from `first_line` for use after the loop (where `first_line` is
        // no longer accessible due to mutable borrow of `self.lexer` in the loop).
        let first_line_indent = first_line.indent;
        let first_line_pos = first_line.pos;
        // Set when a `?` explicit-key indicator is consumed inside a flow sequence.
        // Suppresses the DK4H single-line check for the corresponding `:` separator —
        // explicit keys in flow sequences may span multiple lines (YAML 1.2 §7.4.2).
        let mut explicit_key_in_seq = false;

        // Stack for tracking open flow collections (nested via explicit iteration,
        // not recursion — security requirement).
        let mut flow_stack: Vec<FlowFrame> = Vec::new();
        // All events assembled during this call (pushed to self.queue at end).
        let mut events: Vec<(Event<'input>, Span)> = Vec::new();
        // Set to the indent of the enclosing block mapping to open when this
        // flow collection is used as an implicit complex mapping key
        // (`[flow]: value` or `{ flow }: value`).  `None` means not a key.
        let mut implicit_key_mapping_indent: Option<usize> = None;
        // Current byte offset within `cur_content`.
        let mut pos_in_line: usize = leading;
        // Pending anchor for the next node in this flow collection.
        // Seeded from any block-context anchor that was pending when this flow
        // collection was entered (e.g. `&seq [a, b]` sets pending_anchor before
        // the `[` is dispatched to handle_flow_collection).
        let mut pending_flow_anchor: Option<&'input str> =
            self.pending_anchor.take().map(PendingAnchor::name);
        // Pending tag for the next node in this flow collection.
        // Seeded from any block-context tag that was pending when this flow
        // collection was entered (e.g. `!!seq [a, b]` sets pending_tag before
        // the `[` is dispatched to handle_flow_collection).
        let mut pending_flow_tag: Option<std::borrow::Cow<'input, str>> =
            self.pending_tag.take().map(PendingTag::into_cow);

        // Re-sync `cur_content` / `cur_base_pos` from the buffer.
        // Returns false when the buffer is empty (EOF mid-flow).
        // INVARIANT: called every time after consuming or prepending a line.
        macro_rules! resync {
            () => {{
                match self.lexer.peek_next_line() {
                    Some(l) => {
                        // Safe: we re-assign these immediately without holding
                        // a borrow on `self.lexer` at the same time.
                        (l.content, l.pos)
                    }
                    None => {
                        // EOF
                        ("", self.lexer.current_pos())
                    }
                }
            }};
        }

        let (mut cur_content, mut cur_base_pos) = resync!();

        // The minimum indent for continuation lines in this flow collection.
        // When the flow collection is inside an enclosing block collection,
        // continuation lines must be indented more than the enclosing block's
        // indent level (YAML 1.2: flow context lines must not regress to or
        // below the enclosing block indent level).
        // At document root (coll_stack empty), there is no enclosing block, so
        // no constraint — represented as None.
        let flow_min_indent: Option<usize> = self.coll_stack.last().map(|e| e.indent());

        // -----------------------------------------------------------------------
        // Main parse loop — iterates over characters in the current (and
        // subsequent) lines until the outermost closing delimiter is found.
        // -----------------------------------------------------------------------

        'outer: loop {
            // Document markers (`---` and `...`) are only valid at the document
            // level — they are illegal inside flow collections (YAML 1.2 §8.1).
            // A document marker must appear at the very beginning of a line
            // (column 0) and be followed by whitespace or end-of-line.
            if pos_in_line == 0
                && (cur_content.starts_with("---") || cur_content.starts_with("..."))
            {
                let rest = &cur_content[3..];
                if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
                    let err_pos = abs_pos(cur_base_pos, cur_content, 0);
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "document marker is not allowed inside a flow collection".into(),
                    }));
                }
            }

            // Tabs as indentation on a new line in flow context are invalid
            // (YAML 1.2 §6.2 — indentation uses spaces only).  A tab at the
            // start of a continuation line (before the first non-whitespace
            // character) is a tab used as indentation.  Blank lines (tab only,
            // no content) are exempt — they are treated as empty separator lines.
            //
            // Exception: a line whose first non-tab character is a flow
            // collection delimiter (`[`, `{`, `]`, `}`) is allowed — the tab
            // is serving as visual indentation before the delimiter, not as a
            // structural indent (YAML test suite 6CA3, Q5MG).
            if pos_in_line == 0 {
                let first_non_tab = cur_content.trim_start_matches('\t').chars().next();
                let has_tab_indent = cur_content.starts_with('\t')
                    && !cur_content.trim().is_empty()
                    && !matches!(first_non_tab, Some('[' | '{' | ']' | '}'));
                if has_tab_indent {
                    let err_pos = abs_pos(cur_base_pos, cur_content, 0);
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "tab character is not allowed as indentation in flow context"
                            .into(),
                    }));
                }
            }

            // Skip leading spaces/tabs and comments.
            // `#` is a comment start only when preceded by whitespace (or at
            // start of line, i.e. pos_in_line == 0 with all prior chars being
            // whitespace).  A `#` immediately after a token (e.g. `,#`) is not
            // a comment — it is an error character that will be caught below.
            let prev_was_ws_at_loop_entry = pos_in_line == 0
                || cur_content[..pos_in_line]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c == ' ' || c == '\t');
            let mut prev_was_ws = prev_was_ws_at_loop_entry;
            while pos_in_line < cur_content.len() {
                let Some(ch) = cur_content[pos_in_line..].chars().next() else {
                    break;
                };
                if ch == ' ' || ch == '\t' {
                    prev_was_ws = true;
                    pos_in_line += 1;
                } else if ch == '#' && prev_was_ws {
                    // Emit a Comment event for this `# comment` to end of line.
                    // No MAX_COMMENT_LEN check here — this comment is bounded by the
                    // physical line length (itself bounded by total input size), the
                    // same reason drain_trailing_comment does not apply the limit.
                    let hash_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    // Comment text: everything after `#` (byte at pos_in_line is `#`,
                    // ASCII 1 byte, so text starts at pos_in_line + 1).
                    let text_start = pos_in_line + 1;
                    // SAFETY: text_start <= cur_content.len() because we found
                    // `#` at pos_in_line which is < cur_content.len().
                    let comment_text: &'input str = cur_content.get(text_start..).unwrap_or("");
                    let comment_end =
                        crate::pos::advance_within_line(hash_pos.advance('#'), comment_text);
                    let comment_span = Span {
                        start: hash_pos,
                        end: comment_end,
                    };
                    events.push((Event::Comment { text: comment_text }, comment_span));
                    pos_in_line = cur_content.len();
                } else {
                    break;
                }
            }

            // ----------------------------------------------------------------
            // End of line — consume and advance.
            // ----------------------------------------------------------------
            if pos_in_line >= cur_content.len() {
                self.lexer.consume_line();

                if flow_stack.is_empty() {
                    // Outermost collection closed; done.
                    break 'outer;
                }

                (cur_content, cur_base_pos) = resync!();
                if cur_content.is_empty() && self.lexer.at_eof() {
                    let err_pos = self.lexer.current_pos();
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "unterminated flow collection: unexpected end of input".into(),
                    }));
                }

                // Flow continuation lines must be indented more than the
                // enclosing block context (YAML 1.2: flow lines must not
                // regress to the block indent level).  Blank/whitespace-only
                // lines are exempt — they act as line separators.
                // At document root (no enclosing block), there is no
                // indentation constraint.
                if let Some(min_indent) = flow_min_indent {
                    if let Some(next_line) = self.lexer.peek_next_line() {
                        let trimmed = next_line.content.trim();
                        if !trimmed.is_empty() && next_line.indent <= min_indent {
                            let err_pos = next_line.pos;
                            self.state = IterState::Done;
                            return StepResult::Yield(Err(Error {
                                pos: err_pos,
                                message: "flow collection continuation line is not indented enough"
                                    .into(),
                            }));
                        }
                    }
                }

                pos_in_line = 0;
                continue 'outer;
            }

            let Some(ch) = cur_content[pos_in_line..].chars().next() else {
                continue 'outer;
            };

            // ----------------------------------------------------------------
            // Opening delimiters `[` and `{`
            // ----------------------------------------------------------------
            if ch == '[' || ch == '{' {
                // Check unified depth limit (flow + block combined).
                let total_depth = self.coll_stack.len() + flow_stack.len();
                if total_depth >= MAX_COLLECTION_DEPTH {
                    let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "collection nesting depth exceeds limit".into(),
                    }));
                }

                let open_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                let open_span = zero_span(open_pos);
                pos_in_line += 1;

                // Before pushing this new nested collection, update the parent
                // sequence frame's key_start_idx to point here (the nested
                // collection's start event) — so that if `:` arrives after this
                // collection closes, MappingStart is inserted at the right place.
                let new_start_idx = events.len();
                if let Some(FlowFrame::Sequence {
                    has_value: false,
                    in_implicit_map: false,
                    key_start_idx,
                    ..
                }) = flow_stack.last_mut()
                {
                    *key_start_idx = new_start_idx;
                }

                if ch == '[' {
                    // Push SequenceStart first, then record key_start_idx as the
                    // index AFTER SequenceStart (where the first item will appear).
                    events.push((
                        Event::SequenceStart {
                            anchor: pending_flow_anchor.take(),
                            tag: pending_flow_tag.take(),
                            style: CollectionStyle::Flow,
                        },
                        open_span,
                    ));
                    flow_stack.push(FlowFrame::Sequence {
                        has_value: false,
                        after_colon: false,
                        last_was_plain: false,
                        in_implicit_map: false,
                        key_start_idx: events.len(),
                    });
                } else {
                    flow_stack.push(FlowFrame::Mapping {
                        phase: FlowMappingPhase::Key,
                        has_value: false,
                        after_colon: false,
                        last_was_plain: false,
                        key_continuation: false,
                        explicit_key_pending: false,
                    });
                    events.push((
                        Event::MappingStart {
                            anchor: pending_flow_anchor.take(),
                            tag: pending_flow_tag.take(),
                            style: CollectionStyle::Flow,
                        },
                        open_span,
                    ));
                }
                continue 'outer;
            }

            // ----------------------------------------------------------------
            // Closing delimiters `]` and `}`
            // ----------------------------------------------------------------
            if ch == ']' || ch == '}' {
                let close_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                let close_span = zero_span(close_pos);
                pos_in_line += 1;

                let Some(top) = flow_stack.pop() else {
                    // Closing delimiter with empty stack — mismatched.
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: close_pos,
                        message: format!("unexpected '{ch}' in flow context"),
                    }));
                };

                match (ch, top) {
                    (
                        ']',
                        FlowFrame::Sequence {
                            in_implicit_map, ..
                        },
                    ) => {
                        if in_implicit_map {
                            events.push((Event::MappingEnd, close_span));
                        }
                        events.push((Event::SequenceEnd, close_span));
                    }
                    (
                        '}',
                        FlowFrame::Mapping {
                            phase,
                            explicit_key_pending,
                            ..
                        },
                    ) => {
                        // If a `?` was consumed but no key content followed,
                        // emit a null key and null value before closing.
                        if phase == FlowMappingPhase::Key && explicit_key_pending {
                            events.push((empty_scalar_event(), close_span));
                            events.push((empty_scalar_event(), close_span));
                        } else if phase == FlowMappingPhase::Value {
                            // If mapping is in Value phase (key emitted, no value yet),
                            // emit empty value before closing.
                            events.push((empty_scalar_event(), close_span));
                        }
                        events.push((Event::MappingEnd, close_span));
                    }
                    (']', FlowFrame::Mapping { .. }) => {
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(Error {
                            pos: close_pos,
                            message: "expected '}' to close flow mapping, found ']'".into(),
                        }));
                    }
                    ('}', FlowFrame::Sequence { .. }) => {
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(Error {
                            pos: close_pos,
                            message: "expected ']' to close flow sequence, found '}'".into(),
                        }));
                    }
                    _ => unreachable!("all (ch, top) combinations covered above"),
                }

                // After a nested collection closes inside a parent frame,
                // mark the parent as having a value (the nested collection was it),
                // and if it's a mapping in Value phase, advance to Key phase.
                if let Some(parent) = flow_stack.last_mut() {
                    // Update the last-token-line tracker so the multi-line implicit
                    // key check (DK4H) knows where the key (inner collection) ended.
                    last_token_line = cur_base_pos.line;
                    match parent {
                        FlowFrame::Sequence {
                            has_value,
                            after_colon,
                            last_was_plain,
                            ..
                        } => {
                            *has_value = true;
                            *after_colon = false;
                            *last_was_plain = false;
                        }
                        FlowFrame::Mapping {
                            phase,
                            has_value,
                            after_colon,
                            last_was_plain,
                            key_continuation,
                            explicit_key_pending,
                        } => {
                            *has_value = true;
                            *after_colon = false;
                            *last_was_plain = false;
                            *key_continuation = false;
                            *explicit_key_pending = false;
                            if *phase == FlowMappingPhase::Value {
                                *phase = FlowMappingPhase::Key;
                            }
                        }
                    }
                }

                if flow_stack.is_empty() {
                    // Outermost collection closed.
                    // Consume the current line; prepend any non-empty tail so the
                    // block state machine can process content after the `]`/`}`.
                    let tail_content = &cur_content[pos_in_line..];
                    let tail_trimmed = tail_content.trim_start_matches([' ', '\t']);
                    // `#` is a comment only when preceded by whitespace.  If the
                    // closing bracket is immediately followed by `#` (no space),
                    // that is not a valid comment — it is a syntax error.
                    if tail_trimmed.starts_with('#') {
                        // Check that at least one space or tab separates the
                        // closing bracket from the `#`.  `tail_content` is the
                        // raw suffix after the bracket; `tail_trimmed` has leading
                        // whitespace stripped.  If they differ in length, whitespace
                        // was present between the bracket and `#`.
                        let has_space_before_hash = tail_content.len() > tail_trimmed.len();
                        if !has_space_before_hash {
                            let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                            self.state = IterState::Done;
                            return StepResult::Yield(Err(Error {
                                pos: err_pos,
                                message: "comment requires at least one space before '#'".into(),
                            }));
                        }
                    }
                    // A flow collection used as an implicit mapping key must
                    // fit on a single line (YAML 1.2 §7.4.2).  If the tail
                    // begins with `:` (making this collection a mapping key) and
                    // the closing delimiter is on a different line than the
                    // opening delimiter, reject as a multi-line flow key.
                    if tail_trimmed.starts_with(':') && cur_base_pos.line != start_line {
                        let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(Error {
                            pos: err_pos,
                            message: "multi-line flow collection cannot be used as an implicit mapping key".into(),
                        }));
                    }
                    // If the tail starts with `:` and there is not already an
                    // explicit-key mapping opened at this indent, this flow
                    // collection is an implicit complex mapping key.  Record the
                    // indent so we can prepend a MappingStart after the loop.
                    //
                    // Skip when `complex_key_inline` matches the flow collection's
                    // indent — that means a `?`-opened mapping is already in place.
                    if tail_trimmed.starts_with(':') {
                        let flow_col = self.property_origin_indent.unwrap_or(first_line_indent);
                        let already_in_explicit_key = self.complex_key_inline == Some(flow_col)
                            && self.coll_stack.last().is_some_and(|e| {
                                matches!(e,
                                    CollectionEntry::Mapping(col, MappingPhase::Key, _)
                                    if *col == flow_col
                                )
                            });
                        if !already_in_explicit_key {
                            implicit_key_mapping_indent = Some(flow_col);
                        }
                    }
                    // If the block collection stack is empty AND the tail does not
                    // start with `:` (which would indicate this flow collection is a
                    // mapping key), the flow collection is the document root node.
                    // Mark it so subsequent content on the NEXT LINE triggers the
                    // root-node guard in `step_in_document`.
                    if self.coll_stack.is_empty() && !tail_trimmed.starts_with(':') {
                        self.root_node_emitted = true;
                    }
                    self.lexer.consume_line();
                    if !tail_trimmed.is_empty() {
                        let tail_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                        let synthetic = crate::lines::Line {
                            content: tail_content,
                            offset: tail_pos.byte_offset,
                            indent: tail_pos.column,
                            break_type: crate::lines::BreakType::Eof,
                            pos: tail_pos,
                        };
                        self.lexer.prepend_inline_line(synthetic);
                    }
                    break 'outer;
                }
                continue 'outer;
            }

            // ----------------------------------------------------------------
            // Comma separator
            // ----------------------------------------------------------------
            if ch == ',' {
                // Leading-comma check: if the current frame has not yet produced
                // any value since it was opened (or since the last comma), this
                // comma is invalid — e.g. `[,]` or `{,}`.
                let leading = match flow_stack.last() {
                    Some(
                        FlowFrame::Sequence { has_value, .. }
                        | FlowFrame::Mapping { has_value, .. },
                    ) => !has_value,
                    None => false,
                };
                if leading {
                    let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "invalid leading comma in flow collection".into(),
                    }));
                }

                pos_in_line += 1;

                // Skip whitespace after comma.
                while pos_in_line < cur_content.len() {
                    match cur_content[pos_in_line..].chars().next() {
                        Some(c) if c == ' ' || c == '\t' => pos_in_line += 1,
                        _ => break,
                    }
                }

                // Double-comma check: next char must not be another comma.
                if cur_content[pos_in_line..].starts_with(',') {
                    let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "invalid empty entry: consecutive commas in flow collection"
                            .into(),
                    }));
                }

                // Emit an implicit empty-scalar node when the comma terminates
                // an entry with no scalar yet: either a pending tag/anchor needs
                // attachment, or a flow mapping is in Value phase (key was emitted
                // but no value scalar followed before the comma), or a multiline
                // plain key just completed (phase reverted to Key for `:` recognition
                // but no `:` arrived — the comma ends the key with a null value).
                let in_mapping_value_phase = matches!(
                    flow_stack.last(),
                    Some(FlowFrame::Mapping {
                        phase: FlowMappingPhase::Value,
                        ..
                    })
                );
                let in_mapping_key_continuation = matches!(
                    flow_stack.last(),
                    Some(FlowFrame::Mapping {
                        key_continuation: true,
                        ..
                    })
                );
                if pending_flow_tag.is_some()
                    || pending_flow_anchor.is_some()
                    || in_mapping_value_phase
                    || in_mapping_key_continuation
                {
                    let empty_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    events.push((
                        Event::Scalar {
                            value: Cow::Borrowed(""),
                            style: ScalarStyle::Plain,
                            anchor: pending_flow_anchor.take(),
                            tag: pending_flow_tag.take(),
                        },
                        zero_span(empty_pos),
                    ));
                    // Advance phase: this scalar acts as a value (or key).
                    if let Some(frame) = flow_stack.last_mut() {
                        match frame {
                            FlowFrame::Sequence {
                                has_value,
                                after_colon,
                                last_was_plain,
                                ..
                            } => {
                                *has_value = true;
                                *after_colon = false;
                                *last_was_plain = false;
                            }
                            FlowFrame::Mapping {
                                phase,
                                has_value,
                                after_colon,
                                last_was_plain,
                                key_continuation,
                                explicit_key_pending,
                            } => {
                                *has_value = true;
                                *after_colon = false;
                                *last_was_plain = false;
                                *key_continuation = false;
                                *explicit_key_pending = false;
                                *phase = match *phase {
                                    FlowMappingPhase::Key => FlowMappingPhase::Value,
                                    FlowMappingPhase::Value => FlowMappingPhase::Key,
                                };
                            }
                        }
                    }
                }

                // If we were inside an implicit mapping entry in a sequence,
                // emit MappingEnd before the comma resets the frame.
                if let Some(FlowFrame::Sequence {
                    in_implicit_map: true,
                    ..
                }) = flow_stack.last()
                {
                    let sep_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    events.push((Event::MappingEnd, zero_span(sep_pos)));
                }

                // Reset has_value and (for mappings) go back to Key phase.
                if let Some(frame) = flow_stack.last_mut() {
                    match frame {
                        FlowFrame::Sequence {
                            has_value,
                            after_colon,
                            last_was_plain,
                            in_implicit_map,
                            key_start_idx,
                        } => {
                            *has_value = false;
                            *after_colon = false;
                            *last_was_plain = false;
                            *in_implicit_map = false;
                            *key_start_idx = events.len();
                        }
                        FlowFrame::Mapping {
                            phase,
                            has_value,
                            after_colon,
                            last_was_plain,
                            key_continuation,
                            explicit_key_pending,
                        } => {
                            *has_value = false;
                            *after_colon = false;
                            *last_was_plain = false;
                            *key_continuation = false;
                            *explicit_key_pending = false;
                            if *phase == FlowMappingPhase::Value {
                                *phase = FlowMappingPhase::Key;
                            }
                        }
                    }
                }
                // Reset last_token_line after a comma — the next key can start
                // on the same line as the comma (or any subsequent line) without
                // triggering the multi-line implicit key error.
                last_token_line = cur_base_pos.line;

                continue 'outer;
            }

            // ----------------------------------------------------------------
            // Block scalar indicators forbidden in flow context
            // ----------------------------------------------------------------
            if ch == '|' || ch == '>' {
                let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                self.state = IterState::Done;
                return StepResult::Yield(Err(Error {
                    pos: err_pos,
                    message: format!(
                        "block scalar indicator '{ch}' is not allowed inside a flow collection"
                    ),
                }));
            }

            // ----------------------------------------------------------------
            // Block sequence entry indicator `-` forbidden in flow context.
            //
            // Per YAML 1.2 §7.4, block collections cannot appear inside flow
            // context.  A `-` followed by space, tab, or end-of-content is
            // the block-sequence entry indicator; a `-` followed by any other
            // non-separator character is a valid plain-scalar start (e.g. `-x`
            // or `-1` are legal plain scalars in flow context).
            // ----------------------------------------------------------------
            if ch == '-' {
                let after = &cur_content[pos_in_line + 1..];
                let next_c = after.chars().next();
                if next_c.is_none_or(|c| matches!(c, ' ' | '\t')) {
                    let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "block sequence entry '-' is not allowed inside a flow collection"
                            .into(),
                    }));
                }
            }

            // ----------------------------------------------------------------
            // Quoted scalars — delegate to existing lexer methods.
            //
            // Strategy: consume the current line, prepend a synthetic line
            // starting exactly at the quote character, call the method, then
            // re-sync `cur_content` / `cur_base_pos` from the buffer.
            // ----------------------------------------------------------------
            if ch == '\'' || ch == '"' {
                // `remaining` borrows from `cur_content` which borrows from `'input`.
                // We capture it before touching the lexer buffer.
                let remaining: &'input str = &cur_content[pos_in_line..];
                let cur_abs_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);

                // Consume the current line from the buffer and replace it with
                // a synthetic line that starts at the quote character.  The
                // quoted-scalar method will consume this synthetic line entirely,
                // including any content after the closing quote — so we must
                // reconstruct the tail from `remaining` and `span` below.
                self.lexer.consume_line();
                let synthetic = crate::lines::Line {
                    content: remaining,
                    offset: cur_abs_pos.byte_offset,
                    indent: cur_abs_pos.column,
                    break_type: crate::lines::BreakType::Eof,
                    pos: cur_abs_pos,
                };
                self.lexer.prepend_inline_line(synthetic);

                // Call the appropriate quoted-scalar method.
                let result = if ch == '\'' {
                    self.lexer.try_consume_single_quoted(0)
                } else {
                    // Flow context: no block-indentation constraint on
                    // continuation lines of double-quoted scalars.
                    self.lexer.try_consume_double_quoted(None)
                };

                let (value, span) = match result {
                    Ok(Some(vs)) => vs,
                    Ok(None) => {
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(Error {
                            pos: cur_abs_pos,
                            message: "expected quoted scalar".into(),
                        }));
                    }
                    Err(e) => {
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(e));
                    }
                };

                let style = if ch == '\'' {
                    ScalarStyle::SingleQuoted
                } else {
                    ScalarStyle::DoubleQuoted
                };
                events.push((
                    Event::Scalar {
                        value,
                        style,
                        anchor: pending_flow_anchor.take(),
                        tag: pending_flow_tag.take(),
                    },
                    span,
                ));

                // Reconstruct the tail after the closing quote so the flow
                // parser can continue with `,`, `]`, `}`, etc.
                //
                // For single-line scalars, the tail is in `remaining` at byte
                // offset `span.end.byte_offset - cur_abs_pos.byte_offset`.
                //
                // For multiline scalars, the lexer's continuation loop consumed
                // additional input lines; the tail on the closing-quote line is
                // stored in `self.lexer.pending_multiline_tail`.  Drain it here.
                if let Some((tail, tail_pos)) = self.lexer.pending_multiline_tail.take() {
                    if !tail.is_empty() {
                        let tail_syn = crate::lines::Line {
                            content: tail,
                            offset: tail_pos.byte_offset,
                            indent: tail_pos.column,
                            break_type: crate::lines::BreakType::Eof,
                            pos: tail_pos,
                        };
                        self.lexer.prepend_inline_line(tail_syn);
                    }
                } else {
                    // Single-line scalar: derive tail from `remaining`.
                    let consumed_bytes = span.end.byte_offset - cur_abs_pos.byte_offset;
                    let tail_in_remaining = remaining.get(consumed_bytes..).unwrap_or("");
                    if !tail_in_remaining.is_empty() {
                        let tail_syn = crate::lines::Line {
                            content: tail_in_remaining,
                            offset: span.end.byte_offset,
                            indent: span.end.column,
                            break_type: crate::lines::BreakType::Eof,
                            pos: span.end,
                        };
                        self.lexer.prepend_inline_line(tail_syn);
                    }
                }

                // Re-sync from the buffer.
                (cur_content, cur_base_pos) = resync!();
                pos_in_line = 0;
                // Track where this quoted scalar (potential key) ended.
                last_token_line = cur_base_pos.line;

                if cur_content.is_empty() && self.lexer.at_eof() && !flow_stack.is_empty() {
                    let err_pos = self.lexer.current_pos();
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: "unterminated flow collection: unexpected end of input".into(),
                    }));
                }

                // Advance mapping phase for the emitted scalar; mark frame as having a value.
                if let Some(frame) = flow_stack.last_mut() {
                    match frame {
                        FlowFrame::Sequence {
                            has_value,
                            after_colon,
                            last_was_plain,
                            ..
                        } => {
                            *has_value = true;
                            *after_colon = false;
                            *last_was_plain = false;
                        }
                        FlowFrame::Mapping {
                            phase,
                            has_value,
                            after_colon,
                            last_was_plain,
                            key_continuation,
                            explicit_key_pending,
                        } => {
                            *has_value = true;
                            *after_colon = false;
                            *last_was_plain = false;
                            *key_continuation = false;
                            *explicit_key_pending = false;
                            *phase = match *phase {
                                FlowMappingPhase::Key => FlowMappingPhase::Value,
                                FlowMappingPhase::Value => FlowMappingPhase::Key,
                            };
                        }
                    }
                }

                continue 'outer;
            }

            // ----------------------------------------------------------------
            // Explicit key indicator `?` in flow mappings and sequences
            // ----------------------------------------------------------------
            if ch == '?' {
                let next_ch = cur_content[pos_in_line + 1..].chars().next();
                if next_ch.is_none_or(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) {
                    // `?` followed by whitespace/EOL: explicit key indicator.
                    // In a flow sequence, remember this so the DK4H single-line
                    // check is suppressed for the corresponding `:` separator.
                    if matches!(flow_stack.last(), Some(FlowFrame::Sequence { .. })) {
                        explicit_key_in_seq = true;
                    }
                    // In a flow mapping, record that an explicit-key indicator was
                    // seen so that a trailing `?` with no content (e.g. `{..., ?}`)
                    // emits a null-null pair at `}`.
                    if let Some(FlowFrame::Mapping {
                        explicit_key_pending,
                        ..
                    }) = flow_stack.last_mut()
                    {
                        *explicit_key_pending = true;
                    }
                    pos_in_line += 1;
                    continue 'outer;
                }
                // `?` not followed by whitespace — treat as plain scalar start.
            }

            // ----------------------------------------------------------------
            // `:` value separator in flow mappings
            // ----------------------------------------------------------------
            if ch == ':' {
                let next_ch = cur_content[pos_in_line + 1..].chars().next();
                // `:` is a value separator when:
                //   (a) followed by whitespace, delimiter, or EOL (standard case)
                //   (b) in a flow sequence with a synthetic current line (adjacent
                //       `:` from JSON-like key — YAML 1.2 §7.4.2)
                //   (c) in a flow mapping in Value phase — the key was already
                //       emitted, so `:` can be adjacent to the value (YAML 1.2
                //       §7.4.2 `c-ns-flow-map-separator` allows non-whitespace after
                //       the separator colon)
                let is_standard_sep =
                    next_ch.is_none_or(|c| matches!(c, ' ' | '\t' | ',' | ']' | '}' | '\n' | '\r'));
                let is_adjacent_json_sep = !is_standard_sep
                    && matches!(
                        flow_stack.last(),
                        Some(FlowFrame::Sequence {
                            has_value: true,
                            ..
                        })
                    )
                    && self.lexer.is_next_line_synthetic();
                let is_mapping_value_phase = !is_standard_sep
                    && matches!(
                        flow_stack.last(),
                        Some(FlowFrame::Mapping {
                            phase: FlowMappingPhase::Value,
                            after_colon: false,
                            ..
                        })
                    );
                let is_value_sep =
                    is_standard_sep || is_adjacent_json_sep || is_mapping_value_phase;
                if is_value_sep {
                    // Multi-line implicit single-pair mapping key check (YAML 1.2 §7.4.1):
                    // inside a flow sequence `[...]`, a single-pair mapping entry's key must
                    // be on the same line as the `:` separator.  (Flow mappings `{...}` allow
                    // multi-line implicit keys — see YAML 1.2 §7.4.2.)
                    // Exception: when a `?` explicit-key indicator was seen in this sequence
                    // (`explicit_key_in_seq`), the key may span multiple lines.
                    let in_sequence = matches!(flow_stack.last(), Some(FlowFrame::Sequence { .. }));
                    if in_sequence && cur_base_pos.line != last_token_line && !explicit_key_in_seq {
                        let colon_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(Error {
                            pos: colon_pos,
                            message: "implicit flow mapping key must be on a single line".into(),
                        }));
                    }
                    explicit_key_in_seq = false;
                    if let Some(frame) = flow_stack.last_mut() {
                        match frame {
                            FlowFrame::Mapping {
                                phase,
                                has_value,
                                after_colon,
                                last_was_plain,
                                key_continuation,
                                explicit_key_pending,
                            } => {
                                *last_was_plain = false;
                                *key_continuation = false;
                                *explicit_key_pending = false;
                                if *phase == FlowMappingPhase::Key {
                                    // When `:` arrives in Key phase and no key scalar has
                                    // been emitted yet (`has_value = false`), emit an
                                    // implicit empty key scalar (possibly with a pending
                                    // tag/anchor attached).
                                    if !*has_value {
                                        let key_pos =
                                            abs_pos(cur_base_pos, cur_content, pos_in_line);
                                        events.push((
                                            Event::Scalar {
                                                value: Cow::Borrowed(""),
                                                style: ScalarStyle::Plain,
                                                anchor: pending_flow_anchor.take(),
                                                tag: pending_flow_tag.take(),
                                            },
                                            zero_span(key_pos),
                                        ));
                                        *has_value = true;
                                    }
                                    *phase = FlowMappingPhase::Value;
                                } else {
                                    // `:` arriving in Value phase: the key was already
                                    // emitted; this `:` is the value separator.  Mark
                                    // `after_colon = true` so that a value scalar starting
                                    // with `:` (e.g. `{x: :x}`) is not confused with a
                                    // second separator.
                                    *after_colon = true;
                                }
                            }
                            FlowFrame::Sequence {
                                has_value,
                                after_colon,
                                last_was_plain,
                                in_implicit_map,
                                key_start_idx,
                            } => {
                                // `:` as value separator in a sequence: this is a
                                // single-pair implicit mapping entry (YAML 1.2 §7.4.1).
                                // Insert MappingStart at key_start_idx so the key is
                                // wrapped: MappingStart, key, value, MappingEnd.
                                let colon_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                                // If no key was emitted yet (empty key case like `: val`),
                                // push an empty scalar first, then insert MappingStart before it.
                                if !*has_value {
                                    events.push((empty_scalar_event(), zero_span(colon_pos)));
                                    *has_value = true;
                                }
                                events.insert(
                                    *key_start_idx,
                                    (
                                        Event::MappingStart {
                                            anchor: None,
                                            tag: None,
                                            style: CollectionStyle::Flow,
                                        },
                                        zero_span(colon_pos),
                                    ),
                                );
                                *in_implicit_map = true;
                                // Mark `after_colon` so the next scalar or collection is
                                // not rejected for missing a comma.
                                *after_colon = true;
                                // Reset last_was_plain so the value scalar on the next
                                // line is not appended to the key via multi-line
                                // plain-scalar continuation logic.
                                *last_was_plain = false;
                            }
                        }
                    }
                    pos_in_line += 1;
                    continue 'outer;
                }
                // `:` not followed by separator — treat as plain scalar char.
            }

            // ----------------------------------------------------------------
            // Tag `!tag`, `!!tag`, `!<uri>`, or `!` in flow context
            // ----------------------------------------------------------------
            if ch == '!' {
                let bang_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                let after_bang = &cur_content[pos_in_line + 1..];
                let tag_start = &cur_content[pos_in_line..];
                match scan_tag(after_bang, tag_start, bang_pos) {
                    Err(e) => {
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(e));
                    }
                    Ok((tag_slice, advance_past_bang)) => {
                        // Total bytes: 1 (`!`) + advance_past_bang.
                        // `!<URI>`: advance_past_bang = 1 + uri.len() + 1
                        // `!!suffix`: advance_past_bang = 1 + suffix.len()
                        // `!suffix`: advance_past_bang = suffix.len()
                        // `!` alone: advance_past_bang = 0
                        if pending_flow_tag.is_some() {
                            self.state = IterState::Done;
                            return StepResult::Yield(Err(Error {
                                pos: bang_pos,
                                message: "a node may not have more than one tag".into(),
                            }));
                        }
                        // Resolve tag handle against directive scope at scan time.
                        let resolved_flow_tag =
                            match self.directive_scope.resolve_tag(tag_slice, bang_pos) {
                                Ok(t) => t,
                                Err(e) => {
                                    self.state = IterState::Done;
                                    return StepResult::Yield(Err(e));
                                }
                            };
                        pending_flow_tag = Some(resolved_flow_tag);
                        pos_in_line += 1 + advance_past_bang;
                        // Skip any whitespace after the tag.
                        while pos_in_line < cur_content.len() {
                            match cur_content[pos_in_line..].chars().next() {
                                Some(c) if c == ' ' || c == '\t' => pos_in_line += 1,
                                _ => break,
                            }
                        }
                        continue 'outer;
                    }
                }
            }

            // ----------------------------------------------------------------
            // Anchor `&name` in flow context
            // ----------------------------------------------------------------
            if ch == '&' {
                let after_amp = &cur_content[pos_in_line + 1..];
                let amp_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                match scan_anchor_name(after_amp, amp_pos) {
                    Err(e) => {
                        self.state = IterState::Done;
                        return StepResult::Yield(Err(e));
                    }
                    Ok(name) => {
                        // Two anchors on the same flow node are an error.
                        if pending_flow_anchor.is_some() {
                            let amp_pos2 = abs_pos(cur_base_pos, cur_content, pos_in_line);
                            self.state = IterState::Done;
                            return StepResult::Yield(Err(Error {
                                pos: amp_pos2,
                                message: "a node may not have more than one anchor".into(),
                            }));
                        }
                        pending_flow_anchor = Some(name);
                        pos_in_line += 1 + name.len();
                        // Skip any whitespace after the anchor name.
                        while pos_in_line < cur_content.len() {
                            match cur_content[pos_in_line..].chars().next() {
                                Some(c) if c == ' ' || c == '\t' => pos_in_line += 1,
                                _ => break,
                            }
                        }
                        continue 'outer;
                    }
                }
            }

            // ----------------------------------------------------------------
            // Alias `*name` in flow context
            // ----------------------------------------------------------------
            if ch == '*' {
                let after_star = &cur_content[pos_in_line + 1..];
                let star_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                // YAML 1.2 §7.1: alias nodes cannot have properties (anchor or tag).
                if pending_flow_tag.is_some() {
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: star_pos,
                        message: "alias node cannot have a tag property".into(),
                    }));
                }
                if pending_flow_anchor.is_some() {
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
                        let alias_end = Pos {
                            byte_offset: star_pos.byte_offset + 1 + name.len(),
                            line: star_pos.line,
                            column: star_pos.column + 1 + name.chars().count(),
                        };
                        let alias_span = Span {
                            start: star_pos,
                            end: alias_end,
                        };
                        events.push((Event::Alias { name }, alias_span));
                        pos_in_line += 1 + name.len();
                        // Advance mapping phase; mark frame as having a value.
                        if let Some(frame) = flow_stack.last_mut() {
                            match frame {
                                FlowFrame::Sequence {
                                    has_value,
                                    after_colon,
                                    last_was_plain,
                                    ..
                                } => {
                                    *has_value = true;
                                    *after_colon = false;
                                    *last_was_plain = false;
                                }
                                FlowFrame::Mapping {
                                    phase,
                                    has_value,
                                    after_colon,
                                    last_was_plain,
                                    key_continuation,
                                    explicit_key_pending,
                                } => {
                                    *has_value = true;
                                    *after_colon = false;
                                    *last_was_plain = false;
                                    *key_continuation = false;
                                    *explicit_key_pending = false;
                                    *phase = match *phase {
                                        FlowMappingPhase::Key => FlowMappingPhase::Value,
                                        FlowMappingPhase::Value => FlowMappingPhase::Key,
                                    };
                                }
                            }
                        }
                        continue 'outer;
                    }
                }
            }

            // ----------------------------------------------------------------
            // Multi-line plain scalar continuation in flow context
            //
            // A plain scalar may span multiple lines (YAML §7.3.3).  When the
            // previous emitted token was a plain scalar (`last_was_plain`) and
            // the current character is a valid `ns-plain-char` (i.e. it can
            // appear within a plain scalar body, even if it cannot *start* one),
            // extend the in-progress scalar rather than treating the character
            // as the start of a new token.
            //
            // `ns-plain-char` in flow context: any `ns-char` that is not `:` or
            // `#`, plus `:` followed by ns-plain-safe, plus `#` not preceded by
            // whitespace.  At the start of a continuation line all leading
            // whitespace has been consumed, so `#` at position 0 here would be
            // `#` after whitespace — a comment start, not a continuation char.
            // ----------------------------------------------------------------
            {
                // For flow MAPPINGS: a plain scalar may continue a key only when
                // the phase is currently Value — meaning the previous scalar was
                // a KEY (Key→Value phase advance was done when emitting it).  A
                // VALUE scalar (phase Value→Key) must NOT continue: the next line
                // is a new key that requires a preceding comma.
                // For flow SEQUENCES: `last_was_plain` alone is enough (single-pair
                // implicit mapping keys can span lines, and regular sequence items
                // can also continue, though commas terminate them).
                let frame_last_was_plain = matches!(
                    flow_stack.last(),
                    Some(
                        FlowFrame::Mapping {
                            last_was_plain: true,
                            phase: FlowMappingPhase::Value,
                            ..
                        } | FlowFrame::Sequence {
                            last_was_plain: true,
                            ..
                        }
                    )
                );
                // `ns-plain-char` check: ch must not be a flow terminator, `:` (alone),
                // or `#` (comment start after whitespace, which is the only `#` we can
                // see here since whitespace was consumed).
                let is_ns_plain_char_continuation = frame_last_was_plain
                    && !matches!(ch, ',' | '[' | ']' | '{' | '}' | '#')
                    && (ch != ':' || {
                        let after = &cur_content[pos_in_line + 1..];
                        let next_c = after.chars().next();
                        // `:` is a valid continuation char only when NOT followed by
                        // a separator (space, tab, flow indicator, or end-of-line).
                        next_c.is_some_and(|nc| {
                            !matches!(nc, ' ' | '\t' | ',' | '[' | ']' | '{' | '}')
                        })
                    });

                if is_ns_plain_char_continuation {
                    let slice = &cur_content[pos_in_line..];
                    let scanned = scan_plain_line_flow(slice);
                    if !scanned.is_empty() {
                        // Extend the most-recently-emitted scalar event with a
                        // line-fold (space) and the continuation content.
                        if let Some((
                            Event::Scalar {
                                value,
                                style: ScalarStyle::Plain,
                                ..
                            },
                            _,
                        )) = events.last_mut()
                        {
                            let extended = format!("{value} {scanned}");
                            *value = Cow::Owned(extended);
                        }
                        pos_in_line += scanned.len();
                        // Update last_token_line to this line so the DK4H
                        // multi-line implicit-key check remains anchored to the
                        // last real token (the continuation content).
                        last_token_line = cur_base_pos.line;
                        // The continuation may itself end at EOL, leaving the scalar
                        // still incomplete.  Keep `last_was_plain` true and, for
                        // mappings, revert the phase back to Key so that the `: `
                        // separator is still recognised.
                        if let Some(frame) = flow_stack.last_mut() {
                            match frame {
                                FlowFrame::Mapping {
                                    phase,
                                    last_was_plain,
                                    key_continuation,
                                    ..
                                } => {
                                    // Undo the premature Key→Value advance: the key is not
                                    // yet complete until `: ` is seen.
                                    *phase = FlowMappingPhase::Key;
                                    *last_was_plain = true;
                                    // Mark that a key is in continuation so a `,` can emit
                                    // an implicit null value for this key.
                                    *key_continuation = true;
                                }
                                FlowFrame::Sequence { last_was_plain, .. } => {
                                    *last_was_plain = true;
                                }
                            }
                        }
                        continue 'outer;
                    }
                }
            }

            // ----------------------------------------------------------------
            // Plain scalar in flow context
            // ----------------------------------------------------------------
            {
                // Indicator characters that cannot start a plain scalar in flow.
                let is_plain_first = if matches!(
                    ch,
                    ',' | '['
                        | ']'
                        | '{'
                        | '}'
                        | '#'
                        | '&'
                        | '*'
                        | '!'
                        | '|'
                        | '>'
                        | '\''
                        | '"'
                        | '%'
                        | '@'
                        | '`'
                ) {
                    false
                } else if matches!(ch, '?' | ':' | '-') {
                    // These start a plain scalar only if followed by a safe char.
                    let after = &cur_content[pos_in_line + ch.len_utf8()..];
                    let next_c = after.chars().next();
                    next_c.is_some_and(|nc| !matches!(nc, ' ' | '\t' | ',' | '[' | ']' | '{' | '}'))
                } else {
                    true
                };

                if is_plain_first {
                    // Missing-comma check: in a flow collection with has_value=true,
                    // a new plain scalar is starting without a preceding comma —
                    // YAML 1.2 §7.4 requires commas between entries.
                    match flow_stack.last() {
                        Some(FlowFrame::Mapping {
                            phase: FlowMappingPhase::Key,
                            has_value: true,
                            ..
                        }) => {
                            let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                            self.state = IterState::Done;
                            return StepResult::Yield(Err(Error {
                                pos: err_pos,
                                message: "missing comma between flow mapping entries".into(),
                            }));
                        }
                        Some(FlowFrame::Sequence {
                            has_value: true,
                            after_colon: false,
                            last_was_plain: false,
                            ..
                        }) => {
                            let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                            self.state = IterState::Done;
                            return StepResult::Yield(Err(Error {
                                pos: err_pos,
                                message: "missing comma between flow sequence entries".into(),
                            }));
                        }
                        _ => {}
                    }
                    let slice = &cur_content[pos_in_line..];
                    let scanned = scan_plain_line_flow(slice);
                    if !scanned.is_empty() {
                        let scalar_start = abs_pos(cur_base_pos, cur_content, pos_in_line);
                        let scalar_end =
                            abs_pos(cur_base_pos, cur_content, pos_in_line + scanned.len());
                        let scalar_span = Span {
                            start: scalar_start,
                            end: scalar_end,
                        };

                        events.push((
                            Event::Scalar {
                                value: Cow::Borrowed(scanned),
                                style: ScalarStyle::Plain,
                                anchor: pending_flow_anchor.take(),
                                tag: pending_flow_tag.take(),
                            },
                            scalar_span,
                        ));
                        pos_in_line += scanned.len();
                        // Track where this scalar (potential key) ended for the
                        // multi-line implicit key check (DK4H).
                        last_token_line = cur_base_pos.line;

                        // Advance mapping phase; mark frame as having a value.
                        if let Some(frame) = flow_stack.last_mut() {
                            match frame {
                                FlowFrame::Sequence {
                                    has_value,
                                    after_colon,
                                    last_was_plain,
                                    ..
                                } => {
                                    *has_value = true;
                                    *after_colon = false;
                                    *last_was_plain = true; // plain scalars may continue
                                }
                                FlowFrame::Mapping {
                                    phase,
                                    has_value,
                                    after_colon,
                                    last_was_plain,
                                    key_continuation,
                                    explicit_key_pending,
                                } => {
                                    *has_value = true;
                                    *after_colon = false;
                                    *last_was_plain = true; // plain scalars may continue on next line
                                    *key_continuation = false;
                                    *explicit_key_pending = false;
                                    *phase = match *phase {
                                        FlowMappingPhase::Key => FlowMappingPhase::Value,
                                        FlowMappingPhase::Value => FlowMappingPhase::Key,
                                    };
                                }
                            }
                        }
                        continue 'outer;
                    }
                }

                // Reserved indicators — task 19 will handle directives.
                // `!` (tags), `&`/`*` (anchors/aliases) are handled above.
                // Silently skipping remaining reserved indicators would mangle
                // YAML structure, so we error early here.
                if matches!(ch, '%' | '@' | '`') {
                    let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                    self.state = IterState::Done;
                    return StepResult::Yield(Err(Error {
                        pos: err_pos,
                        message: format!(
                            "indicator '{ch}' inside flow collection is not yet supported"
                        ),
                    }));
                }

                // Any other character that is not a plain-scalar start and is
                // not an indicator handled above (e.g. C0 control characters,
                // DEL, C1 controls, surrogates) is invalid here. Error rather
                // than panicking — this is user-supplied input.
                let err_pos = abs_pos(cur_base_pos, cur_content, pos_in_line);
                self.state = IterState::Done;
                return StepResult::Yield(Err(Error {
                    pos: err_pos,
                    message: format!("invalid character {ch:?} inside flow collection"),
                }));
            }
        }

        // When this flow collection is an implicit complex mapping key
        // (`[flow]: value` or `{ flow }: value`), prepend a block MappingStart
        // event before the flow collection's events and push a Mapping entry
        // onto the collection stack so the subsequent `: value` synthetic is
        // handled correctly.
        //
        // `property_origin_indent` carries the original physical line's indent
        // when an anchor or tag appeared before the `[`/`{` — use it so the
        // mapping opens at the right document column.  Always clear it here
        // (consumed if a key, unused otherwise).
        if let Some(map_col) = implicit_key_mapping_indent {
            let mapping_anchor = self.pending_collection_anchor.take();
            let mapping_tag = self.pending_collection_tag.take();
            events.insert(
                0,
                (
                    Event::MappingStart {
                        anchor: mapping_anchor,
                        tag: mapping_tag,
                        style: crate::event::CollectionStyle::Block,
                    },
                    zero_span(first_line_pos),
                ),
            );
            self.coll_stack
                .push(CollectionEntry::Mapping(map_col, MappingPhase::Key, false));
        }
        self.property_origin_indent = None;

        // Tick the parent block mapping phase (if any) after completing a flow
        // collection that was a key or value in a block mapping.
        self.tick_mapping_phase_after_scalar();

        // Push all accumulated events to the queue.
        self.queue.extend(events);
        StepResult::Continue
    }
}
