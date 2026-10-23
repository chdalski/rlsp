// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::LineIndex;
use tower_lsp::lsp_types::Diagnostic;

use crate::server::YamlVersion;

/// Whether the walk reports every diagnostic or only decides pass/fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// Record every diagnostic.
    Report,
    /// The caller reads only whether any diagnostic was recorded, so
    /// validation stops at the first one.
    Probe,
}

/// Shared per-call context threaded through the validation walk.
///
/// Bundles the parameters that every helper needs — the diagnostic accumulator,
/// the `format_validation` flag, the `yaml_version` for YAML 1.1 compatibility
/// checks, and the reporting `mode` — so individual helpers do not need many
/// arguments.
pub(super) struct Ctx<'a> {
    pub(super) diagnostics: &'a mut Vec<Diagnostic>,
    pub(super) format_validation: bool,
    pub(super) yaml_version: YamlVersion,
    pub(super) idx: &'a LineIndex,
    pub(super) mode: Mode,
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
            mode: Mode::Report,
        }
    }

    pub(super) const fn with_mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }

    /// True once a probe has recorded a diagnostic: its outcome is decided.
    pub(super) const fn probe_decided(&self) -> bool {
        matches!(self.mode, Mode::Probe) && !self.diagnostics.is_empty()
    }
}
