// SPDX-License-Identifier: MIT

use crate::pos::Pos;

/// A parse error produced by the streaming parser.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("parse error at {pos:?}: {message}")]
pub struct Error {
    pub pos: Pos,
    pub message: String,
}
