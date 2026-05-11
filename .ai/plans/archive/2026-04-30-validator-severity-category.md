**Repository:** root
**Status:** Completed (2026-04-30)
**Created:** 2026-04-30

# Validator Severity Category — Resolve at the Producer

## Goal

`validate_flow_style` and `validate_duplicate_keys` construct
diagnostics with hardcoded `WARNING`/`ERROR` severity at
`rlsp-yaml/src/validation/validators.rs:218` and `:567`. The
LSP server then walks each returned vec in `parse_and_publish`
(`rlsp-yaml/src/server.rs:476–496`) and overwrites
`diag.severity` based on the user's `flowStyle` and
`duplicateKeys` string settings. The override works, but it
is detached from the producer — a future change flipping a
validator's hardcoded default would silently no-op the
`== "error"` / `== "warning"` check.

Make the user setting reach the validator directly so
severity is decided once, at the construction site. Replace
the post-hoc rewrite loops with a typed `ValidationSettings`
view that the server constructs once at the parse-and-publish
boundary, and have each configurable validator look up its
own severity by category. No user-facing behavior change.

## Context

- **Producer-rewrite smell.** The two validators bake severity
  in (`flow_diagnostic` and `push_duplicate_diagnostic`),
  while `parse_and_publish` re-walks the diagnostic vec to
  rewrite severity according to user settings. Reading either
  validator in isolation, you would conclude its severity is
  fixed — the truth lives only in `server.rs`.
- **String comparison at the override site.** The current code
  compares against literal `"off"`, `"warning"`, `"error"` in
  `server.rs`. Strings should be parsed once at the boundary,
  not at every comparison site.
- **Other validators are not configurable today.**
  `validate_unused_anchors`, `validate_custom_tags`,
  `validate_key_ordering`, `validate_yaml11_compat`, and the
  schema validator hardcode their severities with no override
  path. They are out of scope — adding them to
  `ValidationSettings` now would either inject hardcoded
  `Some(WARNING)` at the call site (same smell, new disguise)
  or pre-emptively expose severity controls nobody asked for
  (config sprawl + YAGNI). The new module is designed so a
  future task that makes one of them configurable adds one
  enum variant and one settings field — no design rework.
- **External callers.** `validate_flow_style` and
  `validate_duplicate_keys` are `pub` functions called from
  three integration tests:
  - `rlsp-yaml/tests/code_action_property_preservation.rs`
  - `rlsp-yaml/tests/ecosystem_fixtures.rs`
  - `rlsp-yaml/tests/corpus_invariants.rs`
  Their signatures change. Each call site must pass
  `&ValidationSettings::default()` to get the current default
  behavior (flow_style = WARNING, duplicate_keys = ERROR).
- **Settings strings and defaults.** `Settings.flow_style:
  Option<String>` documents `"off"` / `"warning"` (default) /
  `"error"`; `Settings.duplicate_keys: Option<String>`
  documents `"off"` / `"warning"` / `"error"` (default).
  `ValidationSettings::default()` and the `From<&Settings>`
  conversion must preserve those defaults exactly.
- **Pattern reference.** `code_actions` already uses a similar
  shape: the server reads raw `Settings` fields once and
  constructs a typed `YamlFormatOptions` view at the boundary
  (`rlsp-yaml/src/server.rs:1002–1033`). `ValidationSettings`
  follows the same pattern.

## Decisions

- **`DiagnosticCategory` enum + `ValidationSettings` struct,
  in a new module `rlsp-yaml/src/validation/settings.rs`.**
  The enum names the configurable categories
  (`FlowStyle`, `DuplicateKey`); the struct maps each
  category to `Option<DiagnosticSeverity>` (`None` = off).
  A `severity_for(category) -> Option<DiagnosticSeverity>`
  method centralizes the lookup; validators call it and
  return early if `None`.
- **Narrow scope: only the two configurable validators.** Do
  not retrofit `validate_unused_anchors`,
  `validate_custom_tags`, `validate_key_ordering`,
  `validate_yaml11_compat`, or the schema validator. The
  abstraction is built so each future configurability ask is
  one variant + one field, but those validators stay
  unchanged here.
- **`ValidationSettings::default()` matches current default
  behavior** — flow_style = `Some(WARNING)`, duplicate_keys =
  `Some(ERROR)`. External tests calling validators with the
  default get the same severities they assert today.
- **Boundary parsing.** The server reads raw `Settings`
  strings once in `parse_and_publish` and constructs
  `ValidationSettings` via `From<&Settings>` (or equivalent
  constructor). Comparison strings disappear from
  `server.rs`; they live only in the boundary parser, with
  unit tests for valid / unknown / absent values.
- **Two tasks.** Task 1 establishes the pattern end-to-end
  with `validate_flow_style` (new module, server boundary,
  tests). Task 2 mirrors it for `validate_duplicate_keys`
  (one variant + one field + thread severity). Splitting
  isolates the design risk in Task 1 while making Task 2 a
  near-mechanical follow-up.

