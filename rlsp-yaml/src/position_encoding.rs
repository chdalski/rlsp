// SPDX-License-Identifier: MIT

//! Conversion of positions between the columns the features count and the
//! columns the client counts.
//!
//! The features of this server compute and read **internal positions**: the
//! column counts code points, as `rlsp_yaml_parser::LineIndex` does. The
//! server names no `positionEncoding` in its initialize result, so a client
//! counts the column of a **client position** in UTF-16 code units (LSP 3.17,
//! "Position" and `PositionEncodingKind`). The two differ on a line that holds
//! an astral character, one outside the Basic Multilingual Plane: one code
//! point, two UTF-16 code units. The handlers in `server.rs` convert at the
//! boundary with a [`PositionConverter`] and the [`ClientPositions`] trait.

use tower_lsp::lsp_types::{
    AnnotatedTextEdit, CodeAction, CodeActionContext, CodeActionParams, Diagnostic,
    DiagnosticRelatedInformation, DocumentChangeOperation, DocumentChanges, Location, OneOf,
    Position, Range, TextDocumentEdit, TextEdit, WorkspaceEdit,
};

/// An astral character of the text, with what a column after it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Astral {
    line: u32,
    /// The internal column of the character.
    column: u32,
    /// The astral characters of `line` up to and including this one: the
    /// columns after this character are that many units wider for the client.
    widened_by: u32,
}

impl Astral {
    /// The client column the character starts at; it spans two units.
    const fn client_column(self) -> u32 {
        self.column
            .saturating_add(self.widened_by.saturating_sub(1))
    }
}

/// Maps a position of one text between internal columns (code points) and
/// client columns (UTF-16 code units), in both directions.
///
/// It records only the astral characters of the text, so a text without any
/// holds nothing and every position converts to itself. A conversion takes
/// time logarithmic in the number of astral characters.
///
/// Lines are numbered as `LineIndex` numbers them: a line ends at `\n`, `\r\n`,
/// or a lone `\r`, and the line of a position converts to itself. A position
/// on a line past the last one, or in an empty text, converts to itself.
///
/// Edge positions, all of which a client can send:
/// - A client column inside an astral character converts to the internal
///   column of that character, so that a request there finds the character
///   (LSP 3.17 leaves such a column undefined).
/// - A column past the end of its line keeps its excess over the line end in
///   both directions, so a round trip is exact.
/// - Arithmetic saturates: the internal column `u32::MAX`, which syntax
///   diagnostics end at, is the client column `u32::MAX`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PositionConverter {
    /// The astral characters of the text in text order, so sorted by line and
    /// column, and by the client column within a line.
    astral: Vec<Astral>,
}

impl PositionConverter {
    /// Build the converter of `text` with one scan of its bytes.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut astral = Vec::new();
        let (mut line, mut column, mut widened_by) = (0_u32, 0_u32, 0_u32);
        let mut bytes = text.bytes().peekable();
        while let Some(byte) = bytes.next() {
            match byte {
                b'\n' | b'\r' => {
                    if byte == b'\r' {
                        bytes.next_if_eq(&b'\n');
                    }
                    line = line.saturating_add(1);
                    column = 0;
                    widened_by = 0;
                }
                // A continuation byte belongs to a character that was counted
                // at its first byte.
                0x80..=0xBF => {}
                // The first byte of a four-byte sequence: a character from
                // U+10000 on.
                0xF0.. => {
                    widened_by = widened_by.saturating_add(1);
                    astral.push(Astral {
                        line,
                        column,
                        widened_by,
                    });
                    column = column.saturating_add(1);
                }
                _ => column = column.saturating_add(1),
            }
        }
        Self { astral }
    }

    /// The client position of an internal position.
    #[must_use]
    pub fn to_client(&self, position: Position) -> Position {
        // An astral character that starts before the column widens it.
        let widened_by = self
            .astral
            .partition_point(|a| (a.line, a.column) < (position.line, position.character))
            .checked_sub(1)
            .and_then(|last| self.astral.get(last))
            .filter(|last| last.line == position.line)
            .map_or(0, |last| last.widened_by);
        Position::new(position.line, position.character.saturating_add(widened_by))
    }

    /// The internal position of a client position.
    #[must_use]
    pub fn to_internal(&self, position: Position) -> Position {
        // The last astral character that starts at or before the column.
        let last = self
            .astral
            .partition_point(|a| (a.line, a.client_column()) <= (position.line, position.character))
            .checked_sub(1)
            .and_then(|last| self.astral.get(last))
            .filter(|last| last.line == position.line);
        let column = match last {
            // At or inside the two units of the character: its start.
            Some(last) if position.character <= last.client_column().saturating_add(1) => {
                last.column
            }
            Some(last) => position.character.saturating_sub(last.widened_by),
            None => position.character,
        };
        Position::new(position.line, column)
    }
}

