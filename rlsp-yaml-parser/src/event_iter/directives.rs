// SPDX-License-Identifier: MIT

//! Directive parsing and `BetweenDocs` stepper.
//! Methods on `EventIter` that handle %YAML, %TAG, and the
//! transition between documents in a stream.

use super::properties::is_valid_tag_handle;
use super::state::{IterState, StepResult};
use crate::error::Error;
use crate::event::Event;
use crate::limits::{MAX_COMMENT_LEN, MAX_DIRECTIVES_PER_DOC, MAX_TAG_HANDLE_BYTES, MAX_TAG_LEN};
use crate::pos::Pos;
use crate::{EventIter, marker_span, zero_span};

impl<'input> EventIter<'input> {
    /// Consume blank lines, comment lines, and directive lines in `BetweenDocs`
    /// context.
    ///
    /// - Blank lines: silently consumed.
    /// - Comment lines: emitted as `Event::Comment` items into `self.queue`.
    /// - Directive lines (`%`-prefixed): parsed and accumulated into
    ///   `self.directive_scope`.
    ///
    /// Returns `Err` on malformed directives, exceeded limits, or comment
    /// bodies exceeding `MAX_COMMENT_LEN`.  Stops at the first non-blank,
    /// non-comment, non-directive line (i.e. `---`, `...`, or content).
    ///
    /// The caller is responsible for resetting `self.directive_scope` before
    /// entering the `BetweenDocs` state (at each document boundary transition).
    /// This function does NOT reset it — `step_between_docs` re-enters it on
    /// every comment yield, so resetting here would clobber directives parsed
    /// on earlier re-entries for the same document.
    pub(crate) fn consume_preamble_between_docs(&mut self) -> Result<(), Error> {
        loop {
            // Skip blank lines first.
            self.lexer.skip_blank_lines_between_docs();

            // Collect comment lines.
            while self.lexer.is_comment_line() {
                match self.lexer.try_consume_comment(MAX_COMMENT_LEN) {
                    Ok(Some((text, span))) => {
                        self.queue.push_back((Event::Comment { text }, span));
                    }
                    Ok(None) => break,
                    Err(e) => return Err(e),
                }
                self.lexer.skip_blank_lines_between_docs();
            }

            // Parse directive lines.
            while self.lexer.is_directive_line() {
                let Some((content, dir_pos)) = self.lexer.try_consume_directive_line() else {
                    break;
                };
                self.parse_directive(content, dir_pos)?;
                self.lexer.skip_blank_lines_between_docs();
            }

            // After parsing directives, there may be more blank lines or comments.
            if !self.lexer.is_comment_line() && !self.lexer.is_directive_line() {
                return Ok(());
            }
        }
    }

    /// Parse a single directive line and update `self.directive_scope`.
    ///
    /// `content` is the full line content starting with `%` (e.g. `"%YAML 1.2"`).
    /// `dir_pos` is the position of the `%` character.
    pub(crate) fn parse_directive(
        &mut self,
        content: &'input str,
        dir_pos: Pos,
    ) -> Result<(), Error> {
        // Enforce per-document directive count limit.
        if self.directive_scope.directive_count >= MAX_DIRECTIVES_PER_DOC {
            return Err(Error {
                pos: dir_pos,
                message: format!(
                    "directive count exceeds maximum of {MAX_DIRECTIVES_PER_DOC} per document"
                ),
            });
        }

        // `content` starts with `%`; the rest is `NAME[ params...]`.
        let after_percent = &content[1..];

        // Determine directive name (up to first whitespace).
        let name_end = after_percent
            .find([' ', '\t'])
            .unwrap_or(after_percent.len());
        let name = &after_percent[..name_end];
        let rest = after_percent[name_end..].trim_start_matches([' ', '\t']);

        match name {
            "YAML" => self.parse_yaml_directive(rest, dir_pos),
            "TAG" => self.parse_tag_directive(rest, dir_pos),
            _ => {
                // Reserved directive — silently ignore per YAML 1.2 spec.
                self.directive_scope.directive_count += 1;
                Ok(())
            }
        }
    }

