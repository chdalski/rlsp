**Repository:** root
**Status:** InProgress
**Created:** 2026-04-13

## Goal

Improve documentation quality, coverage visibility, and
maintainability across the workspace through three
initiatives: (1) enable documentation lints as compile-time
guardrails, (2) add missing `docs/` directory content for
the parser and formatter, and (3) identify and close code
coverage gaps. Optionally wire VS Code extension coverage
into Codecov if the effort is small.

## Context

- **Workspace lint config** lives in root `Cargo.toml`
  `[workspace.lints]`. All crates inherit via
  `lints.workspace = true`. The workspace already enforces
  `warnings = "deny"`, so any new `warn`-level lint
  becomes a hard error immediately.
- **Current doc lint findings** (without
  `missing_docs_in_private_items`, which is excluded per
  user request):
  - `rlsp-fmt`: 4 errors — 2 undocumented `pub mod`
    (`ir`, `printer` in `lib.rs:34-35`), 2 struct fields
    (`ir.rs:25` — `FlatAlt` variant fields)
  - `rlsp-yaml-parser`: 37 errors — 1 crate-level doc,
    2 module docs (`lib.rs:4,9`), 5 enum variants
    (`encoding.rs:6-10`), 29 struct fields across
    `error.rs`, `loader.rs`, `node.rs`, `pos.rs`
  - `rlsp-yaml`: 0 errors — already clean
  - `missing_errors_doc` and `missing_panics_doc`: 0
    findings — these are pure guardrails for future code
- **Docs directories:**
  - `rlsp-fmt/docs/` does not exist
  - `rlsp-yaml-parser/docs/` has only `benchmarks.md`
  - `rlsp-yaml/docs/` has `configuration.md` +
    `feature-log.md` (complete)