/// A value of a request or response that holds positions, which the boundary of
/// the server converts as a whole.
///
/// Every implementation names all fields of its struct, so a field that a new
/// version of `lsp-types` adds fails to compile until it is handled here.
pub trait ClientPositions: Sized {
    /// `self` with every position replaced by `convert` of it.
    #[must_use]
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self;

    /// `self` with the internal positions in it as client positions.
    #[must_use]
    fn to_client(self, converter: &PositionConverter) -> Self {
        self.map_positions(&|position| converter.to_client(position))
    }

    /// `self` with the client positions in it as internal positions.
    #[must_use]
    fn to_internal(self, converter: &PositionConverter) -> Self {
        self.map_positions(&|position| converter.to_internal(position))
    }
}

impl ClientPositions for Position {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        convert(self)
    }
}

impl ClientPositions for Range {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        let Self { start, end } = self;
        Self::new(convert(start), convert(end))
    }
}

impl<T: ClientPositions> ClientPositions for Option<T> {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        self.map(|value| value.map_positions(convert))
    }
}

impl<T: ClientPositions> ClientPositions for Vec<T> {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        self.into_iter()
            .map(|value| value.map_positions(convert))
            .collect()
    }
}

impl<L: ClientPositions, R: ClientPositions> ClientPositions for OneOf<L, R> {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        match self {
            Self::Left(left) => Self::Left(left.map_positions(convert)),
            Self::Right(right) => Self::Right(right.map_positions(convert)),
        }
    }
}

impl ClientPositions for TextEdit {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        let Self { range, new_text } = self;
        Self {
            range: range.map_positions(convert),
            new_text,
        }
    }
}

impl ClientPositions for AnnotatedTextEdit {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        let Self {
            text_edit,
            annotation_id,
        } = self;
        Self {
            text_edit: text_edit.map_positions(convert),
            annotation_id,
        }
    }
}

impl ClientPositions for TextDocumentEdit {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        let Self {
            text_document,
            edits,
        } = self;
        Self {
            text_document,
            edits: edits.map_positions(convert),
        }
    }
}

impl ClientPositions for DocumentChangeOperation {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        match self {
            // Creating, renaming, and deleting a file name no position.
            Self::Op(operation) => Self::Op(operation),
            Self::Edit(edit) => Self::Edit(edit.map_positions(convert)),
        }
    }
}

impl ClientPositions for DocumentChanges {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        match self {
            Self::Edits(edits) => Self::Edits(edits.map_positions(convert)),
            Self::Operations(operations) => Self::Operations(operations.map_positions(convert)),
        }
    }
}

/// The edits of every document of the map are converted with the converter of
/// one text, since the server edits the document of the request only.
impl ClientPositions for WorkspaceEdit {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        let Self {
            changes,
            document_changes,
            change_annotations,
        } = self;
        Self {
            changes: changes.map(|changes| {
                changes
                    .into_iter()
                    .map(|(uri, edits)| (uri, edits.map_positions(convert)))
                    .collect()
            }),
            document_changes: document_changes.map_positions(convert),
            change_annotations,
        }
    }
}

impl ClientPositions for Location {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        let Self { uri, range } = self;
        Self {
            uri,
            range: range.map_positions(convert),
        }
    }
}

