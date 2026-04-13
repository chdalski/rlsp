**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-13

## Goal

Reorganize the flat `rlsp-yaml/src/` directory (22 `.rs`
files) into domain-based module groups for better
navigability. Also convert existing `schema/mod.rs` and
`schema_validation/mod.rs` to named-file style for
consistency. This is a pure structural refactor — no logic
changes.

## Context

The `rlsp-yaml` crate has 22 source files flat in `src/`,
each implementing an LSP capability or utility. Despite
low coupling (18/22 modules have zero cross-module
imports), the flat layout makes it hard to scan and
understand at a glance.

The user chose grouping by LSP domain with named module
files (not `mod.rs`), per the project's `lang-rust.md`
convention.

**Target layout:**

```
src/
  lib.rs               (updated module declarations)
  main.rs              (unchanged)
  server.rs            (updated crate:: paths)
  parser.rs            (stays)
  document_store.rs    (stays)
  scalar_helpers.rs    (stays)
  hover.rs             (stays)
  completion.rs        (stays)
  navigation.rs        (new — declares submodules)
  navigation/
    references.rs      (moved)
    rename.rs          (moved)
  editing.rs           (new — declares submodules)
  editing/
    formatter.rs       (moved)
    on_type_formatting.rs (moved)
    code_actions.rs    (moved)
  analysis.rs          (new — declares submodules)
  analysis/
    symbols.rs         (moved)
    semantic_tokens.rs (moved)
    selection.rs       (moved)
    folding.rs         (moved)
  validation.rs        (new — declares submodules)
  validation/
    validators.rs      (moved)
    suppression.rs     (moved)
  decorators.rs        (new — declares submodules)
  decorators/
    color.rs           (moved)
    document_links.rs  (moved)
    code_lens.rs       (moved)
  schema.rs            (converted from schema/mod.rs)
  schema/
    association.rs     (stays)
  schema_validation.rs (converted from schema_validation/mod.rs)
  schema_validation/
    formats.rs         (stays)
```

**Cross-module imports that need path updates:**

Internal (`use crate::` in moved files):
- `editing/formatter.rs` → `use crate::server::YamlVersion` (unchanged — server stays at root)
- `analysis/symbols.rs` → `use crate::scalar_helpers::{self, PlainScalarKind}` (unchanged — scalar_helpers stays at root)
- `decorators/code_lens.rs` → `use crate::schema::JsonSchema` (unchanged)

`server.rs` uses fully qualified `crate::` paths throughout
(e.g., `crate::validators::validate_unused_anchors`). All
~30 of these need the new group prefix inserted (e.g.,
`crate::validation::validators::validate_unused_anchors`).

External consumers (tests + benches) that need updates:
- `tests/ecosystem_fixtures.rs` — imports `formatter`, `parser`, `validators`
- `tests/lsp_lifecycle.rs` — imports `server` (unchanged)
- `benches/latency.rs` — imports `completion`, `document_store`, `semantic_tokens`
- `benches/insight.rs` — imports `hover`, `references`, `selection`, `validators`
- `benches/hot_path.rs` — imports `formatter`, `parser`, `schema_validation`, `validators`
- `benches/fixtures/mod.rs` — imports `schema` (unchanged)

Doc comment in `document_links.rs` references `rlsp_yaml::document_links` — update to `rlsp_yaml::decorators::document_links`.

## Steps

- [x] Clarify grouping style and module file convention with user
- [ ] Create module groups, move files, update all paths
- [ ] Convert schema/ and schema_validation/ to named-file style
- [ ] Verify: cargo fmt, clippy, build, test, bench compile

## Tasks

### Task 1: Create module groups and update all paths

Move files into 5 new module groups and update every
import path.

- [ ] Create 5 group directories: `navigation/`, `editing/`, `analysis/`, `validation/`, `decorators/`
- [ ] Create 5 group module files with `pub mod` declarations
- [ ] Move 14 source files to their groups via `git mv`
- [ ] Update `lib.rs`: replace 14 flat module declarations with 5 group modules
- [ ] Update `server.rs`: insert group prefix in all ~30 `crate::` paths
- [ ] Update `schema_validation/mod.rs`: `crate::scalar_helpers` path is unchanged (stays at root)
- [ ] Update doc comment in `document_links.rs` referencing `rlsp_yaml::document_links`
- [ ] Update test imports in `tests/ecosystem_fixtures.rs`
- [ ] Update bench imports in `benches/latency.rs`, `benches/insight.rs`, `benches/hot_path.rs`
- [ ] Verify: `cargo fmt && cargo clippy --all-targets && cargo test && cargo build`

### Task 2: Convert schema and schema_validation to named-file style

Convert the two existing subdirectory modules from
`mod.rs` to named module files for consistency with the
new groups.

- [ ] `git mv schema/mod.rs` → `schema.rs` (as the parent module file)
- [ ] `git mv schema_validation/mod.rs` → `schema_validation.rs`
- [ ] Verify: `cargo fmt && cargo clippy --all-targets && cargo test`

## Decisions

- **hover.rs and completion.rs stay top-level** — they're
  the two largest files (2,087 and 2,701 lines) and each
  maps 1:1 to a major LSP capability. Nesting them would
  reduce navigability.
- **Named module files over mod.rs** — follows the
  project's `lang-rust.md` convention and shows module
  names in editor tabs.
- **No re-exports at crate root** — external paths change
  (e.g., `rlsp_yaml::validators::` →
  `rlsp_yaml::validation::validators::`). Since this is
  an AI-only project with no external consumers beyond our
  own tests and benches, we update consumers rather than
  adding compatibility re-exports.
- **No advisor consultation needed** — pure refactor with
  no behavior changes, no trust boundary changes, and
  existing test patterns to follow.
