// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::LineIndex;
use tower_lsp::lsp_types::Diagnostic;

use crate::server::YamlVersion;

/// Shared per-call context threaded through the validation walk.
///
/// Bundles the parameters that every helper needs — the diagnostic accumulator,
/// the `format_validation` flag, and the `yaml_version` for YAML 1.1
/// compatibility checks — so individual helpers do not need many arguments.
pub(super) struct Ctx<'a> {
    pub(super) diagnostics: &'a mut Vec<Diagnostic>,
    pub(super) format_validation: bool,
    pub(super) yaml_version: YamlVersion,
    pub(super) idx: &'a LineIndex,
}

impl<'a> Ctx<'a> {
    pub(super) const fn new(
        diagnostics: &'a mut Vec<Diagnostic>,
        format_validation: bool,
        yaml_version: YamlVersion,
        idx: &'a LineIndex,
    ) -> Self {
        Self {
            diagnostics,
            format_validation,
            yaml_version,
            idx,
        }
    }
}
