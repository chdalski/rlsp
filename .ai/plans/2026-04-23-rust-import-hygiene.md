**Repository:** root
**Status:** InProgress
**Created:** 2026-04-23

# Rust Import Placement — Workspace-Wide Cleanup

## Goal

Bring the entire Rust workspace into compliance with the
`.claude/rules/lang-rust-imports.md` rule (already committed
at `7ef996a`). A full `syn`-based AST scan at plan-start
identified **76 violations across 23 files**. This plan
enumerates every violation by file and line so the
developer does not re-discover them, and decomposes the
work into vertical task slices that leave the workspace
green (`cargo fmt --check`, `cargo clippy --all-targets`
zero warnings, `cargo test`) after each task.

## Context

- **Rule in force:** `.claude/rules/lang-rust-imports.md`
  (loaded via `paths: ["**/*.rs"]`). Two-tier module-scope
  placement (crate roots: `mod` → `pub use` → `use` → items;
  sub-modules: `use` → `mod` → items), block-scope
  recursion, three function-body `use` exceptions (variant
  glob, collision resolution, cfg-gated), dead-local
  anti-pattern.
- **Scan method:** `syn` AST parse of every `.rs` file under
  `rlsp-fmt/`, `rlsp-yaml/`, `rlsp-yaml-parser/`
  (excluding `target/`). Scanner source:
  `/tmp/import-scan/src/main.rs`; can be re-run to verify
  zero violations after each task. Violation kinds:
  - `UseAfterItem` — module-scope `use` after a non-import
    item. **Count: 5.**
  - `ModAfterItem` — module-scope `mod X;` after a
    non-import item. **Count: 19.**
  - `ModAfterUseInCrateRoot` — crate-root `mod X;` after a
    `use` statement (no intervening items). **Count: 3.**
  - `UseAtTopOfFnBody` — `use` inside a function body at the
    top of the block; needs classification against the
    three allowed exceptions or hoisting. **Count: 49.**
  - `UseAfterStmtInFnBody` — `use` inside a function body
    after another statement (definite violation).
    **Count: 0.** No such cases in the workspace.
- **Scope:**
  - `rlsp-fmt/` — zero violations; no task needed.
  - `rlsp-yaml-parser/` — 35 violations across 11 files.
  - `rlsp-yaml/` — 41 violations across 12 files.
  - VS Code extension (`rlsp-yaml/integrations/vscode/`)
    out of scope (TypeScript).
- **Per-task verification:** `cargo fmt --check`,
  `cargo clippy --all-targets` (zero warnings — workspace
  enforces `warnings = "deny"`), `cargo test`. Behavior must
  not change — this is pure relocation and dead-import
  removal. Existing tests are the regression safety net.
