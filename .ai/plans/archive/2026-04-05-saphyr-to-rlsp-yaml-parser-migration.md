**Repository:** root
**Status:** Completed (2026-04-05)
**Created:** 2026-04-05

## Goal

Migrate `rlsp-yaml` from saphyr/saphyr-parser to
`rlsp-yaml-parser` as its YAML parsing backend. This
removes the two saphyr dependencies, eliminates the 6
documented workarounds (discarded comments, zero container
spans, eager alias resolution, lost chomping indicators,
silent key deduplication, duplicate key text scanning),
and gives the language server a 100%-conformant parser with
first-class comments and spans. Then wire up `contentSchema`
validation as the final step.

## Context

- `rlsp-yaml-parser` is complete: 351/351 conformance,
  1,394 tests, full API (tokenize, events, AST, schema,
  emitter)
- saphyr is used in 11 source files across `rlsp-yaml/src/`
- Key type mappings:
  - `YamlOwned` → `Node<Span>`
  - `ScalarOwned::String/Integer/Float/Bool/Null` →
    `Node::Scalar { value: String, style }` (all scalars
    are strings; infer type via schema or parsing)
  - `MarkedYamlOwned` → `Node<Span>` (spans always present)
  - `Marker` → `Pos` (line 1-based, column 0-based)
  - `MappingOwned` → `Vec<(Node, Node)>` inside
    `Node::Mapping { entries }`
  - `SequenceOwned` → `Vec<Node>` inside
    `Node::Sequence { items }`
  - `LoadableYamlNode::load_from_str()` → `load()`
  - `saphyr_parser::Parser/BufferedInput` → not needed
    (only used in formatter for comment extraction;
    comments now come from `Document.comments`)
- Scalar type inference needed: saphyr provides typed
  scalars (`ScalarOwned::Integer(42)`), our parser gives
  strings (`"42"`). ~15 sites need helper functions to
  infer type from string content.
- Key files:
  - `rlsp-yaml/Cargo.toml` — dependencies
  - `rlsp-yaml/src/parser.rs` — entry point wrapper
  - `rlsp-yaml/src/document_store.rs` — stores parsed ASTs
  - `rlsp-yaml/src/formatter.rs` — comment extraction +
    scalar formatting (most complex)
  - `rlsp-yaml/src/schema_validation.rs` — heavy pattern
    matching (5,850 lines)
  - `rlsp-yaml/src/selection.rs` — Marker→Pos conversion
  - `rlsp-yaml/src/validators.rs`,
    `hover.rs`, `symbols.rs`, `completion.rs`, `schema.rs`,
    `server.rs` — pattern matching throughout

## Steps

- [x] Add rlsp-yaml-parser dependency and create type
      helpers
- [x] Migrate parser.rs, document_store.rs, server.rs
      (entry points)
- [x] Migrate symbols.rs, hover.rs, completion.rs
- [x] Migrate validators.rs, schema.rs, schema_validation.rs
- [x] Migrate selection.rs and formatter.rs
- [x] Remove saphyr dependencies, update tests, clean up
- [x] Wire up contentSchema validation

## Tasks

### Task 1: Foundation — dependency + helpers + entry points

Add `rlsp-yaml-parser` as a path dependency and create
shared helper functions. Migrate `parser.rs`,
`document_store.rs`, and `server.rs` — the entry points
that load YAML and store the AST.

- [x] Add `rlsp-yaml-parser = { path = "../rlsp-yaml-parser" }`
      to `[dependencies]` in `rlsp-yaml/Cargo.toml`
- [x] Create a type alias or re-export module for the new
      types (`Node`, `Span`, `Pos`, `Document`, `ScalarStyle`)
- [x] Create `scalar_type_helpers` — functions to infer
      scalar type from string content (null, bool, int,
      float, string) using the same rules as
      `rlsp-yaml-parser::schema::CoreSchema`
- [x] Migrate `parser.rs`: replace `YamlOwned::load_from_str`
      with `rlsp_yaml_parser::load`, update error handling
      from `Marker` to `Pos`, update `ParseResult` to use
      `Node<Span>` (or `Document<Span>`)
- [x] Migrate `document_store.rs`: replace `YamlOwned` and
      `MarkedYamlOwned` storage with `Document<Span>` /
      `Node<Span>`
- [x] Migrate `server.rs`: update type signatures

**Files:** `Cargo.toml`, `parser.rs`, `document_store.rs`,
`server.rs`, new helper module

**Commit:** a4af8bc

### Task 2: LSP features — symbols, hover, completion

Migrate the three LSP feature files. These all do pattern
matching on `YamlOwned` variants.

