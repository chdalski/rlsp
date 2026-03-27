// SPDX-License-Identifier: MIT

//! A fast, lightweight YAML language server implementing the
//! [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).
//!
//! This crate provides the library modules used by the `rlsp-yaml` binary.
//! See the [repository](https://github.com/chdalski/rlsp) for usage and configuration.

pub mod code_actions;
pub mod code_lens;
pub mod completion;
pub mod document_links;
pub mod document_store;
pub mod folding;
pub mod formatter;
pub mod hover;
pub mod on_type_formatting;
pub mod parser;
pub mod references;
pub mod rename;
pub mod schema;
pub mod schema_validation;
pub mod selection;
pub mod semantic_tokens;
pub mod server;
pub mod symbols;
pub mod validators;
