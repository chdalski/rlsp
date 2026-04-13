// SPDX-License-Identifier: MIT

use crate::pos::Pos;

/// A parse error produced by the streaming parser.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("parse error at {pos:?}: {message}")]
pub struct Error {
    /// Source position where the parse error was detected.
    pub pos: Pos,
    /// Human-readable description of the error.
    pub message: String,
}
