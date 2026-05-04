// SPDX-License-Identifier: MIT

//! Directive parsing and `BetweenDocs` stepper.
//! Methods on `EventIter` that handle %YAML, %TAG, and the
//! transition between documents in a stream.

use super::properties::is_valid_tag_handle;
use super::state::{IterState, StepResult};
use crate::chars::{is_ns_char, is_ns_tag_char_single, is_ns_uri_char_single};
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

        // Validate directive name: every character must be ns-char ([84]).
        for ch in name.chars() {
            if !is_ns_char(ch) {
                return Err(Error {
                    pos: dir_pos,
                    message: format!(
                        "directive name contains non-printable character U+{:04X}",
                        ch as u32
                    ),
                });
            }
        }

        // Compute the byte offset of `rest` (params) from the start of `content`
        // (`%`), so that parse_yaml_directive can compute precise digit positions.
        // SAFETY: both `content` and `rest` are slices of the same input string;
        // `rest` is a trimmed sub-slice of `after_percent[name_end..]`.
        // The subtraction gives the number of bytes from `%` to the start of params.
        let params_offset = rest.as_ptr() as usize - content.as_ptr() as usize;
        match name {
            "YAML" => self.parse_yaml_directive(rest, dir_pos, params_offset),
            "TAG" => self.parse_tag_directive(rest, dir_pos),
            _ => {
                // Reserved directive — validate parameters then silently ignore per YAML 1.2 spec.
                for token in rest.split_ascii_whitespace() {
                    for ch in token.chars() {
                        if !is_ns_char(ch) {
                            return Err(Error {
                                pos: dir_pos,
                                message: format!(
                                    "directive parameter contains non-printable character U+{:04X}",
                                    ch as u32
                                ),
                            });
                        }
                    }
                }
                self.directive_scope.directive_count += 1;
                Ok(())
            }
        }
    }

    /// Parse `%YAML major.minor` and store in directive scope.
    ///
    /// `params_offset` is the byte distance from the `%` character (`dir_pos`)
    /// to the start of `params` within the directive line.  It is used to
    /// compute precise error positions pointing at individual version digits
    /// instead of the `%` sign.
    pub(crate) fn parse_yaml_directive(
        &mut self,
        params: &str,
        dir_pos: Pos,
        params_offset: usize,
    ) -> Result<(), Error> {
        if self.directive_scope.version.is_some() {
            return Err(Error {
                pos: dir_pos,
                message: "duplicate %YAML directive in the same document".into(),
            });
        }

        // Pre-validate: every character in the parameter must be ns-char ([85]).
        // Stop at the first comment character (`#`) since trailing comments are allowed.
        let param_body = params.trim_start_matches([' ', '\t']);
        let param_body = param_body.find('#').map_or(param_body, |pos| {
            param_body[..pos].trim_end_matches([' ', '\t'])
        });
        for ch in param_body.chars() {
            if !is_ns_char(ch) {
                return Err(Error {
                    pos: dir_pos,
                    message: format!(
                        "directive parameter contains non-printable character U+{:04X}",
                        ch as u32
                    ),
                });
            }
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

        // Compute positions of the major and minor digit strings within the line.
        // `params_offset` bytes from `%` to the start of params; major_str starts
        // at params[0], so major is at dir_pos + params_offset bytes.
        // `minor_str` follows the dot: dir_pos + params_offset + dot + 1 bytes.
        let major_pos = Pos {
            byte_offset: dir_pos.byte_offset + params_offset,
            line: dir_pos.line,
            column: dir_pos.column + params_offset,
        };
        let minor_pos = Pos {
            byte_offset: dir_pos.byte_offset + params_offset + dot + 1,
            line: dir_pos.line,
            column: dir_pos.column + params_offset + dot + 1,
        };

        let major = major_str.parse::<u8>().map_err(|_| Error {
            pos: major_pos,
            message: format!("malformed %YAML major version: {major_str:?}"),
        })?;
        let minor = minor_str.parse::<u8>().map_err(|_| Error {
            pos: minor_pos,
            message: format!("malformed %YAML minor version: {minor_str:?}"),
        })?;

        // Only major version 1 is accepted; 2+ is a hard error.
        if major != 1 {
            return Err(Error {
                pos: major_pos,
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
        // Pre-validate: every non-whitespace character in the parameters must be ns-char ([85]).
        for token in params.split_ascii_whitespace() {
            for ch in token.chars() {
                if !is_ns_char(ch) {
                    return Err(Error {
                        pos: dir_pos,
                        message: format!(
                            "directive parameter contains non-printable character U+{:04X}",
                            ch as u32
                        ),
                    });
                }
            }
        }

        // Split on whitespace to get handle and prefix.
        let handle_end = params.find([' ', '\t']).ok_or_else(|| Error {
            pos: dir_pos,
            message: format!("malformed %TAG directive: expected 'handle prefix', got {params:?}"),
        })?;
        let handle = &params[..handle_end];
        let raw_prefix = params[handle_end..].trim_start_matches([' ', '\t']);

        if raw_prefix.is_empty() {
            return Err(Error {
                pos: dir_pos,
                message: "malformed %TAG directive: missing prefix".into(),
            });
        }

        // Split the raw prefix at the first space/tab: everything before is the
        // prefix body (ns-tag-prefix); everything after must be empty or a comment.
        // YAML 1.2.2 §6.8.2 grammar: `ns-tag-prefix` is followed by `s-l-comments`,
        // not by arbitrary content — `#` is a valid ns-uri-char but only inside the
        // prefix body before any whitespace. The correct terminator is whitespace
        // because ns-uri-char excludes space/tab.
        let prefix_body = raw_prefix
            .find([' ', '\t'])
            .map_or(raw_prefix, |ws| &raw_prefix[..ws]);
        let trailing_after_prefix = raw_prefix[prefix_body.len()..].trim_start_matches([' ', '\t']);
        if !trailing_after_prefix.is_empty() && !trailing_after_prefix.starts_with('#') {
            return Err(Error {
                pos: dir_pos,
                message: "malformed %TAG directive: unexpected trailing content after prefix"
                    .into(),
            });
        }

        // Validate handle shape: must be `!`, `!!`, or `!<word-chars>!`
        // where word chars are ASCII alphanumeric or `-`
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
        if prefix_body.len() > MAX_TAG_LEN {
            return Err(Error {
                pos: dir_pos,
                message: format!("tag prefix exceeds maximum length of {MAX_TAG_LEN} bytes"),
            });
        }

        // Validate prefix against ns-uri-char ([93]/[94]/[95]).
        validate_tag_prefix(prefix_body).map_err(|offset| Error {
            pos: dir_pos,
            message: format!(
                "tag prefix contains character not allowed in URI at byte offset {offset}"
            ),
        })?;

        // Duplicate handle check.
        if self.directive_scope.tag_handles.contains_key(handle) {
            return Err(Error {
                pos: dir_pos,
                message: format!("duplicate %TAG directive for handle {handle:?}"),
            });
        }

        self.directive_scope
            .tag_handles
            .insert(handle.to_owned(), prefix_body.to_owned());
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

/// Validate a `%TAG` prefix string against the `ns-uri-char` alphabet.
///
/// Productions:
///   `[93] ns-tag-prefix       ::= c-ns-local-tag-prefix | ns-global-tag-prefix`
///   `[94] c-ns-local-tag-prefix ::= "!" ns-uri-char*`
///   `[95] ns-global-tag-prefix ::= ns-tag-char ns-uri-char*`
///
/// Returns `Ok(())` on success, or `Err(byte_offset)` where `byte_offset` is
/// the position of the first invalid byte within `prefix`.
fn validate_tag_prefix(prefix: &str) -> Result<(), usize> {
    let bytes = prefix.as_bytes();

    // Determine starting offset based on prefix type.
    // Local prefix starts with `!` (c-tag); global prefix starts with ns-tag-char.
    let start = if bytes.first() == Some(&b'!') {
        1usize // `!` is the c-tag indicator; advance past it
    } else {
        let ch = prefix.chars().next().unwrap_or('\0');
        if !is_ns_tag_char_single(ch) {
            return Err(0);
        }
        ch.len_utf8()
    };

    // All remaining bytes must be ns-uri-char or valid %HH percent-encoded sequences.
    let mut pos = start;
    while pos < bytes.len() {
        // Use .get() to satisfy the indexing_slicing lint; the while guard
        // ensures pos < bytes.len(), so unwrap_or is unreachable in practice.
        if bytes.get(pos).copied() == Some(b'%') {
            let h1 = bytes
                .get(pos + 1)
                .copied()
                .is_some_and(|b| b.is_ascii_hexdigit());
            let h2 = bytes
                .get(pos + 2)
                .copied()
                .is_some_and(|b| b.is_ascii_hexdigit());
            if h1 && h2 {
                pos += 3;
                continue;
            }
            return Err(pos);
        }
        let ch = prefix[pos..].chars().next().unwrap_or('\0');
        if !is_ns_uri_char_single(ch) {
            return Err(pos);
        }
        pos += ch.len_utf8();
    }
    Ok(())
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "test code")]
mod tests {
    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    /// Collect the first `Err` event from the event stream, panicking if none.
    fn first_err(input: &str) -> crate::Error {
        crate::parse_events(input)
            .find_map(Result::err)
            .expect("expected an error in the event stream")
    }

    // -------------------------------------------------------------------------
    // Class 1: %YAML major != 1 — error points to the major digit
    // -------------------------------------------------------------------------

    // 1a. Major 2 — single-digit, input `%YAML 2.1`
    // Byte layout: %=0 Y=1 A=2 M=3 L=4 ' '=5 2=6  →  error at byte 6, col 6.
    #[test]
    fn yaml_directive_unsupported_major_2_pos_is_major_digit() {
        let err = first_err("%YAML 2.1\n");
        assert_eq!(err.pos.byte_offset, 6, "byte_offset");
        assert_eq!(err.pos.line, 1, "line");
        assert_eq!(err.pos.column, 6, "column");
        assert!(
            err.message.contains("2.1"),
            "message should mention version"
        );
    }

    // 1b. Major 0.
    #[test]
    fn yaml_directive_unsupported_major_0_pos_is_major_digit() {
        let err = first_err("%YAML 0.1\n");
        assert_eq!(err.pos.byte_offset, 6, "byte_offset");
        assert_eq!(err.pos.line, 1, "line");
        assert_eq!(err.pos.column, 6, "column");
    }

    // 1c. Major 3.
    #[test]
    fn yaml_directive_unsupported_major_3_pos_is_major_digit() {
        let err = first_err("%YAML 3.2\n");
        assert_eq!(err.pos.byte_offset, 6, "byte_offset");
        assert_eq!(err.pos.line, 1, "line");
        assert_eq!(err.pos.column, 6, "column");
    }

    // 1d. Non-zero line: directive on second line.
    // Input: "\n%YAML 2.1\n" — `%` at byte 1, line 2, col 0.
    // Major at byte 1+6=7, line 2, col 6.
    #[test]
    fn yaml_directive_unsupported_major_non_zero_offset() {
        let err = first_err("\n%YAML 2.1\n");
        assert_eq!(err.pos.byte_offset, 7, "byte_offset");
        assert_eq!(err.pos.line, 2, "line");
        assert_eq!(err.pos.column, 6, "column");
    }

    // -------------------------------------------------------------------------
    // Class 2: %YAML version digit overflows u8 — error points to first digit
    // -------------------------------------------------------------------------

    // 2a. Major 256 overflow — first digit of "256" is at byte 6.
    #[test]
    fn yaml_directive_major_overflow_pos_is_first_digit_of_major() {
        let err = first_err("%YAML 256.0\n");
        assert_eq!(err.pos.byte_offset, 6, "byte_offset");
        assert_eq!(err.pos.line, 1, "line");
        assert_eq!(err.pos.column, 6, "column");
        assert!(
            err.message.contains("major"),
            "message should mention major"
        );
    }

    // 2b. Minor 256 overflow — dot is at byte 7, minor "2" is at byte 8.
    #[test]
    fn yaml_directive_minor_overflow_pos_is_first_digit_of_minor() {
        let err = first_err("%YAML 1.256\n");
        assert_eq!(err.pos.byte_offset, 8, "byte_offset");
        assert_eq!(err.pos.line, 1, "line");
        assert_eq!(err.pos.column, 8, "column");
        assert!(
            err.message.contains("minor"),
            "message should mention minor"
        );
    }

    // 2c. Major overflow on non-first line.
    // Input: "\n%YAML 256.0\n" — `%` at byte 1, line 2, col 0; major "2" at byte 7, col 6.
    #[test]
    fn yaml_directive_major_overflow_non_zero_offset() {
        let err = first_err("\n%YAML 256.0\n");
        assert_eq!(err.pos.byte_offset, 7, "byte_offset");
        assert_eq!(err.pos.line, 2, "line");
        assert_eq!(err.pos.column, 6, "column");
    }
}
