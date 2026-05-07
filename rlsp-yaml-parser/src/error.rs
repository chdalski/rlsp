// SPDX-License-Identifier: MIT

use crate::pos::Pos;

/// The category of a parse error.
///
/// Consumers can match on `kind` to route errors without substring-matching
/// `message` text.  `message` remains the authoritative human-readable description.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A character that is not allowed in the current YAML context was found.
    ///
    /// Produced wherever the parser rejects a non-printable or otherwise
    /// forbidden codepoint — e.g. `U+0001` in a comment body, a `\x07` hex
    /// escape in a double-quoted scalar, or a NUL in a directive parameter.
    InvalidCharacter,
    /// A grammar or structural error that is not caused by a specific forbidden
    /// character.
    ///
    /// Produced for unterminated scalars, bad indentation, duplicate directives,
    /// and all other parse failures.
    Syntax,
}

/// A parse error produced by the streaming parser.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("parse error at {pos:?}: {message}")]
#[non_exhaustive]
pub struct Error {
    /// Source position where the parse error was detected.
    pub pos: Pos,
    /// Human-readable description of the error.
    pub message: String,
    /// Broad category of the error, for routing without message-string matching.
    pub kind: ErrorKind,
}

impl Error {
    /// Construct a [`ErrorKind::Syntax`] error.
    #[must_use]
    pub const fn syntax(pos: Pos, message: String) -> Self {
        Self {
            pos,
            message,
            kind: ErrorKind::Syntax,
        }
    }

    /// Construct a [`ErrorKind::InvalidCharacter`] error.
    #[must_use]
    pub const fn invalid_character(pos: Pos, message: String) -> Self {
        Self {
            pos,
            message,
            kind: ErrorKind::InvalidCharacter,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_pos() -> Pos {
        Pos {
            byte_offset: 0,
            line: 1,
            column: 0,
        }
    }

    #[test]
    fn error_syntax_constructor_sets_kind_syntax() {
        let pos = zero_pos();
        let err = Error::syntax(pos, "msg".to_owned());
        assert_eq!(err.kind, ErrorKind::Syntax);
        assert_eq!(err.message, "msg");
        assert_eq!(err.pos, pos);
    }

    #[test]
    fn error_invalid_character_constructor_sets_kind_invalid_character() {
        let pos = zero_pos();
        let err = Error::invalid_character(pos, "msg".to_owned());
        assert_eq!(err.kind, ErrorKind::InvalidCharacter);
        assert_eq!(err.message, "msg");
        assert_eq!(err.pos, pos);
    }
}
