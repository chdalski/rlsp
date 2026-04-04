// SPDX-License-Identifier: MIT

//! `rlsp-yaml-parser` — a spec-faithful YAML 1.2 parser.
//!
//! This crate implements the full YAML 1.2 grammar by transliterating each of
//! the 211 formal productions from the spec into a parser combinator function.
//! Comments and spans are first-class data.

pub mod combinator;
pub mod pos;
pub mod token;
