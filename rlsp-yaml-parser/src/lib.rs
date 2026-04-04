// SPDX-License-Identifier: MIT

//! `rlsp-yaml-parser` — a spec-faithful YAML 1.2 parser.
//!
//! This crate implements the full YAML 1.2 grammar by transliterating each of
//! the 211 formal productions from the spec into a parser combinator function.
//! Comments and spans are first-class data.

pub mod block;
pub mod chars;
pub mod combinator;
pub mod encoding;
pub mod event;
pub mod flow;
pub mod loader;
pub mod node;
pub mod pos;
pub mod schema;
pub mod stream;
pub mod structure;
pub mod token;

pub use event::parse_events;
pub use loader::load;
pub use stream::tokenize;
