// SPDX-License-Identifier: MIT

//! A fast, lightweight YAML language server implementing the
//! [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).
//!
//! This crate provides the library modules used by the `rlsp-yaml` binary.
//! See the [repository](https://github.com/chdalski/rlsp) for usage and configuration.

pub mod analysis;
pub mod completion;
pub mod decorators;
pub mod document_store;
pub mod editing;
pub mod hover;
pub mod navigation;
pub mod parser;
pub mod scalar_helpers;
pub mod schema;
pub mod schema_validation;
pub mod server;
pub mod validation;
