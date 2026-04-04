// SPDX-License-Identifier: MIT

use crate::pos::Pos;

/// Token codes corresponding to the YAML 1.2 token vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    BeginMapping,
    EndMapping,
    BeginSequence,
    EndSequence,
    BeginScalar,
    EndScalar,
    BeginComment,
    EndComment,
    BeginAnchor,
    EndAnchor,
    BeginAlias,
    EndAlias,
    BeginTag,
    EndTag,
    BeginDocument,
    EndDocument,
    BeginNode,
    EndNode,
    BeginPair,
    EndPair,
    DirectivesEnd,
    DocumentEnd,
    Text,
    Indicator,
    Meta,
    LineFeed,
    LineFold,
    White,
    Indent,
    Break,
    Error,
}

/// A single token emitted by the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'input> {
    pub code: Code,
    pub pos: Pos,
    pub text: &'input str,
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn token_carries_code_and_pos() {
        let pos = Pos {
            byte_offset: 5,
            char_offset: 5,
            line: 2,
            column: 3,
        };
        let token = Token {
            code: Code::BeginMapping,
            pos,
            text: "{",
        };

        assert_eq!(token.code, Code::BeginMapping);
        assert_eq!(token.pos, pos);
    }

    #[test]
    fn code_variants_are_distinct() {
        assert_ne!(Code::BeginMapping, Code::EndMapping);
        assert_ne!(Code::Text, Code::Indicator);
        assert_ne!(Code::BeginScalar, Code::EndScalar);
        assert_ne!(Code::BeginDocument, Code::EndDocument);
    }

    #[test]
    fn code_enum_is_exhaustively_matchable() {
        // Compile-time check: if a variant is added without updating this
        // match, the test fails to compile.
        let code = Code::BeginMapping;
        let _ = match code {
            Code::BeginMapping => 0,
            Code::EndMapping => 1,
            Code::BeginSequence => 2,
            Code::EndSequence => 3,
            Code::BeginScalar => 4,
            Code::EndScalar => 5,
            Code::BeginComment => 6,
            Code::EndComment => 7,
            Code::BeginAnchor => 8,
            Code::EndAnchor => 9,
            Code::BeginAlias => 10,
            Code::EndAlias => 11,
            Code::BeginTag => 12,
            Code::EndTag => 13,
            Code::BeginDocument => 14,
            Code::EndDocument => 15,
            Code::BeginNode => 16,
            Code::EndNode => 17,
            Code::BeginPair => 18,
            Code::EndPair => 19,
            Code::DirectivesEnd => 20,
            Code::DocumentEnd => 21,
            Code::Text => 22,
            Code::Indicator => 23,
            Code::Meta => 24,
            Code::LineFeed => 25,
            Code::LineFold => 26,
            Code::White => 27,
            Code::Indent => 28,
            Code::Break => 29,
            Code::Error => 30,
        };
    }

    #[test]
    fn token_is_debug_formattable() {
        let token = Token {
            code: Code::Text,
            pos: Pos::ORIGIN,
            text: "hello",
        };
        let s = format!("{token:?}");
        assert!(!s.is_empty());
    }
}
