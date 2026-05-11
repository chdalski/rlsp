**Repository:** root
**Status:** Completed (2026-04-27)
**Created:** 2026-04-27

# Code Actions Respect User Formatter Config

## Goal

Code actions in `rlsp-yaml/src/editing/code_actions/` currently
ignore the user's `formatPrintWidth`, `formatSingleQuote`,
`formatBracketSpacing`, and other workspace settings. The
`code_actions(docs, text, range, diagnostics, uri)` dispatch
signature does not accept formatter options, so every action
submodule calls `format_subtree(..., &YamlFormatOptions::default(), ...)`.
The server reads user settings into `YamlFormatOptions` for the
formatter (`rlsp-yaml/src/server.rs:1050+`) but never plumbs
them to code actions.

The most user-visible symptom is `block_to_flow`'s `(long line)`
title suffix at `block_to_flow.rs:48`, which compares
`new_text.len()` against a hardcoded `80` and appends the suffix
when exceeded. The hardcoded threshold ignores the user's
`formatPrintWidth`, the comparison runs over the already-wrapped
multi-line output (so the threshold doesn't reflect any
meaningful single-line length), and the formatter's auto-wrap
makes the warning informational at best — auto-format-on-save
users would see the line wrapped on next save anyway, rendering
the warning workflow-conditional.

This plan plumbs user formatter config through the code-actions
dispatch and applies it to `block_to_flow` so the action's
output is wrapped using the user's `formatPrintWidth` (the same
way the document formatter wraps), and the `(long line)` title
suffix is removed entirely — the action produces flow output
that fits the user's configured width by construction, so a
warning serves no purpose. Apply the plumbed config to all 8
action submodules (including `flow_to_block`, which calls a
shared helper in `block_to_flow`) so every action respects user
settings consistently.

## Context

### Current dispatch path

- `pub fn code_actions(docs, text, range, diagnostics, uri)`
  at `rlsp-yaml/src/editing/code_actions.rs:40` — does not
  accept `YamlFormatOptions`
- Server invokes via the LSP `code_action` handler — reads
  `Settings` but doesn't construct or pass `YamlFormatOptions`
  for code actions
- Code-action submodules that call `format_subtree(...)` do
  so with `&YamlFormatOptions::default()` because they have
  no access to user config — the only available option.
  Of the 8 submodules, 6 call `format_subtree` directly
  (`block_scalar`, `block_to_flow`, `delete_anchor`,
  `quoted_bool`, `yaml11_bool`, `yaml11_octal`); 1
  (`flow_to_block`) reaches it indirectly via the shared
  helper `block_text_and_start_col`; 1 (`tab_to_spaces`)
  does not call it at all but still receives the options
  ref for consistent dispatch (see "Affected modules" below)

### Affected modules

`rlsp-yaml/src/editing/code_actions/`:
- `block_to_flow.rs` — primary fix target (auto-wrapped flow
  using the user's `print_width`, no `(long line)` title
  suffix); also exports the helper `block_text_and_start_col`
- `flow_to_block.rs` — calls `block_text_and_start_col` at
  two sites (lines 26 and 133); the helper's signature gains
  the options ref in Task 2, so `flow_to_block` must thread
  options through both call sites
- `block_scalar.rs` — config-respect fix
- `quoted_bool.rs` — config-respect fix
- `yaml11_bool.rs` — config-respect fix
- `yaml11_octal.rs` — config-respect fix
- `delete_anchor.rs` — config-respect fix
- `tab_to_spaces.rs` — config-respect fix (likely no behavior
  change, but consistent plumbing)

Inline `#[cfg(test)]` modules in `code_actions.rs`,
`block_to_flow.rs`, `flow_to_block.rs`, and the other
submodules call `code_actions(...)` directly. Task 1
updates these call sites; the `cargo test -p rlsp-yaml`
acceptance criterion catches any miss at compile time.

### Settings used by the formatter

`server.rs:1050+` constructs `YamlFormatOptions` with these
user-visible fields:
- `print_width` ← `formatPrintWidth` (default 80)
- `tab_width` ← LSP `params.options.tab_size`
- `single_quote` ← `formatSingleQuote` (default false)
- `preserve_quotes` ← `formatPreserveQuotes` (default false)
- `bracket_spacing` ← `formatBracketSpacing` (default true)
- `format_enforce_block_style`, `format_remove_duplicate_keys`,
  `format_indent_sequences` — formatter-only, not relevant
  to code actions on this pass

The plan treats `YamlFormatOptions` as the unit of config
plumbed through — same struct the formatter uses, no new type.

### `block_to_flow` design

