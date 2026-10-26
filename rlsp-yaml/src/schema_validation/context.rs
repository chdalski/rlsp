// SPDX-License-Identifier: MIT

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineIndex, Span};
use tower_lsp::lsp_types::Diagnostic;

use crate::schema::ParsedSchema;
use crate::server::YamlVersion;

/// Steps every run may spend regardless of document size.
pub(super) const STEP_FLOOR: usize = 40_000;

/// Steps granted per YAML node in the run's documents. Bounds the work a schema
/// can multiply onto each node, which keeps total work linear in document size.
pub(super) const STEPS_PER_NODE: usize = 64;

/// Upper bound on the budget of any run, so a huge document cannot scale it
/// without limit.
pub(super) const STEP_CEILING: usize = 5_000_000;

/// Units of scanning work (comparisons, key lookups, copied nodes, text bytes)
/// that one step pays for. The step that enters a node covers the first block of
/// work, so only larger scans cost more.
pub(super) const WORK_PER_STEP: usize = 64;

/// Steps that scanning `work` units costs.
pub(super) const fn work_steps(work: usize) -> usize {
    work / WORK_PER_STEP
}

/// Most diagnostics a run returns. Reaching it ends the run early, like running
/// out of steps, so a hostile schema cannot flood the client with diagnostics.
pub(super) const MAX_DIAGNOSTICS: usize = 10_000;

/// Validation steps a run may spend, shared by everything the run does.
///
/// A step is one entry into `validate_node`, one schema node visited by the
/// `collect_evaluated_*` helpers, one use of a schema pattern (per
/// `REGEX_BYTES_PER_STEP` bytes of pattern), or `WORK_PER_STEP` units of work
/// scanning input, such as the comparisons of a `uniqueItems` check.
/// The first refused spend marks the budget exhausted for good: whatever the
/// run computes afterwards is cut short, and callers must not read a cut-short
/// result as a pass.
pub(super) struct Budget {
    limit: usize,
    spent: Cell<usize>,
    exhausted: Cell<bool>,
    /// Schema patterns this run has paid the compile price of.
    paid_patterns: RefCell<HashSet<String>>,
    /// `format: regex` document values this run has compiled, with whether each
    /// is valid and the steps its compilation was priced at.
    regex_values: RefCell<HashMap<String, (bool, usize)>>,
}

impl Budget {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            spent: Cell::new(0),
            exhausted: Cell::new(false),
            paid_patterns: RefCell::new(HashSet::new()),
            regex_values: RefCell::new(HashMap::new()),
        }
    }

    pub(super) fn has_paid_for_pattern(&self, pattern: &str) -> bool {
        self.paid_patterns.borrow().contains(pattern)
    }

    pub(super) fn note_paid_for_pattern(&self, pattern: &str) {
        if !self.has_paid_for_pattern(pattern) {
            self.paid_patterns.borrow_mut().insert(pattern.to_string());
        }
    }

    pub(super) fn recall_regex_value(&self, value: &str) -> Option<(bool, usize)> {
        self.regex_values.borrow().get(value).copied()
    }

    pub(super) fn remember_regex_value(&self, value: &str, outcome: (bool, usize)) {
        self.regex_values
            .borrow_mut()
            .insert(value.to_string(), outcome);
    }

    /// Take `steps` from the budget. Returns `false` and marks the budget
    /// exhausted when fewer than `steps` remain.
    pub(super) fn spend(&self, steps: usize) -> bool {
        match self.spent.get().checked_add(steps) {
            Some(total) if total <= self.limit => {
                self.spent.set(total);
                true
            }
            _ => {
                self.exhaust();
                false
            }
        }
    }

    /// Take the steps that scanning `work` units costs. Returns `false` and
    /// marks the budget exhausted when they do not remain.
    pub(super) fn spend_work(&self, work: usize) -> bool {
        self.spend(work_steps(work))
    }

    /// Stop the run: every later spend is refused.
    pub(super) fn exhaust(&self) {
        self.spent.set(self.limit);
        self.exhausted.set(true);
    }

    /// Whether a spend was ever refused.
    pub(super) const fn is_exhausted(&self) -> bool {
        self.exhausted.get()
    }

    /// Steps spent so far.
    #[cfg(test)]
    pub(super) const fn spent(&self) -> usize {
        self.spent.get()
    }
}

/// Step budget for validating `docs`: a fixed floor plus a fixed number of steps
/// per unit of document weight (see `node_weight`), capped at `STEP_CEILING`.
pub(super) fn step_budget(docs: &[Document<Span>]) -> usize {
    let nodes = docs
        .iter()
        .map(|doc| node_weight(&doc.root))
        .fold(0_usize, usize::saturating_add);
    nodes
        .saturating_mul(STEPS_PER_NODE)
        .saturating_add(STEP_FLOOR)
        .min(STEP_CEILING)
}

/// Weight of the tree rooted at `node`, mapping keys included: one per node,
/// plus the steps a scalar's text costs to scan, so a large string brings the
/// steps its checks need.
pub(super) fn node_weight(node: &Node<Span>) -> usize {
    match node {
        Node::Scalar { value, .. } => work_steps(value.len()).saturating_add(1),
        Node::Alias { .. } => 1,
        Node::Mapping { entries, .. } => entries
            .iter()
            .map(|(key, value)| node_weight(key).saturating_add(node_weight(value)))
            .fold(1, usize::saturating_add),
        Node::Sequence { items, .. } => {
            items.iter().map(node_weight).fold(1, usize::saturating_add)
        }
    }
}

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
/// checks, the step `budget`, the reporting `mode`, and the parsed schema that
/// resolves reference nodes — so individual helpers do not need many arguments.
pub(super) struct Ctx<'a> {
    pub(super) diagnostics: &'a mut Vec<Diagnostic>,
    pub(super) format_validation: bool,
    pub(super) yaml_version: YamlVersion,
    pub(super) idx: &'a LineIndex,
    pub(super) budget: &'a Budget,
    /// Resolves the reference nodes of the schema being validated against.
    pub(super) parsed: &'a ParsedSchema,
    pub(super) mode: Mode,
    /// How many embedded documents (`contentSchema`) enclose the node being
    /// validated. Each of them restarts the validation depth, so this is the
    /// bound on a schema that reaches `contentSchema` through a cycle.
    pub(super) content_depth: usize,
    /// Set when this walk skipped a check it could not finish (depth limit,
    /// or a `collect_evaluated_*` scan the budget cut short). A probe that
    /// records no diagnostic but is undecided has not passed.
    pub(super) undecided: bool,
}

impl<'a> Ctx<'a> {
    pub(super) const fn new(
        diagnostics: &'a mut Vec<Diagnostic>,
        format_validation: bool,
        yaml_version: YamlVersion,
        idx: &'a LineIndex,
        budget: &'a Budget,
        parsed: &'a ParsedSchema,
    ) -> Self {
        Self {
            diagnostics,
            format_validation,
            yaml_version,
            idx,
            budget,
            parsed,
            mode: Mode::Report,
            content_depth: 0,
            undecided: false,
        }
    }

    pub(super) const fn with_mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }

    /// Sets the number of embedded documents that enclose the walk.
    pub(super) const fn with_content_depth(mut self, content_depth: usize) -> Self {
        self.content_depth = content_depth;
        self
    }

    /// True once a probe has recorded a diagnostic: its outcome is decided.
    pub(super) const fn probe_decided(&self) -> bool {
        matches!(self.mode, Mode::Probe) && !self.diagnostics.is_empty()
    }
}