impl ClientPositions for DiagnosticRelatedInformation {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        let Self { location, message } = self;
        Self {
            location: location.map_positions(convert),
            message,
        }
    }
}

/// The locations of the related information are converted with the converter
/// of one text, since the server points them into the document of the
/// diagnostic only.
impl ClientPositions for Diagnostic {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        let Self {
            range,
            severity,
            code,
            code_description,
            source,
            message,
            related_information,
            tags,
            data,
        } = self;
        Self {
            range: range.map_positions(convert),
            severity,
            code,
            code_description,
            source,
            message,
            related_information: related_information.map_positions(convert),
            tags,
            data,
        }
    }
}

impl ClientPositions for CodeAction {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        let Self {
            title,
            kind,
            diagnostics,
            edit,
            command,
            is_preferred,
            disabled,
            data,
        } = self;
        Self {
            title,
            kind,
            diagnostics: diagnostics.map_positions(convert),
            edit: edit.map_positions(convert),
            command,
            is_preferred,
            disabled,
            data,
        }
    }
}

impl ClientPositions for CodeActionContext {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        let Self {
            diagnostics,
            only,
            trigger_kind,
        } = self;
        Self {
            diagnostics: diagnostics.map_positions(convert),
            only,
            trigger_kind,
        }
    }
}