Today, the action produces wrapped flow output and adds a
hardcoded `(long line)` warning when the wrapped output's total
byte length exceeds 80. The warning is workflow-conditional
(meaningless under auto-format-on-save, since the formatter
would wrap on save anyway) and the threshold is hardcoded
rather than tied to the user's setting.

Target design:
1. Format the flow node with the user's plumbed
   `YamlFormatOptions` (which carries the user's
   `formatPrintWidth`). The Wadler-Lindig pretty-printer
   produces flow output that fits the user's configured width:
   single-line when it fits, wrapped at structural boundaries
   when it doesn't.
2. Title is always `Convert block to flow style`. No
   `(long line)` suffix; no length comparison; no branching
   logic. The action's contract is "convert block to flow,
   formatted to fit your print_width" — there is nothing to
   warn about because the output always fits by construction.
3. Behavior is identical for users with auto-format-on-save and
   users without — the action emits the same wrapped shape
   either way.

This is the simplest possible design that respects user
settings. If users later want a way to disable auto-wrap (e.g.
to get single-line flow regardless of length), that's a deferred
config follow-up — see Decisions and Non-Goals.

### Fixture fallout

The action's output shape (wrapped flow that fits
`formatPrintWidth`) is largely the same as today's behavior —
the formatter already wrapped the output. The differences after
the fix:

- The wrapping threshold tracks the user's configured
  `formatPrintWidth` instead of the formatter's default `80`.
  For users running default settings, end-state output is the
  same; for users with non-default `formatPrintWidth`, the
  action now respects their width.
- The `(long line)` title suffix is gone. The fixture
  `block-to-flow-long-line-warning.md`'s frontmatter
  `applies-action: (long line)` becomes invalid — that fixture
  must be repurposed (or replaced) to demonstrate
  long-input-produces-wrapped-output instead of the
  now-removed warning.
- A new fixture `block-to-flow-respects-configured-print-width.md`
  exercises non-default `formatPrintWidth` (e.g. 120) and
  proves the user's setting controls the wrap boundary.

`block_scalar` fixtures may also need updates if user's
`print_width` setting affects in-block line-breaks. Audit
during execution.

### Specifications and references

- LSP `code_action` request — server's existing handler is
  the integration point
- `rlsp-yaml/docs/configuration.md:94` — `formatPrintWidth`
  documentation
- `rlsp-yaml/docs/configuration.md:20+` — full settings
  surface

## Steps

- [x] Extend `code_actions(...)` dispatch to accept
      `YamlFormatOptions` (or a `&YamlFormatOptions`)