    /// Parse `%YAML major.minor` and store in directive scope.
    pub(crate) fn parse_yaml_directive(&mut self, params: &str, dir_pos: Pos) -> Result<(), Error> {
        if self.directive_scope.version.is_some() {
            return Err(Error {
                pos: dir_pos,
                message: "duplicate %YAML directive in the same document".into(),
            });
        }

        // Parse `major.minor`.
        let dot = params.find('.').ok_or_else(|| Error {
            pos: dir_pos,
            message: format!("malformed %YAML directive: expected 'major.minor', got {params:?}"),
        })?;
        let major_str = &params[..dot];
        let after_dot = &params[dot + 1..];
        // Minor version ends at first whitespace or end of string.
        let minor_end = after_dot.find([' ', '\t']).unwrap_or(after_dot.len());
        let minor_str = &after_dot[..minor_end];
        // Anything after the minor version must be empty or a comment (# ...).
        let trailing = after_dot[minor_end..].trim_start_matches([' ', '\t']);
        if !trailing.is_empty() && !trailing.starts_with('#') {
            return Err(Error {
                pos: dir_pos,
                message: format!(
                    "malformed %YAML directive: unexpected trailing content {trailing:?}"
                ),
            });
        }

        let major = major_str.parse::<u8>().map_err(|_| Error {
            pos: dir_pos,
            message: format!("malformed %YAML major version: {major_str:?}"),
        })?;
        let minor = minor_str.parse::<u8>().map_err(|_| Error {
            pos: dir_pos,
            message: format!("malformed %YAML minor version: {minor_str:?}"),
        })?;

        // Only major version 1 is accepted; 2+ is a hard error.
        if major != 1 {
            return Err(Error {
                pos: dir_pos,
                message: format!("unsupported YAML version {major}.{minor}: only 1.x is supported"),
            });
        }

        self.directive_scope.version = Some((major, minor));
        self.directive_scope.directive_count += 1;
        Ok(())
    }

    /// Parse `%TAG !handle! prefix` and store in directive scope.
    pub(crate) fn parse_tag_directive(
        &mut self,
        params: &'input str,
        dir_pos: Pos,
    ) -> Result<(), Error> {
        // Split on whitespace to get handle and prefix.
        let handle_end = params.find([' ', '\t']).ok_or_else(|| Error {
            pos: dir_pos,
            message: format!("malformed %TAG directive: expected 'handle prefix', got {params:?}"),
        })?;
        let handle = &params[..handle_end];
        let prefix = params[handle_end..].trim_start_matches([' ', '\t']);

        if prefix.is_empty() {
            return Err(Error {
                pos: dir_pos,
                message: "malformed %TAG directive: missing prefix".into(),
            });
        }

        // Validate handle shape: must be `!`, `!!`, or `!<word-chars>!`
        // where word chars are ASCII alphanumeric, `-`, or `_`
        // (YAML 1.2 §6.8.1 productions [89]–[92]).
        if !is_valid_tag_handle(handle) {
            return Err(Error {
                pos: dir_pos,
                message: format!("malformed %TAG handle: {handle:?} is not a valid tag handle"),
            });
        }

        // Validate handle length.
        if handle.len() > MAX_TAG_HANDLE_BYTES {
            return Err(Error {
                pos: dir_pos,
                message: format!(
                    "tag handle exceeds maximum length of {MAX_TAG_HANDLE_BYTES} bytes"
                ),
            });
        }

        // Validate prefix length.
        if prefix.len() > MAX_TAG_LEN {
            return Err(Error {
                pos: dir_pos,
                message: format!("tag prefix exceeds maximum length of {MAX_TAG_LEN} bytes"),
            });
        }

        // Reject control characters in prefix.
        for ch in prefix.chars() {
            if (ch as u32) < 0x20 || ch == '\x7F' {
                return Err(Error {
                    pos: dir_pos,
                    message: format!("tag prefix contains invalid control character {ch:?}"),
                });
            }
        }

        // Duplicate handle check.
        if self.directive_scope.tag_handles.contains_key(handle) {
            return Err(Error {
                pos: dir_pos,
                message: format!("duplicate %TAG directive for handle {handle:?}"),
            });
        }

        self.directive_scope
            .tag_handles
            .insert(handle.to_owned(), prefix.to_owned());
        self.directive_scope.directive_count += 1;
        Ok(())
    }

