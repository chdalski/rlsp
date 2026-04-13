// SPDX-License-Identifier: MIT

//! A fast, lightweight YAML language server implementing the
//! [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).
//!
//! This crate provides the library modules used by the `rlsp-yaml` binary.
//! See the [repository](https://github.com/chdalski/rlsp) for usage and configuration.

/// Document analysis features: folding, selection, semantic tokens, and symbols.
pub mod analysis;
/// Completion item provider.
pub mod completion;
/// Decorators: code lenses, color highlights, and document links.
pub mod decorators;
/// In-memory document store (tracks open documents and parsed ASTs).
pub mod document_store;
/// Editing features: code actions, formatting, and on-type formatting.
pub mod editing;
/// Hover documentation provider.
pub mod hover;
/// Navigation features: find-references and rename.
pub mod navigation;
/// YAML document parsing utilities used by LSP handlers.
pub mod parser;
/// Scalar type inference helpers for YAML 1.2 Core schema.
pub mod scalar_helpers;
/// JSON Schema loading, resolution, and completion data.
pub mod schema;
/// JSON Schema validation against loaded schemas.
pub mod schema_validation;
/// LSP server implementation (request/notification handlers).
pub mod server;
/// Diagnostic validation: suppression and validators.
pub mod validation;