- [x] Update the server's `code_action` LSP handler to
      construct `YamlFormatOptions` from user settings (mirror
      the formatter's existing path) and pass it through
- [x] Plumb the options reference into every action submodule
      function signature
- [x] Replace every `format_subtree(..., &YamlFormatOptions::default(), ...)`
      call in code-action submodules with the passed-in options
- [x] Fix `block_to_flow.rs:41-52`: pass user's `options`
      (already plumbed by Task 1) to `format_subtree` so the
      formatter wraps using the user's `formatPrintWidth`;
      drop the `(long line)` title-suffix branching logic
      and the hardcoded `80` threshold; emit `Convert block
      to flow style` as the single, unconditional title
- [x] Audit `tests/fixtures/code_actions/block-to-flow-*.md`
      and update `Expected-Document` for each fixture whose
      output changes (now formatter-wrapped flow using the
      user's `formatPrintWidth`)
- [x] Audit `tests/fixtures/code_actions/block-scalar-*.md`
      similarly if `print_width` flow change affects them
- [x] Add at least one new fixture exercising a non-default
      `formatPrintWidth` (e.g. 120) to prove the user's
      configured width is honored at action-emit time
- [x] Update `tests/fixtures/CLAUDE.md` to document any new
      frontmatter field if needed (likely a `format-options:`
      block under `## Code-Action Fixtures` to specify
      non-default `print_width` etc.)
- [x] Update inline tests in `block_to_flow.rs` and other
      modules that previously asserted hardcoded behavior to
      reflect the user-config-driven behavior
- [x] Add an entry to `rlsp-yaml/docs/feature-log.md`
      describing the user-facing behavior changes: (a) all
      code actions now honor user formatter settings
      (`formatPrintWidth`, `formatSingleQuote`,
      `formatBracketSpacing`, `formatPreserveQuotes`)
      consistently with the document formatter; (b)
      `block_to_flow`'s output wraps using the user's
      configured `formatPrintWidth` (instead of a hardcoded
      80-byte threshold over already-wrapped output), and
      the misleading `(long line)` title suffix has been
      removed

## Tasks

### Task 1: Plumb `YamlFormatOptions` through `code_actions` dispatch

Extend the dispatch signature and the server's LSP handler to
pass user formatter config through to code actions. Update
every action submodule's signature to accept the options ref.
Replace `YamlFormatOptions::default()` calls with the
passed-in options at every call site.

This task is structural and behavior-preserving: with default
settings (the only currently-tested path), behavior is
identical to today. The change unlocks the per-action fixes
that follow.

**Sub-tasks:**

- [x] Change `code_actions(...)` signature in
      `rlsp-yaml/src/editing/code_actions.rs:40` to accept a
      `&YamlFormatOptions` (or equivalent options ref)
- [x] Update the server's `code_action` LSP handler in
      `rlsp-yaml/src/server.rs` to build `YamlFormatOptions`
      from user settings (mirror lines 1055–1064) and pass
      it to `code_actions(...)`
- [x] Plumb the options ref into each of the 8 action
      submodule entry-point functions: `block_scalar`,
      `block_to_flow`, `flow_to_block`, `delete_anchor`,
      `quoted_bool`, `tab_to_spaces`, `yaml11_bool`,
      `yaml11_octal`
- [x] Change the signature of the shared helper
      `block_text_and_start_col` in `block_to_flow.rs` to
      accept a `&YamlFormatOptions` parameter, and update
      its two `format_subtree` calls (lines 110 and 113) to
      use the passed-in options. Update both callers in
      `flow_to_block.rs` (lines 26 and 133) to thread their
      options through. **Do NOT apply the single-line
      `usize::MAX` treatment to these two calls** —
      `block_text_and_start_col` produces block-style output
      consumed by `flow_to_block`'s flow→block path;
      single-line treatment would corrupt block conversion.
      Task 2's `usize::MAX` change is scoped exclusively to
      `block_to_flow.rs:41` (the main action's flow output).
- [x] Replace every other `format_subtree(..., &YamlFormatOptions::default(), ...)`
      call site in code-action submodules with the passed-in
      options. Construct quote-style overrides (e.g.
      `quote_opts` in `yaml11_bool.rs` and `yaml11_octal.rs`)
      via `..options.clone()` instead of
      `..YamlFormatOptions::default()` so user settings
      survive struct-update syntax.
- [x] `tab_to_spaces.rs` has no `format_subtree` calls; its
      entry-point signature still gains the options ref for
      consistent plumbing across all 8 modules — no
      behavior change in this submodule
- [x] Update inline tests in `code_actions.rs` and all
      submodules that call `code_actions(...)` directly to
      pass `YamlFormatOptions::default()` explicitly
- [x] Update the fixture harness in
      `rlsp-yaml/tests/code_action_fixtures.rs` to pass a
      fixed `YamlFormatOptions::default()` to
      `code_actions(...)`. Frontmatter-driven options
      (`format-options:`) are explicitly out of scope for
      Task 1 — Task 3 owns that harness extension.

**Completed:** 2026-04-27 — `d228feff4b042d180907439f1b3d4fb793cd2f04`

**Acceptance:**

- `code_actions(...)` accepts a `&YamlFormatOptions` parameter
- All 8 action submodule entry-point functions
  (`block_scalar`, `block_to_flow`, `flow_to_block`,
  `delete_anchor`, `quoted_bool`, `tab_to_spaces`,
  `yaml11_bool`, `yaml11_octal`) take the options ref
- `block_text_and_start_col` in `block_to_flow.rs` accepts
  a `&YamlFormatOptions` parameter; both callers in
  `flow_to_block.rs` thread options through
- After Task 1, no `format_subtree(...)` call site within
  `rlsp-yaml/src/editing/code_actions/` constructs its
  options argument from `YamlFormatOptions::default()`
  (verified by `grep -rn "YamlFormatOptions::default()"
  rlsp-yaml/src/editing/code_actions/` returning only
  `..YamlFormatOptions::default()` patterns that have been
  rewritten to `..options.clone()`, or returning no
  matches at all)
- The server's `code_action` LSP handler constructs
  `YamlFormatOptions` from user settings and passes it
- The fixture harness at `tests/code_action_fixtures.rs`
  passes `YamlFormatOptions::default()` to `code_actions(...)`
  (no frontmatter parsing yet)
- `cargo test -p rlsp-yaml` passes (behavior preserved with
  default options across both inline tests and all 87
  existing fixtures)
- `cargo clippy --all-targets -- -D warnings` exits 0
- `cargo fmt --check` clean

### Task 2: Drop `block_to_flow`'s hardcoded warning logic

Task 1 already routes the user's `options` into
`block_to_flow`'s entry point, so `format_subtree(&flow_node,
options, base_indent)` at line 41 will produce flow output
wrapped to fit the user's `formatPrintWidth` automatically.
This task removes the leftover branching logic at lines 48-52
that was computing the no-longer-needed `(long line)` title
suffix.

**Sub-tasks:**

- [x] Remove the `if new_text.len() > 80 { ... } else { ... }`
      block at `block_to_flow.rs:48-52`. Replace with an
      unconditional `let title = "Convert block to flow style".to_string();`
- [x] Verify `block_to_flow.rs:41` (already touched by Task 1)
      reads as `format_subtree(&flow_node, options, base_indent)`
      — the formatter wraps using `options.print_width`
- [x] Update the 3 inline Pattern C tests in `block_to_flow.rs`:
      - `should_not_append_long_line_warning_for_short_result`
        — the asserted behavior (no `(long line)` suffix on
        short results) is now the *unconditional* behavior.
        Delete this test; the assertion is no longer
        meaningful as a separate case (every output has the
        same title).
      - `should_produce_reparseable_yaml_when_long_sequence_wraps`
        — keep, still validates that wrapped output parses.
        Update only if the test asserts on the title suffix.
      - `should_produce_reparseable_yaml_when_long_nested_mapping_wraps`
        — keep, same reasoning as above.

**Completed:** 2026-04-27 — `7a29e4bc92ba384fc316d02173ee46ad22157d63`

**Acceptance:**

- `block_to_flow.rs` no longer contains the hardcoded `80`
  literal at line 48 (verify with
  `grep -n "\b80\b" rlsp-yaml/src/editing/code_actions/block_to_flow.rs`
  returning no production-code match)
- The action at `block_to_flow.rs:41` invokes `format_subtree`
  with the plumbed `options` argument — the formatter wraps
  using the user's `formatPrintWidth`
- Every `CodeAction` returned by the action has the
  unconditional title `Convert block to flow style` (no
  branching, no suffix)
- `block_text_and_start_col` (lines 110/113) continues to
  produce wrapped block output for `flow_to_block`
  consumers (regression-tested by all existing
  `flow_to_block` inline tests passing without semantic
  change)
- `should_not_append_long_line_warning_for_short_result` is
  deleted; the remaining 2 inline Pattern C tests in
  `block_to_flow.rs` pass with the new unconditional-title
  logic
- `cargo test -p rlsp-yaml` passes
- `cargo clippy --all-targets -- -D warnings` exits 0
- `cargo fmt --check` clean

### Task 3: Update fixtures to reflect user-config-aware output

Audit and update `tests/fixtures/code_actions/block-to-flow-*.md`
and any other fixtures whose output now changes. Add at least
one fixture exercising a non-default `formatPrintWidth`.

**Sub-tasks:**

- [x] Add an entry to `rlsp-yaml/docs/feature-log.md`
      describing the user-facing behavior changes from
      Tasks 1 and 2: (a) all code actions now honor user
      formatter settings consistently with the document
      formatter; (b) `block_to_flow`'s output wraps using
      the user's configured `formatPrintWidth`, and the
      misleading `(long line)` title suffix has been
      removed
- [x] Decide on a `format-options:` frontmatter convention
      for fixtures that need non-default options (e.g.
      `format-options: { print_width: 120 }`). Update the
      harness in `tests/code_action_fixtures.rs` to parse
      it.
- [x] Audit every `block-to-flow-*.md` fixture: re-derive
      `Expected-Document` if needed. Output shape is
      formatter-controlled (wrapped flow that fits
      `formatPrintWidth`); under default settings most
      fixtures should match today's expected output, since
      today's hardcoded threshold (80) coincides with the
      default `formatPrintWidth` (80).
- [x] Repurpose or delete `block-to-flow-long-line-warning.md`.
      Renamed to `block-to-flow-wraps-long-output.md` with
      `applies-action: Convert block to flow style`.
- [x] Add a new fixture
      `block-to-flow-respects-configured-print-width.md`
      that uses `format-options: { print_width: 120 }` to
      prove the action honors the user's configured width
- [x] Audit `block-scalar-*.md` fixtures for output changes
      (none required — all 24 fixtures pass unchanged)
- [x] Update `tests/fixtures/CLAUDE.md`'s
      `## Code-Action Fixtures` section to document the
      `format-options:` field

**Completed:** 2026-04-27 — `d244f46ac8fe124fe1c88cc2e87da8a14d157f70`

**Acceptance:**

- `cargo test --test code_action_fixtures` passes for every
  fixture under `tests/fixtures/code_actions/` — the harness
  assertion is the contract; any fixture whose
  `Expected-Document` is wrong now fails this check
- `block-to-flow-long-line-warning.md` no longer exists in
  its current form (deleted or renamed/repurposed) — its
  frontmatter `applies-action: (long line)` is invalid
  after Task 2 removes the title suffix
- A new fixture file
  `rlsp-yaml/tests/fixtures/code_actions/block-to-flow-respects-configured-print-width.md`
  exists, contains `format-options:` frontmatter setting
  `print_width: 120`, has an `Expected-Document` showing
  unwrapped single-line output for input whose single-line
  flow form is ~85 chars, and passes
- `tests/fixtures/CLAUDE.md`'s `## Code-Action Fixtures`
  section contains a documented `format-options:` field
  with at least one example and a statement of which option
  keys are supported (mirroring the formatter fixtures'
  `settings:` block)
- `rlsp-yaml/docs/feature-log.md` contains a new entry
  describing the user-facing behavior changes (all code
  actions honoring user formatter settings;
  `block_to_flow`'s wrap behavior now driven by
  `formatPrintWidth`; `(long line)` title suffix removed)
- The fixture harness at `tests/code_action_fixtures.rs`
  parses the `format-options:` block when present, falls
  back to `YamlFormatOptions::default()` when absent, and
  has at least 2 new self-tests covering: (a) a fixture
  with `format-options:` parses correctly and is passed to
  `code_actions(...)`, (b) a fixture without
  `format-options:` uses defaults
- `cargo test -p rlsp-yaml` passes
- `cargo clippy --all-targets -- -D warnings` exits 0
- `cargo fmt --check` clean

## Decisions

- **Plumb `YamlFormatOptions` directly, do not introduce a
  separate `CodeActionOptions` type** — code actions consume
  the same setting surface the formatter does (print_width,
  bracket_spacing, single_quote, preserve_quotes). A separate
  type would duplicate fields and force a sync rule (see the
  existing CLAUDE.md "Settings Sync" table). YAGNI.
- **Auto-wrap output to fit `formatPrintWidth`, no warning** —
  the action emits flow output wrapped to fit the user's
  configured width (single-line when it fits, wrapped at
  structural boundaries when it doesn't). No
  `(long line)` warning. Rationale: the warning was
  workflow-conditional (auto-format-on-save users would see
  the line wrapped on next save anyway, making the warning
  actionable only for the subset without auto-format). A
  setting to opt out of auto-wrap (e.g. `autoWrapFlowStyle:
  false` for users who want single-line flow regardless of
  length) is a deferred follow-up — added if user demand
  emerges, not pre-emptively.
- **No `usize::MAX` no-wrap path** — the formatter's existing
  `print_width`-driven wrapping is exactly what we want, no
  new code paths in `rlsp-fmt` required.
- **Fixture frontmatter extension allowed (`format-options:`)
  for the explicit purpose of testing config-driven
  behavior** — same field family as the formatter fixtures'
  `settings:` block. The format is no longer "locked" in the
  sense it was after the original code-action fixture plan;
  this plan extends it deliberately for a documented purpose.
- **Validators are not in scope** — the audit found no
  `YamlFormatOptions` references in `rlsp-yaml/src/validation/`,
  so the systemic config-respect issue does not extend to
  validators. If validators later grow a need for user format
  config, that's a separate plan.
- **`formatEnforceBlockStyle` policy enforcement is not in
  scope** — when this setting is true, the user prefers
  block-only style and `block_to_flow` should arguably not
  be offered. That's a policy/UX decision separate from the
  warning-correctness issue this plan addresses. File as a
  follow-up.

## Non-Goals

- Validator config integration — validators don't currently
  consume formatter options
- `formatEnforceBlockStyle` policy enforcement (suppressing
  `block_to_flow` when user prefers block-only) — separate
  policy concern
- An `autoWrapFlowStyle` (or similar) configuration setting
  to disable auto-wrap and force single-line flow output —
  defer until evidence shows users want to opt out of the
  wrapping default
- Refactoring the formatter or `format_subtree` API —
  consume the existing API; do not re-design it
- Adding new formatter settings — this plan plumbs *existing*
  settings; new settings are out of scope
- Changing the fixture harness beyond the `format-options:`
  field addition — the broader fixture format stays as it is
- Touching the diagnostic-driven action modules' inline
  tests beyond what Task 1 mechanically requires (signature
  threading)
- Fixing the `string_to_block_scalar` anchor-doubling bug
  tracked in `.ai/memory/project_followup_plans.md` —
  separate concern