- [x] Migrate `symbols.rs`: replace `YamlOwned::Value(
      ScalarOwned::String(s))` with `Node::Scalar { value, .. }`,
      replace `YamlOwned::Mapping(map)` with
      `Node::Mapping { entries, .. }`, etc.
- [x] Migrate `hover.rs`: same pattern matching changes +
      update scalar type display to use inferred types
- [x] Migrate `completion.rs`: same pattern matching changes

**Files:** `symbols.rs`, `hover.rs`, `completion.rs`

**Commit:** 8b0d3d9

### Task 3: Validation — validators, schema, schema_validation

Migrate the validation files. These are the heaviest
pattern matching users. `schema_validation.rs` (5,850 lines)
is the largest file and uses numeric type inference
extensively.

- [x] Migrate `validators.rs`: pattern matching refactor
- [x] Migrate `schema.rs`: pattern matching + type
      introspection changes
- [x] Migrate `schema_validation.rs`: extensive pattern
      matching + replace `ScalarOwned::Integer(i)` /
      `ScalarOwned::FloatingPoint(f)` with string parsing
      via helper functions

**Files:** `validators.rs`, `schema.rs`,
`schema_validation.rs`

**Commit:** 906ce2e

### Task 4: Selection and formatter

Migrate the two most structurally different files.
`selection.rs` converts Markers to LSP ranges throughout.
`formatter.rs` uses low-level saphyr_parser APIs for
comment extraction and handles scalar formatting by type.

- [x] Migrate `selection.rs`: replace all `Marker` method
      calls (`.line()`, `.col()`) with `Pos` field access
      (`.line`, `.column`), replace `MarkedYamlOwned` with
      `Node<Span>`, replace span computation workarounds
      with native span access
- [x] Migrate `formatter.rs`: replace saphyr_parser comment
      extraction with `Document.comments`, replace
      `ScalarOwned` formatting switch with string-based
      formatting, remove `YamlLoader` usage

**Files:** `selection.rs`, `formatter.rs`

**Commit:** 6dce350

### Task 5: Remove saphyr and clean up

Remove saphyr dependencies entirely and fix all remaining
references.

- [x] Remove `saphyr = "0.0.6"` from `[dependencies]`
- [x] Remove `saphyr-parser = "0.0.6"` from `[dependencies]`
- [x] Grep entire crate for remaining `saphyr` references
      (imports, comments, docs) and remove/update them
- [x] Update `tests/ecosystem_fixtures.rs` to use new types
- [x] Run full test suite: all existing tests must pass
- [x] `cargo clippy --all-targets` zero warnings
- [x] `cargo fmt --check` clean
- [x] Update `CLAUDE.md` Components table if needed
- [x] Update `rlsp-yaml/README.md` to reference
      rlsp-yaml-parser instead of saphyr

**Files:** `Cargo.toml`, all source files, README,
CLAUDE.md

**Commit:** 20218c5

### Task 6: Wire up contentSchema validation

Add sub-schema validation for decoded content. The
decoding infrastructure exists (`data-encoding` crate,
`contentEncoding`/`contentMediaType` validation in
`schema_validation.rs`). The missing piece: after decoding
content, validate the decoded result against the
`contentSchema` if one is specified.

- [x] Add `content_schema: Option<Box<Self>>` field to
      `JsonSchema` struct in `schema.rs`
- [x] Parse `"contentSchema"` keyword in
      `parse_schema_with_root`
- [x] After successful content decoding in
      `schema_validation.rs`, if `content_schema` is
      present, parse the decoded content as YAML and
      validate it against the sub-schema
- [x] Unit tests: contentSchema with base64-encoded JSON,
      contentSchema with plain text, contentSchema
      validation failure
- [x] Verify `cargo clippy` and `cargo test` pass

**Files:** `schema.rs`, `schema_validation.rs`

**Commit:** a26559b

## Decisions

- **Migrate incrementally by file group.** Each task
  touches a specific set of files and can be committed
  independently. The crate compiles at each stage because
  both saphyr and rlsp-yaml-parser can coexist as
  dependencies during the transition.
- **Dual dependency during migration.** Both saphyr and
  rlsp-yaml-parser are in Cargo.toml during Tasks 1-4.
  Task 5 removes saphyr after all code is migrated. This
  avoids a big-bang switchover that breaks everything at
  once.
- **Scalar type inference via helper functions.** Rather
  than adding a typed scalar enum to rlsp-yaml-parser
  (which would duplicate schema resolution), create helper
  functions in rlsp-yaml that infer types from string
  content when needed. This keeps the parser's API clean
  (scalars are strings) while giving the LSP the type
  information it needs.
- **contentSchema in the same plan.** It's a small addition
  (3 code changes + tests) that fits naturally after the
  migration since it touches the same schema validation
  code.