## Non-Goals

- Retrofitting `validate_unused_anchors`,
  `validate_custom_tags`, `validate_key_ordering`,
  `validate_yaml11_compat`, or the schema validator. They
  remain unchanged and are not added to
  `DiagnosticCategory`.
- Changing user-facing behavior (severities, settings
  schema, settings JSON keys, default values, "off"
  semantics). This is a refactor; output for any given
  configuration is identical before and after.
- Removing or renaming the `flowStyle` / `duplicateKeys`
  settings or their string vocabulary (`off` / `warning` /
  `error`). The string vocabulary is the user-facing API.
- Adding new severities (e.g. INFORMATION as a configurable
  level for these validators) — not requested.

## Steps

- [x] Task 1: Introduce `validation/settings.rs` and retrofit
      `validate_flow_style`
- [x] Task 2: Retrofit `validate_duplicate_keys` to use
      `ValidationSettings`

## Tasks

### Task 1: Introduce `validation/settings.rs` and retrofit `validate_flow_style`

**Commit:** `e274dcdfe4f8a4ca1dd1aff7299fcae7f89727f4`

Create the `DiagnosticCategory` enum and `ValidationSettings`
struct in a new module `rlsp-yaml/src/validation/settings.rs`,
declare it from `rlsp-yaml/src/validation.rs`, and convert
`validate_flow_style` to consume it. Replace the
`if flow_style_setting != "off" { ... if == "error" { rewrite
loop } }` block in `parse_and_publish` with a single
`diagnostics.extend(validate_flow_style(&docs, &settings))`
call. `validate_duplicate_keys` is not touched in this task —
its existing string-override block in `server.rs` stays in
place until Task 2.

- [x] Add `rlsp-yaml/src/validation/settings.rs` with:
  - `pub enum DiagnosticCategory { FlowStyle }` (the
    `DuplicateKey` variant is added in Task 2).
  - `pub struct ValidationSettings { pub flow_style:
    Option<DiagnosticSeverity> }` (the `duplicate_keys` field
    is added in Task 2).
  - `impl Default for ValidationSettings` returning
    `flow_style: Some(DiagnosticSeverity::WARNING)`.
  - `impl ValidationSettings { pub fn severity_for(&self,
    category: DiagnosticCategory) -> Option<DiagnosticSeverity> }`.
  - A constructor `pub fn from_settings(settings: &Settings)
    -> ValidationSettings` (or equivalent `From` impl) that
    parses `settings.flow_style: Option<String>` to
    `Option<DiagnosticSeverity>` using the documented mapping
    (`"off"` → `None`; `"warning"` or absent → `Some(WARNING)`;
    `"error"` → `Some(ERROR)`; unknown strings → default).
- [x] Declare the new module in `rlsp-yaml/src/validation.rs`
      and re-export `DiagnosticCategory` and
      `ValidationSettings`.
- [x] Modify `validate_flow_style` in
      `rlsp-yaml/src/validation/validators.rs` to take
      `(&[Document<Span>], &ValidationSettings)`. Look up
      `settings.severity_for(DiagnosticCategory::FlowStyle)`;
      return `Vec::new()` early if `None`. Pass the resolved
      `DiagnosticSeverity` into `flow_diagnostic` so the
      hardcoded `WARNING` at line 218 is removed.
- [x] In `rlsp-yaml/src/server.rs::parse_and_publish`,
      construct `ValidationSettings` once from the locked
      `Settings` (matching the pattern at lines 1002–1033),
      drop the `if flow_style_setting.as_deref() != Some("off")
      { ... }` block entirely, and replace it with a single
      `diagnostics.extend(validate_flow_style(&result.documents,
      &validation_settings));` call. The
      `validate_duplicate_keys` block in `server.rs` is
      unchanged here.
- [x] Update inline tests in
      `rlsp-yaml/src/validation/validators.rs` that call
      `validate_flow_style` to pass
      `&ValidationSettings::default()`.
- [x] Update integration tests calling `validate_flow_style`
      to pass `&ValidationSettings::default()`:
      `tests/code_action_property_preservation.rs`,
      `tests/ecosystem_fixtures.rs`,
      `tests/corpus_invariants.rs`.
- [x] Add unit tests in `rlsp-yaml/src/validation/settings.rs`
      for the boundary parser:
  - `from_settings` produces `flow_style: None` when the
    string is `"off"`.
  - `from_settings` produces `Some(WARNING)` when the
    string is absent (default).
  - `from_settings` produces `Some(WARNING)` when the
    string is `"warning"`.
  - `from_settings` produces `Some(ERROR)` when the
    string is `"error"`.
  - `from_settings` falls back to default when the string
    is an unknown value (e.g. `"verbose"`).
  - `severity_for(DiagnosticCategory::FlowStyle)` returns
    the configured `flow_style` value.