impl ClientPositions for CodeActionParams {
    fn map_positions(self, convert: &dyn Fn(Position) -> Position) -> Self {
        let Self {
            text_document,
            range,
            context,
            work_done_progress_params,
            partial_result_params,
        } = self;
        Self {
            text_document,
            range: range.map_positions(convert),
            context: context.map_positions(convert),
            work_done_progress_params,
            partial_result_params,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::Duration;

    use proptest::prelude::*;
    use rstest::rstest;
    use serde_json::{Value, json};
    use tower_lsp::lsp_types::{
        CodeActionContext, Command, CreateFile, DeleteFile, DiagnosticSeverity, NumberOrString,
        OptionalVersionedTextDocumentIdentifier, PartialResultParams, ResourceOp,
        TextDocumentIdentifier, Url, WorkDoneProgressParams,
    };

    use super::*;

    const ASTRAL: &str = "\u{1f600}";

    /// The lines of `text` without their breaks, found by an own scan so the
    /// tests do not lean on the code they check: a line ends at `\n`, `\r\n`,
    /// or a lone `\r`.
    fn lines_of(text: &str) -> Vec<String> {
        let mut lines = vec![String::new()];
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\n' => lines.push(String::new()),
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    lines.push(String::new());
                }
                _ => lines.last_mut().expect("a line").push(c),
            }
        }
        lines
    }

    fn utf16_len(text: &str) -> u32 {
        u32::try_from(text.encode_utf16().count()).expect("fits")
    }

    fn at(line: u32, character: u32) -> Position {
        Position::new(line, character)
    }

    // ---- known positions ----

    #[rstest]
    #[case::ascii("abc\n", at(0, 2), at(0, 2))]
    #[case::at_an_astral_character("A", at(0, 0), at(0, 0))]
    #[case::after_an_astral_character("A", at(0, 1), at(0, 2))]
    #[case::after_two_astral_characters("AA", at(0, 2), at(0, 4))]
    #[case::between_two_astral_characters("AA", at(0, 1), at(0, 2))]
    #[case::mixed_widths("aA\u{e9}b", at(0, 3), at(0, 4))]
    #[case::end_of_a_mixed_line("aA\u{e9}b", at(0, 4), at(0, 5))]
    #[case::three_byte_character_before_an_astral_one("\u{540d}A", at(0, 2), at(0, 3))]
    #[case::largest_bmp_character_is_one_unit("\u{ffff}x", at(0, 1), at(0, 1))]
    #[case::smallest_astral_character("\u{10000}x", at(0, 1), at(0, 2))]
    #[case::largest_astral_character("\u{10ffff}x", at(0, 1), at(0, 2))]
    #[case::line_without_one_between_two_that_have_one("A\nb\nA", at(1, 1), at(1, 1))]
    #[case::line_after_one_that_has_one("A\nb\nA", at(2, 1), at(2, 2))]
    #[case::crlf_is_one_break("A\r\nA", at(1, 1), at(1, 2))]
    #[case::lone_cr_is_a_break("A\rA", at(1, 1), at(1, 2))]
    #[case::cr_before_crlf("A\r\r\nA", at(2, 1), at(2, 2))]
    #[case::empty_line_between_cr_and_crlf("A\r\r\nA", at(1, 0), at(1, 0))]
    #[case::leading_breaks("\n\nA", at(2, 1), at(2, 2))]
    #[case::astral_character_before_a_cr("A\rb", at(0, 1), at(0, 2))]
    #[case::astral_character_at_the_end_of_the_text("ab\nA", at(1, 1), at(1, 2))]
    #[case::line_after_the_final_break("A\n", at(1, 3), at(1, 3))]
    #[case::end_of_a_line_with_the_break("A\r\nA", at(0, 5), at(0, 6))]
    #[case::byte_order_mark_is_one_unit("\u{feff}A", at(0, 1), at(0, 1))]
    #[case::after_a_byte_order_mark("\u{feff}A", at(0, 2), at(0, 3))]
    #[case::past_the_end_of_the_line("aA", at(0, 5), at(0, 6))]
    #[case::line_past_the_last("A", at(5, 7), at(5, 7))]
    #[case::empty_text("", at(2, 9), at(2, 9))]
    #[case::the_maximum_line("A", at(u32::MAX, 1), at(u32::MAX, 1))]
    fn an_internal_position_converts_to_the_client_position_and_back(
        #[case] text: &str,
        #[case] internal: Position,
        #[case] client: Position,
    ) {
        let converter = PositionConverter::new(&text.replace('A', ASTRAL));

        assert_eq!(converter.to_client(internal), client);
        assert_eq!(converter.to_internal(client), internal);
    }

    #[test]
    fn lines_between_two_lines_with_astral_characters_are_left_alone() {
        let converter = PositionConverter::new(&format!("{ASTRAL}{}{ASTRAL}", "\n".repeat(7)));

        for line in 1..7 {
            assert_eq!(converter.to_client(at(line, 3)), at(line, 3), "{line}");
            assert_eq!(converter.to_internal(at(line, 3)), at(line, 3), "{line}");
        }
        assert_eq!(converter.to_client(at(7, 1)), at(7, 2));
    }

    #[rstest]
    #[case::at_the_start_of_the_character(at(0, 1), at(0, 1))]
    #[case::inside_the_character(at(0, 2), at(0, 1))]
    #[case::after_the_character(at(0, 3), at(0, 2))]
    #[case::inside_the_second_character(at(0, 4), at(0, 2))]
    #[case::after_the_second_character(at(0, 5), at(0, 3))]
    fn a_client_column_inside_an_astral_character_converts_to_its_start(
        #[case] client: Position,
        #[case] internal: Position,
    ) {
        let converter = PositionConverter::new(&format!("a{ASTRAL}{ASTRAL}"));

        assert_eq!(converter.to_internal(client), internal);
    }

    #[rstest]
    #[case::first_column(at(0, 1), at(0, 0))]
    #[case::last_character_of_the_line(at(0, 4), at(0, 2))]
    #[case::second_line(at(1, 2), at(1, 1))]
    fn a_client_column_inside_a_character_on_any_line_converts_to_its_start(
        #[case] client: Position,
        #[case] internal: Position,
    ) {
        let converter = PositionConverter::new(&format!("{ASTRAL}b{ASTRAL}\nb{ASTRAL}"));

        assert_eq!(converter.to_internal(client), internal);
    }

    // ---- the largest columns ----

    #[test]
    fn the_maximum_internal_column_is_the_maximum_client_column() {
        let converter = PositionConverter::new(&format!("a{ASTRAL}"));

        assert_eq!(converter.to_client(at(0, u32::MAX)), at(0, u32::MAX));
        assert_eq!(converter.to_client(at(0, u32::MAX - 1)), at(0, u32::MAX));
        assert_eq!(
            converter.to_client(at(0, u32::MAX - 2)),
            at(0, u32::MAX - 1)
        );
    }

    #[test]
    fn the_maximum_client_column_stays_past_the_end_of_its_line() {
        let converter = PositionConverter::new(&format!("{ASTRAL}{ASTRAL}\n"));

        let internal = converter.to_internal(at(0, u32::MAX));

        assert_eq!(internal, at(0, u32::MAX - 2));
        assert_eq!(converter.to_client(internal), at(0, u32::MAX));
    }

    #[test]
    fn the_maximum_columns_of_a_text_without_an_astral_character_convert_to_themselves() {
        let converter = PositionConverter::new("a\n\u{e9}");

        assert_eq!(converter.to_client(at(1, u32::MAX)), at(1, u32::MAX));
        assert_eq!(converter.to_internal(at(1, u32::MAX)), at(1, u32::MAX));
    }

    #[test]
    fn a_column_next_to_the_maximum_does_not_overflow_on_a_line_with_many_astral_characters() {
        let converter = PositionConverter::new(&ASTRAL.repeat(50));

        for column in [u32::MAX - 50, u32::MAX - 1, u32::MAX] {
            assert_eq!(converter.to_client(at(0, column)).line, 0);
            assert_eq!(converter.to_internal(at(0, column)).line, 0);
        }
        assert_eq!(
            converter.to_client(at(0, u32::MAX - 50)).character,
            u32::MAX
        );
        assert_eq!(
            converter.to_internal(at(0, u32::MAX)).character,
            u32::MAX - 50
        );
    }

    // ---- what the converter holds ----

    #[test]
    fn a_text_without_an_astral_character_records_nothing() {
        let converter = PositionConverter::new(&"a\n\u{e9}\u{feff}\r\nb\rc\u{540d}".repeat(100));

        assert_eq!(converter.astral.capacity(), 0);
        assert_eq!(converter, PositionConverter::default());
    }

    #[test]
    fn the_converter_holds_one_entry_per_astral_character_whatever_the_lines() {
        let text = format!("{}{ASTRAL}\n{ASTRAL}{ASTRAL}", "\n".repeat(10_000));

        assert_eq!(PositionConverter::new(&text).astral.len(), 3);
    }

    // ---- against an independent count ----

    fn text_of(characters: &[char]) -> String {
        characters.iter().collect()
    }

    fn characters(astral: bool) -> impl Strategy<Value = Vec<char>> {
        let mut alphabet = vec!['a', '\u{e9}', '\u{540d}', '\r', '\n'];
        if astral {
            alphabet.push('\u{1f600}');
            alphabet.push('\u{10ffff}');
        }
        prop::collection::vec(prop::sample::select(alphabet), 0..40)
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 2048, ..ProptestConfig::default() })]

        #[test]
        fn the_client_column_is_the_utf16_length_of_the_line_before_it(
            characters in characters(true),
        ) {
            let text = text_of(&characters);
            let converter = PositionConverter::new(&text);
            for (line, content) in lines_of(&text).iter().enumerate() {
                let line = u32::try_from(line).expect("fits");
                let chars: Vec<char> = content.chars().collect();
                for column in 0..=chars.len() + 5 {
                    let before: String = chars.iter().take(column).collect();
                    let excess = u32::try_from(column.saturating_sub(chars.len())).expect("fits");
                    let internal = at(line, u32::try_from(column).expect("fits"));
                    prop_assert_eq!(
                        converter.to_client(internal),
                        at(line, utf16_len(&before) + excess),
                        "{:?} at {:?}", text, internal
                    );
                }
            }
        }

        #[test]
        fn converting_to_the_client_and_back_returns_the_position(
            characters in characters(true),
            line in 0_u32..44,
            column in 0_u32..46,
        ) {
            let converter = PositionConverter::new(&text_of(&characters));
            let internal = at(line, column);

            prop_assert_eq!(converter.to_internal(converter.to_client(internal)), internal);
        }

        #[test]
        fn a_client_column_converts_back_to_itself_unless_it_is_inside_an_astral_character(
            characters in characters(true),
        ) {
            let text = text_of(&characters);
            let converter = PositionConverter::new(&text);
            for (line, content) in lines_of(&text).iter().enumerate() {
                let line = u32::try_from(line).expect("fits");
                // The client columns at which a character of the line starts.
                let mut starts = Vec::new();
                let mut units = 0;
                for c in content.chars() {
                    starts.push((units, c.len_utf16()));
                    units += u32::try_from(c.len_utf16()).expect("fits");
                }
                for column in 0..=units + 5 {
                    let client = at(line, column);
                    let internal = converter.to_internal(client);
                    let inside = starts
                        .iter()
                        .position(|&(start, width)| width == 2 && start + 1 == column);
                    if let Some(index) = inside {
                        prop_assert_eq!(
                            internal,
                            at(line, u32::try_from(index).expect("fits")),
                            "{:?} at {:?}", text, client
                        );
                    } else {
                        prop_assert_eq!(
                            converter.to_client(internal),
                            client,
                            "{:?} at {:?}", text, client
                        );
                    }
                }
            }
        }

        #[test]
        fn both_conversions_keep_the_order_of_columns(
            characters in characters(true),
            line in 0_u32..44,
            a in 0_u32..60,
            b in 0_u32..60,
        ) {
            let converter = PositionConverter::new(&text_of(&characters));
            let (low, high) = (a.min(b), a.max(b));

            prop_assert!(
                converter.to_client(at(line, low)).character
                    <= converter.to_client(at(line, high)).character
            );
            prop_assert!(
                converter.to_internal(at(line, low)).character
                    <= converter.to_internal(at(line, high)).character
            );
        }

        #[test]
        fn a_text_without_an_astral_character_converts_every_position_to_itself(
            characters in characters(false),
            line in prop::num::u32::ANY,
            column in prop::num::u32::ANY,
        ) {
            let converter = PositionConverter::new(&text_of(&characters));
            let given = at(line, column);

            prop_assert_eq!(converter.to_client(given), given);
            prop_assert_eq!(converter.to_internal(given), given);
        }

        #[test]
        fn no_position_makes_a_conversion_panic_or_change_the_line(
            characters in characters(true),
            line in prop::num::u32::ANY,
            column in prop::num::u32::ANY,
        ) {
            let converter = PositionConverter::new(&text_of(&characters));
            let given = at(line, column);

            prop_assert_eq!(converter.to_client(given).line, line);
            prop_assert_eq!(converter.to_internal(given).line, line);
        }

        #[test]
        fn the_lines_are_the_lines_of_the_line_index(characters in characters(true)) {
            let text = text_of(&characters);
            let converter = PositionConverter::new(&text);
            let index = rlsp_yaml_parser::LineIndex::new(&text);
            let lines = lines_of(&text);
            for (offset, _) in text.char_indices() {
                let (line, column) = index.line_column(u32::try_from(offset).expect("fits"));
                let internal = at(line - 1, column);
                let content: Vec<char> = lines[(line - 1) as usize].chars().collect();
                let before: String = content.iter().take(column as usize).collect();
                prop_assert_eq!(
                    converter.to_client(internal),
                    at(line - 1, utf16_len(&before)),
                    "{:?} at offset {}", text, offset
                );
            }
        }
    }

    #[rstest]
    #[timeout(Duration::from_secs(10))]
    fn a_long_line_converts_every_position_correctly() {
        let line: String = (0..100_000)
            .map(|i| if i % 2 == 0 { ASTRAL } else { "a" })
            .collect();
        let converter = PositionConverter::new(&line);

        // Character `i` starts at `i` internally and at `i + ceil(i / 2)` for
        // the client: every other character before it is astral.
        for i in 0..=100_000_u32 {
            let client = i + i.div_ceil(2);
            assert_eq!(converter.to_client(at(0, i)), at(0, client), "{i}");
            assert_eq!(converter.to_internal(at(0, client)), at(0, i), "{i}");
        }
    }

    // ---- the values of requests and responses ----

    fn uri() -> Url {
        Url::parse("file:///test.yaml").expect("a URL")
    }

    /// A converter for a first line of astral characters: every internal
    /// column from 1 on is a different client column.
    fn converter() -> PositionConverter {
        PositionConverter::new(&ASTRAL.repeat(200))
    }

    /// Hands out positions of one line with a new column each time, always at
    /// the start of a character, so that they convert back to themselves.
    struct Positions(Cell<u32>);

    impl Positions {
        fn next(&self) -> Position {
            self.0.set(self.0.get() + 2);
            at(0, self.0.get())
        }

        fn range(&self) -> Range {
            Range::new(self.next(), self.next())
        }

        fn edit(&self) -> TextEdit {
            TextEdit::new(self.range(), "t".to_string())
        }

        fn text_document_edit(&self) -> TextDocumentEdit {
            TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier {
                    uri: uri(),
                    version: Some(1),
                },
                edits: vec![
                    OneOf::Left(self.edit()),
                    OneOf::Right(AnnotatedTextEdit {
                        text_edit: self.edit(),
                        annotation_id: "a".to_string(),
                    }),
                ],
            }
        }

        fn diagnostic(&self) -> Diagnostic {
            Diagnostic {
                range: self.range(),
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String("code".to_string())),
                code_description: None,
                source: Some("s".to_string()),
                message: "m".to_string(),
                related_information: Some(vec![
                    DiagnosticRelatedInformation {
                        location: Location::new(uri(), self.range()),
                        message: "r".to_string(),
                    },
                    DiagnosticRelatedInformation {
                        location: Location::new(uri(), self.range()),
                        message: "r".to_string(),
                    },
                ]),
                tags: None,
                data: Some(json!({ "line": 1, "character": 5 })),
            }
        }

        fn workspace_edit(&self, operations: bool) -> WorkspaceEdit {
            let document_changes = if operations {
                DocumentChanges::Operations(vec![
                    DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                        uri: uri(),
                        options: None,
                        annotation_id: None,
                    })),
                    DocumentChangeOperation::Edit(self.text_document_edit()),
                    DocumentChangeOperation::Op(ResourceOp::Delete(DeleteFile {
                        uri: uri(),
                        options: None,
                    })),
                ])
            } else {
                DocumentChanges::Edits(vec![self.text_document_edit(), self.text_document_edit()])
            };
            WorkspaceEdit {
                changes: Some(std::iter::once((uri(), vec![self.edit(), self.edit()])).collect()),
                document_changes: Some(document_changes),
                change_annotations: None,
            }
        }

        fn code_action(&self, operations: bool) -> CodeAction {
            CodeAction {
                title: "t".to_string(),
                kind: None,
                diagnostics: Some(vec![self.diagnostic(), self.diagnostic()]),
                edit: Some(self.workspace_edit(operations)),
                command: Some(Command {
                    title: "c".to_string(),
                    command: "c".to_string(),
                    arguments: Some(vec![json!({ "line": 2, "character": 7 })]),
                }),
                is_preferred: Some(true),
                disabled: None,
                data: Some(json!({ "line": 3, "character": 9 })),
            }
        }

        fn code_action_params(&self) -> CodeActionParams {
            CodeActionParams {
                text_document: TextDocumentIdentifier { uri: uri() },
                range: self.range(),
                context: CodeActionContext {
                    diagnostics: vec![self.diagnostic(), self.diagnostic()],
                    only: None,
                    trigger_kind: None,
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            }
        }
    }

    /// The positions of the JSON of a value in document order, as pairs of
    /// line and character, leaving out the opaque `data` and `arguments` of
    /// diagnostics, actions, and commands.
    fn positions_in(value: &Value, found: &mut Vec<(u64, u64)>) {
        match value {
            Value::Object(map) => {
                if let (Some(line), Some(character)) = (
                    map.get("line").and_then(Value::as_u64),
                    map.get("character").and_then(Value::as_u64),
                ) {
                    found.push((line, character));
                }
                for (key, child) in map {
                    if key != "data" && key != "arguments" {
                        positions_in(child, found);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| positions_in(item, found)),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }

    /// Check that `convert` replaces each position of `value` by the
    /// conversion of it, none is missed, and `back` returns the value.
    fn assert_every_position_converted<T>(
        value: &T,
        to: impl Fn(T, &PositionConverter) -> T,
        back: impl Fn(T, &PositionConverter) -> T,
        convert: impl Fn(&PositionConverter, Position) -> Position,
    ) where
        T: Clone + PartialEq + std::fmt::Debug + serde::Serialize,
    {
        let converter = converter();
        let before = serde_json::to_value(value).expect("JSON");
        let after_value = to(value.clone(), &converter);
        let after = serde_json::to_value(&after_value).expect("JSON");
        let (mut old, mut new) = (Vec::new(), Vec::new());
        positions_in(&before, &mut old);
        positions_in(&after, &mut new);

        assert!(old.len() >= 8, "the fixture holds positions: {before}");
        assert_eq!(old.len(), new.len(), "{after}");
        for (&(line, character), &found) in old.iter().zip(&new) {
            let expected = convert(
                &converter,
                at(
                    u32::try_from(line).expect("fits"),
                    u32::try_from(character).expect("fits"),
                ),
            );
            assert_ne!(found.1, character, "a position stays: {after}");
            assert_eq!(
                found,
                (u64::from(expected.line), u64::from(expected.character))
            );
        }
        assert_eq!(&back(after_value, &converter), value);
    }

    #[test]
    fn every_position_of_a_code_action_is_converted_in_both_directions() {
        for operations in [false, true] {
            assert_every_position_converted(
                &Positions(Cell::new(0)).code_action(operations),
                ClientPositions::to_client,
                ClientPositions::to_internal,
                PositionConverter::to_client,
            );
        }
    }

    #[test]
    fn every_position_of_code_action_params_is_converted_in_both_directions() {
        assert_every_position_converted(
            &Positions(Cell::new(0)).code_action_params(),
            ClientPositions::to_internal,
            ClientPositions::to_client,
            PositionConverter::to_internal,
        );
    }

    #[test]
    fn every_position_of_the_smaller_values_is_converted() {
        let positions = Positions(Cell::new(0));

        assert_every_position_converted(
            &vec![positions.diagnostic(), positions.diagnostic()],
            ClientPositions::to_client,
            ClientPositions::to_internal,
            PositionConverter::to_client,
        );
        assert_every_position_converted(
            &Some(positions.workspace_edit(false)),
            ClientPositions::to_client,
            ClientPositions::to_internal,
            PositionConverter::to_client,
        );
        assert_every_position_converted(
            &vec![
                positions.range(),
                positions.range(),
                positions.range(),
                positions.range(),
            ],
            ClientPositions::to_client,
            ClientPositions::to_internal,
            PositionConverter::to_client,
        );
        assert_eq!(
            Position::new(0, 1).to_client(&converter()),
            Position::new(0, 2)
        );
    }

    #[test]
    fn opaque_data_and_commands_are_left_alone() {
        let action = Positions(Cell::new(0)).code_action(false);

        let client = action.clone().to_client(&converter());

        assert_eq!(client.data, action.data);
        assert_eq!(client.command, action.command);
        assert_eq!(
            client.diagnostics.expect("diagnostics")[0].data,
            action.diagnostics.expect("diagnostics")[0].data
        );
    }

    #[test]
    fn resource_operations_come_back_as_they_were() {
        let operation = DocumentChangeOperation::Op(ResourceOp::Delete(DeleteFile {
            uri: uri(),
            options: None,
        }));

        assert_eq!(operation.clone().to_client(&converter()), operation);
    }

    #[test]
    fn a_value_without_a_position_stays_as_it_is() {
        let none: Option<Diagnostic> = None;

        assert_eq!(none.to_client(&converter()), None);
        assert_eq!(Vec::<TextEdit>::new().to_client(&converter()), vec![]);
    }
}