- **Coverage:** Rust coverage runs in CI via
  `cargo-llvm-cov` → Codecov. VS Code extension has no
  coverage configured (vitest supports v8 natively but
  it's not wired up). The `codecov.yml` defines
  `patch: 80%` target and tracks `rlsp-yaml` as a
  separate component.
- **VS Code extension coverage feasibility:** Adding
  coverage requires: (a) `@vitest/coverage-v8` dev dep,
  (b) `coverage` section in `vitest.config.ts`,
  (c) CI step to generate LCOV, (d) upload to Codecov
  with a flag. This is ~4 config changes — small enough
  to include in this plan.

## Steps

- [x] Enable `missing_docs`, `clippy::missing_errors_doc`,
      `clippy::missing_panics_doc` in workspace lints
      (8757a28)
- [x] Fix all 41 lint errors (4 in rlsp-fmt, 37 in
      rlsp-yaml-parser) (8757a28)
- [x] Add `docs/architecture.md` for rlsp-yaml-parser
      (3955b2a)
- [x] Add `docs/feature-log.md` for rlsp-yaml-parser
      (5aa4b8b)
- [x] Add `docs/feature-log.md` for rlsp-fmt
      (5aa4b8b)
- [x] Check Codecov coverage for each crate, identify gaps
      (668a75b)
- [x] Add tests to close coverage gaps
      (668a75b)
- [x] Wire VS Code extension unit test coverage into
      Codecov (b6f8a5d)
- [x] Cross-link new docs from crate READMEs (a806dca)

## Tasks

### Task 1: Enable doc lints and fix all errors ✅ (8757a28)

Add three lints to root `Cargo.toml` `[workspace.lints]`
and fix all resulting errors so the workspace compiles
cleanly.

**Lints to add:**

In `[workspace.lints.rust]`:
```toml
missing_docs = "warn"
```

In `[workspace.lints.clippy]`:
```toml
missing_errors_doc = "warn"
missing_panics_doc = "warn"
```

**Errors to fix (41 total):**

`rlsp-fmt` (4 errors):
- `src/lib.rs:34-35` — add module-level docs for
  `pub mod ir` and `pub mod printer`
- `src/ir.rs:25` — add field docs for `FlatAlt` variant's
  two fields

`rlsp-yaml-parser` (37 errors):
- `src/lib.rs:3` — add crate-level doc comment (`//!`)
- `src/lib.rs:4,9` — add module-level docs for 2 public
  module re-exports
- `src/encoding.rs:6-10` — add docs for 5 `Encoding` enum
  variants
- `src/error.rs:9-10` — add docs for 2 struct fields
- `src/loader.rs:59-83` — add docs for 7 struct fields
  (likely `Node` or loader output types)
- `src/node.rs:34-72` — add docs for 14 struct fields
  (AST node types)
- `src/pos.rs:8-10,72-73` — add docs for 5 struct fields
  (`Pos` and `Span`)

**Acceptance criteria:**
- `cargo clippy --all-targets` passes with zero warnings
- All three lints are present in `[workspace.lints]`
- Doc comments are meaningful (not "The X field" stubs)

### Task 2: Add docs/architecture.md for rlsp-yaml-parser ✅ (3955b2a)

Write a design document explaining the parser's internal
architecture. This is a reference document for contributors
and future agents working on the parser.

**Content should cover:**
- Streaming architecture — why events, not a tree
- O(1) first-event latency design and how it's achieved
- State machine structure (event iterator, step dispatcher)
- Separation of concerns: scanner/lexer → event iterator →
  loader (optional AST)
- Comment attachment strategy
- Security limits (nesting depth, anchor count, expansion
  limit)
- Key design decisions and trade-offs (spec-faithfulness
  vs raw speed, zero-copy where possible)

**Source material:** The code itself (`src/event_iter/`,
`src/loader.rs`, `src/scanner/`), the existing
`docs/benchmarks.md`, and the README.

**Acceptance criteria:**
- File exists at `rlsp-yaml-parser/docs/architecture.md`
- Accurately reflects the current implementation (verified
  by reading source)
- Written for a technical audience (contributor or AI agent)

### Task 3: Add docs/feature-log.md for both crates ✅ (5aa4b8b)

Add feature decision logs to both `rlsp-yaml-parser` and
`rlsp-fmt`, following the format established by
`rlsp-yaml/docs/feature-log.md`.

**rlsp-yaml-parser/docs/feature-log.md:**
- Document implemented capabilities (streaming events,
  AST loading, YAML 1.2 conformance, YAML 1.1 compat
  mode, security limits, encoding detection, anchors/
  aliases, tag resolution, block scalar chomping, etc.)
- Document any won't-implement decisions with rationale

**rlsp-fmt/docs/feature-log.md:**
- Document implemented capabilities (Wadler-Lindig
  algorithm, flat/break mode, indentation, group
  optimization, FlatAlt, join combinator, etc.)
- Document any won't-implement decisions with rationale

**Source material:** Read `rlsp-yaml/docs/feature-log.md`
for the format template. Read each crate's source and
git history for content.

**Acceptance criteria:**
- Both files exist at their respective `docs/` paths
- Format matches `rlsp-yaml/docs/feature-log.md` style
- Content is accurate and reflects current capabilities

### Task 4: Identify and close Rust code coverage gaps ✅ (668a75b)

Check current Codecov coverage for each crate, identify
the largest gaps, and add tests to close them.

**Process:**
1. Check current coverage on Codecov:
   `https://app.codecov.io/gh/chdalski/rlsp`
   Identify per-file and per-crate coverage percentages.
2. Identify files with the lowest coverage percentages
3. Prioritize: public API paths > error handling paths >
   internal utilities
4. Add tests targeting uncovered branches

**Acceptance criteria:**
- Coverage report generated and gaps documented in the
  commit message
- Tests added for the most impactful uncovered paths
- All new and existing tests pass
- No decrease in overall coverage percentage

### Task 5: Wire VS Code extension coverage into Codecov ✅ (b6f8a5d)

Add vitest coverage collection and upload it to Codecov
alongside the existing Rust coverage.

**Changes needed:**

1. Add `@vitest/coverage-v8` dev dependency:
   ```
   cd rlsp-yaml/integrations/vscode && pnpm add -D @vitest/coverage-v8
   ```

2. Update `vitest.config.ts` to include coverage config:
   ```typescript
   coverage: {
     provider: 'v8',
     reporter: ['lcov', 'text'],
     reportsDirectory: './coverage',
   }
   ```

3. Add `test:coverage` script to `package.json`:
   ```json
   "test:coverage": "vitest run --coverage"
   ```

4. Update `.github/workflows/coverage.yml` to add a
   vscode coverage job:
   - Install pnpm + Node.js
   - Run `pnpm run test:coverage`
   - Upload `coverage/lcov.info` to Codecov with a
     `vscode` flag

5. Update `codecov.yml` to add the vscode component:
   ```yaml
   - component_id: vscode-extension
     paths:
       - rlsp-yaml/integrations/vscode/src/**
   ```

**Acceptance criteria:**
- `pnpm run test:coverage` generates LCOV report locally
- Coverage workflow includes vscode extension upload
- Codecov config tracks vscode as a separate component

### Task 6: Cross-link new docs from crate READMEs ✅ (a806dca)

Update `rlsp-yaml-parser/README.md` and `rlsp-fmt/README.md`
to link to the new `docs/` files (architecture.md,
feature-log.md) so users discover them from the README.

**Acceptance criteria:**
- Parser README links to `docs/architecture.md` and
  `docs/feature-log.md`
- Formatter README links to `docs/feature-log.md`
- Links are in a natural location (e.g. a Documentation
  section or inline where relevant)

## Decisions

- **Exclude `clippy::missing_docs_in_private_items`** —
  user decision; ~60+ private implementation items would
  need docs with limited external value
- **Skip `docs/settings.md` for rlsp-fmt** — the only
  settings are `FormatOptions` (3 fields), already
  documented in README and rustdoc
- **Architecture doc for parser only** — rlsp-fmt's
  Wadler-Lindig implementation is simple enough that
  rustdoc and README suffice; the parser's streaming
  state machine architecture warrants dedicated
  documentation
- **Feature logs for both crates** — tracks design
  decisions and capabilities consistently across the
  workspace
- **Include VS Code coverage in this plan** — the effort
  is ~4 config changes, not enough to warrant a separate
  plan