    /// Skip blank lines while collecting any comment lines encountered as
    /// `Event::Comment` items pushed to `self.queue`.
    ///
    /// Used in `InDocument` context.
    /// Returns `Err` if a comment body exceeds `MAX_COMMENT_LEN`.
    pub(crate) fn skip_and_collect_comments_in_doc(&mut self) -> Result<(), Error> {
        loop {
            // Skip truly blank lines (not comments).
            self.lexer.skip_empty_lines();
            // Collect any comment lines.
            if !self.lexer.is_comment_line() {
                return Ok(());
            }
            while self.lexer.is_comment_line() {
                match self.lexer.try_consume_comment(MAX_COMMENT_LEN) {
                    Ok(Some((text, span))) => {
                        self.queue.push_back((Event::Comment { text }, span));
                    }
                    Ok(None) => break,
                    Err(e) => return Err(e),
                }
            }
            // Loop to skip any blank lines that follow the comments.
        }
    }

    /// Handle one iteration step in the `BetweenDocs` state.
    pub(crate) fn step_between_docs(&mut self) -> StepResult<'input> {
        match self.consume_preamble_between_docs() {
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

        if self.lexer.at_eof() {
            // Per YAML 1.2 §9.2, directives require a `---` marker.
            // A directive followed by EOF (no `---`) is a spec violation.
            if self.directive_scope.directive_count > 0 {
                let pos = self.lexer.current_pos();
                self.state = IterState::Done;
                return StepResult::Yield(Err(Error {
                    pos,
                    message: "directives must be followed by a '---' document-start marker".into(),
                }));
            }
            let end = self.lexer.current_pos();
            self.state = IterState::Done;
            return StepResult::Yield(Ok((Event::StreamEnd, zero_span(end))));
        }
        if self.lexer.is_directives_end() {
            let (marker_pos, _) = self.lexer.consume_marker_line(false);
            if let Some(e) = self.lexer.marker_inline_error.take() {
                self.state = IterState::Done;
                return StepResult::Yield(Err(e));
            }
            self.state = IterState::InDocument;
            self.root_node_emitted = false;
            // Take the accumulated directives — scope stays active for document body tag resolution.
            let version = self.directive_scope.version;
            let tag_directives = self.directive_scope.tag_directives();
            self.queue.push_back((
                Event::DocumentStart {
                    explicit: true,
                    version,
                    tag_directives,
                },
                marker_span(marker_pos),
            ));
            self.drain_trailing_comment();
            return StepResult::Continue;
        }
        if self.lexer.is_document_end() {
            // Orphan `...` — if directives were parsed without a `---` marker,
            // that is a spec violation (YAML 1.2 §9.2: directives require `---`).
            if self.directive_scope.directive_count > 0 {
                let pos = self.lexer.current_pos();
                self.state = IterState::Done;
                return StepResult::Yield(Err(Error {
                    pos,
                    message: "directives must be followed by a '---' document-start marker".into(),
                }));
            }
            self.lexer.consume_marker_line(true);
            if let Some(e) = self.lexer.marker_inline_error.take() {
                self.state = IterState::Done;
                return StepResult::Yield(Err(e));
            }
            return StepResult::Continue; // orphan `...`, no event
        }
        // Per YAML 1.2 §9.2, directives require a `---` marker.  If the next
        // line is not `---` and we have already parsed directives, that is a
        // spec violation — reject before emitting an implicit DocumentStart.
        if self.directive_scope.directive_count > 0 {
            let pos = self.lexer.current_pos();
            self.state = IterState::Done;
            return StepResult::Yield(Err(Error {
                pos,
                message: "directives must be followed by a '---' document-start marker".into(),
            }));
        }
        debug_assert!(
            self.lexer.has_content(),
            "expected content after consuming blank/comment/directive lines"
        );
        let content_pos = self.lexer.current_pos();
        self.state = IterState::InDocument;
        self.root_node_emitted = false;
        // Take the accumulated directives — scope stays active for document body tag resolution.
        let version = self.directive_scope.version;
        let tag_directives = self.directive_scope.tag_directives();
        StepResult::Yield(Ok((
            Event::DocumentStart {
                explicit: false,
                version,
                tag_directives,
            },
            zero_span(content_pos),
        )))
    }
}
