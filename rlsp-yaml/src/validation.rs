// SPDX-License-Identifier: MIT

/// Typed settings view used by configurable validators.
pub mod settings;
/// Diagnostic suppression logic (e.g. `# yaml-language-server: disable`).
pub mod suppression;
/// Diagnostic validators (duplicate keys, schema violations, etc.).
pub mod validators;

pub use settings::{DiagnosticCategory, ValidationSettings};