- **References:**
  - `.claude/rules/lang-rust-imports.md` — the rule.
  - Project `CLAUDE.md` — workspace layout and conventions.
  - [Rust Style Guide — imports](https://doc.rust-lang.org/nightly/style-guide/#imports).

## Steps

- [x] Fix the 27 module-scope violations across 7 files
      (Task 1) — mechanical hoisting and reordering.
      *(commit `5b81387`)*
- [x] Classify and fix the 14 function-body `use`
      statements in `rlsp-yaml-parser/` across 7 files
      (Task 2). *(commit `fffe9d8`)*
- [ ] Classify and fix the 35 function-body `use`
      statements in `rlsp-yaml/` across 11 files, and
      confirm zero violations remain across the workspace
      (Task 3).

## Tasks

### Task 1: Fix all module-scope violations (27 across 7 files)

Pure mechanical work — relocate misplaced `use` and `mod`
declarations to the file-header position per the rule's
two-tier convention.

**`rlsp-yaml/src/schema_validation.rs` (sub-module — `use` → `mod` → items):**

- [x] `:39` hoist `use crate::scalar_helpers;` into the
      top-of-file `use crate::...` group.
- [x] `:40` hoist `use crate::schema::{AdditionalProperties,
      JsonSchema, SchemaType};` into the same group.
- [x] `:41` hoist `use crate::server::YamlVersion;` into the
      same group.
- [x] `:43` hoist `mod formats;` to the module's `mod`
      group directly after the `use` block (create the
      group if none exists).

**`rlsp-yaml/src/editing/code_actions/yaml11_octal.rs` (sub-module):**

- [x] `:556` hoist `use rstest::rstest;` to the top of the
      enclosing `#[cfg(test)] mod tests { ... }` block (the
      block is the test module, not the file; `use` must
      precede any `#[test] fn` in the block).

**`rlsp-yaml/tests/corpus_invariants.rs` (test-crate root — `mod` → `use` → items):**

- [x] `:1305` hoist `use rlsp_yaml_parser::{CollectionStyle,
      Pos, ScalarStyle, Span as TestSpan};` into the
      top-of-file `use` block (after any `mod`
      declarations).

**`rlsp-yaml-parser/tests/smoke/main.rs` (test-crate root — `mod` → `use` → items):**

- [x] `:96–:113` move all 18 `mod X;` declarations
      (`anchors_and_aliases`, `block_scalars`, `comments`,
      `conformance`, `directives`, `documents`,
      `flow_collections`, `folded_scalars`, `mappings`,
      `nested_collections`, `nested_flow_block_mixing`,
      `probe_dispatch`, `quoted_scalars`, `scalar_dispatch`,
      `scalars`, `sequences`, `stream`, `tags`) to the very
      top of the file, above the existing `use` block.
- [x] Remove the `// Submodules` banner comment — it
      was compensating for the misplaced location.
- [x] Final file order: SPDX/doc → `mod` declarations →
      `use` statements → helper fns (`event_variants`,
      `parse_to_vec`, `evs`, `has_error`, `scalar_values`,
      `count`) → `#[test]` functions.

**`rlsp-yaml-parser/benches/latency.rs` (bench-crate root):**

- [x] `:23` move `mod fixtures;` above the `use` block at
      the top of the file (crate-root order: `mod` → `use`).

**`rlsp-yaml-parser/benches/memory.rs` (bench-crate root):**

- [x] `:20` same — move `mod fixtures;` above the `use`
      block.

**`rlsp-yaml-parser/benches/throughput.rs` (bench-crate root):**

- [x] `:23` same — move `mod fixtures;` above the `use`
      block.

**Verification:**

- [x] `cargo fmt` applied to every modified file.
- [x] `cargo fmt --check` clean across the workspace.
- [x] `cargo clippy --all-targets` zero warnings across the
      workspace.
- [x] `cargo test` passes across the workspace.
- [x] Verify each listed checkbox above by inspecting the
      modified file's header block against the two-tier
      convention. If the scanner at `/tmp/import-scan/` is
      still available, re-running it against the 7 files
      reports zero module-scope violations
      (`UseAfterItem`, `ModAfterItem`,
      `ModAfterUseInCrateRoot` all zero) — otherwise
      per-file inspection plus the above `cargo fmt --check`
      and `cargo clippy --all-targets` clean runs are the
      verification.

### Task 2: Classify and fix fn-body `use` statements in `rlsp-yaml-parser` (14 cases across 7 files)

For every listed line, read the enclosing function, classify
the `use` against the rule's allowed exceptions, and act:

- **Variant glob** (`use X::*;` where unqualified variant
  names appear in the body) — keep unchanged.
- **Name-collision resolver** — keep; add a one-line comment
  above the `use` stating the collision.
- **`#[cfg]`-gated path that cannot hoist** — keep; add a
  one-line comment stating the cfg reason.
- **Dead local** (body references the imported name only
  fully-qualified) — delete the `use`.
- **Plain misplaced** (none of the above) — hoist to the
  module's top-of-file `use` block (or to the top of the
  enclosing inline `mod` / test module's `use` block).

File-and-line targets (14):

- [x] `rlsp-yaml-parser/src/event_iter/base.rs:598` —
      `use std::borrow::Cow;`
- [x] `rlsp-yaml-parser/src/event_iter/flow.rs:47` —
      `use crate::lexer::scan_plain_line_flow;`
- [x] `rlsp-yaml-parser/src/event_iter/flow.rs:48` —
      `use std::borrow::Cow;`
- [x] `rlsp-yaml-parser/src/event_iter/properties.rs:26` —
      `use crate::chars::is_ns_anchor_char;`
- [x] `rlsp-yaml-parser/src/lexer.rs:500` —
      `use crate::chars::is_ns_anchor_char;`
- [x] `rlsp-yaml-parser/src/node.rs:433` —
      `use crate::event::CollectionStyle;`
- [x] `rlsp-yaml-parser/src/node.rs:452` —
      `use crate::event::CollectionStyle;`
- [x] `rlsp-yaml-parser/src/node.rs:522` —
      `use crate::event::CollectionStyle;`
- [x] `rlsp-yaml-parser/src/node.rs:541` —
      `use crate::event::CollectionStyle;`
- [x] `rlsp-yaml-parser/tests/schema_resolution.rs:703` —
      `use rlsp_yaml_parser::LoadError;`
- [x] `rlsp-yaml-parser/tests/schema_resolution.rs:720` —
      `use rlsp_yaml_parser::LoadError;`
- [x] `rlsp-yaml-parser/tests/schema_resolution.rs:735` —
      `use rlsp_yaml_parser::LoadError;`
- [x] `rlsp-yaml-parser/tests/smoke/directives.rs:508` —
      `use std::fmt::Write as _;`
- [x] `rlsp-yaml-parser/tests/smoke/directives.rs:523` —
      `use std::fmt::Write as _;`

**Verification:**

- [x] Every listed line is either removed, unchanged (with
      justifying comment if it was a keep-for-exception
      case), or has its `use` relocated to the enclosing
      module or test module's header `use` block.
- [x] `cargo fmt --check` clean.
- [x] `cargo clippy --all-targets -p rlsp-yaml-parser` zero
      warnings; workspace-wide clippy also zero warnings.
- [x] `cargo test -p rlsp-yaml-parser` passes; workspace
      tests still pass.
- [x] If the scanner at `/tmp/import-scan/` is still
      available, re-running it against `rlsp-yaml-parser/`
      reports zero `UseAtTopOfFnBody` violations that are
      not documented exceptions. Otherwise the verification
      is the per-line checkbox above plus clean clippy and
      tests — each listed location has been classified and
      acted upon.

### Task 3: Classify and fix fn-body `use` statements in `rlsp-yaml` (35 cases across 11 files)

Same classification procedure as Task 2, applied to every
listed line in `rlsp-yaml/`.

File-and-line targets (35):

- [ ] `rlsp-yaml/src/analysis/semantic_tokens.rs:187` —
      `use rlsp_yaml_parser::ScalarStyle;`
- [ ] `rlsp-yaml/src/document_store.rs:243` —
      `use rlsp_yaml_parser::node::Node;`
- [ ] `rlsp-yaml/src/document_store.rs:341` —
      `use rlsp_yaml_parser::node::Node;`
- [ ] `rlsp-yaml/src/editing/code_actions/flow_to_block.rs:750` —
      `use rlsp_yaml_parser::Span;`
- [ ] `rlsp-yaml/src/editing/code_actions/flow_to_block.rs:751` —
      `use rlsp_yaml_parser::node::Node;`
- [ ] `rlsp-yaml/src/editing/formatter.rs:1676` —
      `use std::collections::HashSet;`
- [ ] `rlsp-yaml/src/hover.rs:233` —
      `use std::fmt::Write;`
- [ ] `rlsp-yaml/src/hover.rs:310` —
      `use std::fmt::Write;`
- [ ] `rlsp-yaml/src/hover.rs:1589` —
      `use serde_json::json;`
- [ ] `rlsp-yaml/src/hover.rs:1611` —
      `use serde_json::json;`
- [ ] `rlsp-yaml/src/hover.rs:1628` —
      `use serde_json::json;`
- [ ] `rlsp-yaml/src/hover.rs:1657` —
      `use serde_json::json;`
- [ ] `rlsp-yaml/src/hover.rs:1678` —
      `use serde_json::json;`
- [ ] `rlsp-yaml/src/parser.rs:255` —
      `use rlsp_yaml_parser::node::Node;`
- [ ] `rlsp-yaml/src/parser.rs:291` —
      `use rlsp_yaml_parser::node::Node;`
- [ ] `rlsp-yaml/src/parser.rs:359` —
      `use rlsp_yaml_parser::node::Node;`
- [ ] `rlsp-yaml/src/schema.rs:374` —
      `use std::net::IpAddr;`
- [ ] `rlsp-yaml/src/schema.rs:473` —
      `use std::io::Read as _;`
- [ ] `rlsp-yaml/src/schema.rs:547` —
      `use std::io::Read as _;`
- [ ] `rlsp-yaml/src/schema.rs:1911` —
      `use std::io::Read as _;`
- [ ] `rlsp-yaml/src/schema_validation.rs:293` —
      `use rlsp_yaml_parser::ScalarStyle;`
- [ ] `rlsp-yaml/src/schema_validation.rs:784` —
      `use rlsp_yaml_parser::ScalarStyle;`
- [ ] `rlsp-yaml/src/schema_validation.rs:2741` —
      `use std::sync::{Arc, Mutex};`
- [ ] `rlsp-yaml/src/schema_validation.rs:4184` —
      `use crate::schema::parse_schema;`
- [ ] `rlsp-yaml/src/schema_validation.rs:4185` —
      `use serde_json::json;`
- [ ] `rlsp-yaml/src/schema_validation.rs:4212` —
      `use crate::schema::parse_schema;`
- [ ] `rlsp-yaml/src/schema_validation.rs:4213` —
      `use serde_json::json;`
- [ ] `rlsp-yaml/src/server.rs:1967` —
      `use tower_lsp::lsp_types::{DocumentRangeFormattingParams, FormattingOptions, TextDocumentIdentifier, WorkDoneProgressParams};`
- [ ] `rlsp-yaml/src/server.rs:2011` —
      `use tower_lsp::lsp_types::{DocumentRangeFormattingParams, FormattingOptions, TextDocumentIdentifier, WorkDoneProgressParams};`
- [ ] `rlsp-yaml/src/server.rs:2070` —
      `use tower_lsp::lsp_types::{DocumentFormattingParams, FormattingOptions, TextDocumentIdentifier, WorkDoneProgressParams};`
- [ ] `rlsp-yaml/src/server.rs:2160` —
      `use tower_lsp::lsp_types::{PartialResultParams, TextDocumentIdentifier, WorkDoneProgressParams};`
- [ ] `rlsp-yaml/src/server.rs:2186` —
      `use tower_lsp::lsp_types::{PartialResultParams, TextDocumentIdentifier, WorkDoneProgressParams};`
- [ ] `rlsp-yaml/tests/corpus_invariants.rs:1637` —
      `use std::fmt::Write as _;`
- [ ] `rlsp-yaml/tests/corpus_invariants.rs:1676` —
      `use rlsp_yaml_parser::{Pos, ScalarStyle};`
- [ ] `rlsp-yaml/tests/lsp_lifecycle.rs:78` —
      `use tower::Service;`

**Verification:**

- [ ] Every listed line is either removed, unchanged (with
      justifying comment if kept as an exception), or
      relocated to the enclosing module or test module's
      header `use` block.
- [ ] `cargo fmt --check` clean.
- [ ] `cargo clippy --all-targets -p rlsp-yaml` zero
      warnings; workspace-wide clippy also zero warnings.
- [ ] `cargo test -p rlsp-yaml` passes; workspace tests
      still pass.
- [ ] If the scanner at `/tmp/import-scan/` is still
      available, re-running it against the full workspace
      (`rlsp-fmt rlsp-yaml rlsp-yaml-parser`) reports
      **zero violations** across all kinds (other than
      `UseAtTopOfFnBody` entries that are documented
      exceptions — variant globs, collision resolvers with
      comments, cfg-gated with comments). Otherwise the
      verification is the aggregate of every per-line
      checkbox across Tasks 1, 2, and 3 being marked done
      plus workspace-wide `cargo clippy --all-targets` and
      `cargo test` clean.

## Decisions

- **Task slicing by work type + per-crate for fn-body
  audits.** Task 1 bundles all 27 module-scope violations
  across 7 files because the fix is purely mechanical
  (relocate to header) and reviewing them together is
  faster than three tiny per-crate commits. Tasks 2 and 3
  split the 49 fn-body cases per crate because each case
  needs classification judgment, and per-crate review is
  the right granularity to keep commits focused.
- **No Task 1 for "create the rule."** The rule file
  `.claude/rules/lang-rust-imports.md` was drafted by the
  lead during planning, edited by the user, and committed
  directly at `7ef996a` before this plan. This plan starts
  from that SHA and covers only the code cleanup.
- **No allowlist or skip list for fn-body `use`
  exceptions.** When a fn-body `use` is kept for a legitimate
  reason (variant glob, collision, cfg), the rule requires
  a justifying comment above the `use` — that comment IS
  the record. No central `KNOWN_FN_BODY_USES` list.
- **Scanner is disposable.** The scanner lives at
  `/tmp/import-scan/` and is a planning tool, not a
  maintained artifact. It does not ship to the workspace
  and is not a Task 1 or 4. Re-run it locally from `/tmp`
  when needed.
- **Dead-local deletion preferred over hoist.** If a
  fn-body `use` is dead (body uses fully-qualified names),
  delete it rather than hoisting — adding a module-level
  import that no call site depends on creates noise.
  Hoist only when doing so enables shortening at the call
  site or collapses multiple dead locals.
- **Per-crate clippy verification.** Each task runs
  `cargo clippy --all-targets -p <crate>` for targeted
  feedback plus workspace-wide clippy because the workspace
  enforces `warnings = "deny"` globally.

## Non-Goals

- Enforcing a particular order within `use` groups (rustfmt
  handles alphabetical; grouping by std / external / crate
  is the existing convention and stays as-is).
- Modifying the VS Code extension (TypeScript, not Rust).
- Adding a custom clippy lint, dylint, or CI grep check
  (rule enforcement is reviewer-driven).
- Adding a `project-sanity` report entry for import
  hygiene.
- Changing imports' actual content (names imported) — this
  is pure relocation and dead-import deletion.
- Changing `mod.rs` usage patterns or module boundaries —
  only `use` / `mod` statement placement is in scope.
- Modifying the scanner source at `/tmp/import-scan/` into
  a maintained workspace artifact.