- [x] Add unit tests in
      `rlsp-yaml/src/validation/validators.rs` for severity
      propagation:
  - `validate_flow_style` returns empty vec when
    `flow_style: None` even on input that would otherwise
    produce diagnostics.
  - `validate_flow_style` produces ERROR-severity
    diagnostics when `flow_style: Some(ERROR)`.
  - `validate_flow_style` produces WARNING-severity
    diagnostics with `ValidationSettings::default()`
    (regression for the existing default).

**Acceptance criteria.**

- `cargo build`, `cargo clippy --all-targets`, `cargo test`,
  `cargo fmt --check` all pass.
- The `flow_diagnostic` helper no longer references
  `DiagnosticSeverity::WARNING` directly.
- `parse_and_publish` no longer contains the
  `if flow_style_setting.as_deref() == Some("error") { ...
  for diag in &mut flow_diags { diag.severity = ... } }`
  rewrite loop. (The `duplicate_keys` rewrite block stays;
  Task 2 removes it.)
- `validate_flow_style`'s public signature is
  `pub fn validate_flow_style(docs: &[Document<Span>],
  settings: &ValidationSettings) -> Vec<Diagnostic>`.
- All three integration test files compile and pass with
  `&ValidationSettings::default()` at every call site.
- New unit tests cover the boundary-parser cases listed
  above and the three severity-propagation cases.

### Task 2: Retrofit `validate_duplicate_keys` to use `ValidationSettings`

**Commit:** `e3e57759d84ff404f2964095af0841346e12ec50`

Mirror Task 1 for the duplicate-key validator. Add
`DiagnosticCategory::DuplicateKey`, add the `duplicate_keys`
field on `ValidationSettings` (default `Some(ERROR)`), thread
the resolved severity through `push_duplicate_diagnostic`,
and remove the second rewrite block from `parse_and_publish`.
After this task lands, `parse_and_publish` no longer rewrites
diagnostic severity at all — the producer-rewrite smell is
fully resolved.

- [x] Extend `DiagnosticCategory` with the `DuplicateKey`
      variant.
- [x] Add `pub duplicate_keys: Option<DiagnosticSeverity>` to
      `ValidationSettings`. Update `Default::default` to
      include `duplicate_keys: Some(DiagnosticSeverity::ERROR)`.
- [x] Update `severity_for` to return `duplicate_keys` for the
      new variant.
- [x] Update `from_settings` to parse `settings.duplicate_keys:
      Option<String>` to `Option<DiagnosticSeverity>` using
      the documented mapping (`"off"` → `None`; `"warning"`
      → `Some(WARNING)`; `"error"` or absent →
      `Some(ERROR)`; unknown strings → default).
- [x] Modify `validate_duplicate_keys` to take
      `(&[Document<Span>], &ValidationSettings)`. Look up
      `settings.severity_for(DiagnosticCategory::DuplicateKey)`;
      return `Vec::new()` early if `None`. Thread the
      resolved severity through `push_duplicate_diagnostic`
      so the hardcoded `ERROR` at line 567 is removed.
- [x] In `parse_and_publish`, replace the `if
      duplicate_keys_setting.as_deref() != Some("off") { ...
      if == "warning" { rewrite loop } }` block with a single
      `diagnostics.extend(validate_duplicate_keys(&result.documents,
      &validation_settings));` call.
- [x] Update inline tests in `validators.rs` calling
      `validate_duplicate_keys` to pass
      `&ValidationSettings::default()`.
- [x] Update integration tests calling
      `validate_duplicate_keys`:
      `tests/ecosystem_fixtures.rs`,
      `tests/corpus_invariants.rs`.
- [x] Extend `from_settings` boundary-parser tests for the
      new `duplicate_keys` cases (off / absent / warning /
      error / unknown).
- [x] Extend `severity_for` tests to cover the
      `DiagnosticCategory::DuplicateKey` variant.
- [x] Add severity-propagation unit tests for
      `validate_duplicate_keys`:
  - Returns empty vec when `duplicate_keys: None`.
  - Produces WARNING-severity diagnostics when
    `duplicate_keys: Some(WARNING)`.
  - Produces ERROR-severity diagnostics with
    `ValidationSettings::default()` (regression for the
    existing default).

**Acceptance criteria.**

- `cargo build`, `cargo clippy --all-targets`, `cargo test`,
  `cargo fmt --check` all pass.
- `push_duplicate_diagnostic` no longer references
  `DiagnosticSeverity::ERROR` directly.
- `parse_and_publish` contains zero `for diag in &mut
  <validator_diags> { diag.severity = ... }` rewrite loops.
- Both validator string settings (`flow_style_setting`,
  `duplicate_keys_setting`) are no longer dereferenced or
  string-compared inside `parse_and_publish`. The string
  parsing exists only in `from_settings`.
- `validate_duplicate_keys`'s public signature is
  `pub fn validate_duplicate_keys(docs: &[Document<Span>],
  settings: &ValidationSettings) -> Vec<Diagnostic>`.
- Both integration test files compile and pass with
  `&ValidationSettings::default()` at every call site.
- New unit tests cover the duplicate-key boundary parser
  cases and the three severity-propagation cases.
