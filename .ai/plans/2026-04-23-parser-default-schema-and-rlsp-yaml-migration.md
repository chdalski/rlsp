**Repository:** root
**Status:** InProgress
**Created:** 2026-04-23

# Make Core schema the loader default and migrate rlsp-yaml

## Goal

Make `Schema::Core` the loader's default so every `load()`
call produces resolved tag URIs — the spec-conformant
behavior. Drop `Option<Schema>` from `LoaderOptions` (a
plain `Schema` field replaces it; `Failsafe` covers "don't
care about types"). Migrate `rlsp-yaml` to read tag URIs
from the AST instead of re-running its own Core regex table,
and thin `scalar_helpers.rs` to only the functions that
cannot be replaced by tag comparisons.

## Context

- **Plan 1 (committed):** `.ai/plans/2026-04-23-parser-schema-resolution.md`
  added opt-in `Schema::Core` resolution via
  `LoaderBuilder::schema()` and `load_with_schema()`. The
  default `load()` still produces `tag: None`.
- **Current `LoaderOptions`:** `schema: Option<Schema>`,
  default `None`. Callers that want resolution must opt in.
- **`scalar_helpers.rs` in `rlsp-yaml`:** duplicates the
  Core schema regex table. Functions split into two groups:
  - *Type classification* — `classify_plain_scalar`,
    `is_null`, `is_bool`, `is_integer`, `is_float`. These
    become redundant once the AST carries resolved tags.
  - *Value parsing* — `parse_integer`, `parse_float`. These
    extract actual numeric values for JSON Schema range
    validation (min/max/multipleOf). Tags don't provide
    values, so these stay.
  - *YAML 1.1 compatibility* — `is_yaml11_bool`,
    `yaml11_bool_canonical`, `is_yaml11_octal`. Unrelated
    to Core schema resolution; these stay.
- **Callsites of type classification functions:**
  - `schema_validation.rs:795-798` — `!is_null && !is_bool
    && !is_integer && !is_float` → becomes
    `tag == "tag:yaml.org,2002:str"`
  - `schema_validation.rs:1543` — `classify_plain_scalar()`
    match → becomes tag-URI match
  - `schema_validation.rs:1603-1612` — `is_null`/`is_bool`
    then `parse_integer`/`parse_float` → tag check for
    null/bool, `parse_integer`/`parse_float` stay for value
    extraction
  - `symbols.rs:159` — `classify_plain_scalar()` →
    tag-URI match for document symbols
  - `validators.rs:373` — `!is_null(value)` → tag check
- **Callsites that need `style` info:** Some callsites
  check `style == ScalarStyle::Plain` before calling type
  inference. After migration, `tag` already encodes the
  resolution result (quoted scalars get `!!str`), so the
  style check is redundant for type inference — but it may
  still be needed for other purposes (YAML 1.1 warnings
  only apply to plain scalars).
- **`parse_yaml()` in `parser.rs:24`:** production entry
  point using `LoaderBuilder::new().lossless()
  .max_nesting_depth(256).build().load(text)`. Does NOT
  call `.schema()`. After this plan, it doesn't need to —
  the default is Core.
- **Integration tests in `schema_resolution.rs`:**
  IT-25/26/27 assert `tag: None` under default `load()`.
  These become invalid once the default resolves tags.
- **`load_with_schema()` convenience function:** becomes
  redundant — `load()` resolves by default, and callers
  wanting a different schema use
  `LoaderBuilder::schema(s)`.
- **Tests across the codebase** that assert `tag: None`
  on loaded nodes: these all need updating to expect the
  resolved tag URI.
- **`rlsp-yaml` bench files** (`latency.rs`, `insight.rs`,
  `hot_path.rs`): call `rlsp_yaml_parser::load()`. The
  default change means benchmarks now include resolution
  overhead — this is the realistic workload since
  production will always resolve.

## Non-Goals

- Adding typed values (`Value::Int(42)` etc.) — tags are
  URI strings, not typed representations
- Custom schema support — only the three spec schemas
- Removing `parse_integer` / `parse_float` /
  `is_yaml11_bool` / `yaml11_bool_canonical` /
  `is_yaml11_octal` from `scalar_helpers.rs` — these serve
  purposes that tags don't cover

## Steps

- [x] Task 1 — make `Schema::Core` the loader default and
      drop `Option<Schema>`
- [x] Task 2 — migrate `rlsp-yaml` type-inference callsites
      to tag-URI comparisons
- [x] Task 3 — thin `scalar_helpers.rs` and update docs

## Tasks

### Task 1: Make `Schema::Core` the loader default and drop `Option<Schema>`

Change `LoaderOptions.schema` from `Option<Schema>` to
`Schema` with default `Schema::Core`. Remove
`load_with_schema()` (redundant). Update all parser-crate
tests.

- [x] In `loader.rs`: change `schema: Option<Schema>` to
      `schema: Schema` in `LoaderOptions`. Default to
      `Schema::Core` in `Default` impl.
- [x] In `loader.rs`: update `LoaderBuilder::schema()` to
      take `Schema` directly (drop the `Option` wrapping).
- [x] In `loader.rs`: update `apply_schema_to_node` call
      site — currently guarded by
      `if self.options.schema.is_some()`; change to always
      call (schema is always present).
- [x] In `loader.rs`: remove `load_with_schema()` function.
- [x] In `lib.rs`: remove `load_with_schema` from
      `pub use loader::{ ... }` re-export.
- [x] In `schema_resolution.rs`: delete IT-25/26/27
      (regression tests for `tag: None` under default
      `load()`). Update IT-28/29 (convenience-vs-builder
      equivalence) to use `LoaderBuilder::schema()` instead
      of `load_with_schema()`.
- [x] Update `tests/robustness.rs`: calls to `load()` and
      `LoaderBuilder::new()...build().load()` now produce
      resolved tags — update any assertions on `tag` fields.
- [x] Update `tests/loader_spans.rs`: same — update tag
      assertions if present.
- [x] Update `tests/encoding.rs`: same.
- [x] Update `tests/implicit_key_length.rs`: same.
- [x] Update `rlsp-yaml-parser/docs/feature-log.md`: the
      §10 Schema Resolution entry currently describes
      `load_with_schema()` as the API and states `load()`
      preserves `tag: None`. Replace with: `Schema::Core`
      is the default for `load()`, `Failsafe`/`JSON`/`Core`
      are selectable via `LoaderBuilder::schema()`,
      `parse_events()` is the path for raw representation.
- [x] Update `rlsp-yaml-parser/docs/yaml-spec-conformance.md`:
      six §10 entries reference `load_with_schema()` or
      claim `tag: None` for untagged AST nodes — both
      become false after this task. Specifically:
      - Remove `load_with_schema(...)` from Implementation
        fields of the Failsafe `!`, JSON plain scalar, and
        Core plain scalar entries. Replace with
        `LoaderBuilder::schema()`.
      - Update the Failsafe `?` entry: clarify `tag: None`
        is the event-stream representation; the default
        loader now resolves via Core. Callers wanting
        `tag: None` in the AST use
        `LoaderBuilder::schema(Schema::Failsafe)` or
        `parse_events()`.
      - Update the `!!seq` and `!!str` entries: untagged
        nodes receive a resolved tag URI from Core by
        default; `tag: None` remains accurate only for the
        event stream.
- [x] `cargo test -p rlsp-yaml-parser` passes.
- [x] `cargo test --workspace` passes (rlsp-yaml's
      `parser.rs` already handles `LoadError::UnresolvedScalar`;
      other rlsp-yaml code reads `value` not `tag`, so it
      should compile — but tests may assert `tag: None`).
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

**Commit:** `60469fb`

### Task 2: Migrate rlsp-yaml type-inference callsites to tag-URI comparisons

Replace `scalar_helpers` type-classification calls with
tag-URI string comparisons on the AST node's `tag` field.

- [x] In `schema_validation.rs`: replace
      `classify_plain_scalar(value)` at line ~1543 with a
      match on `tag.as_deref()`:
      `Some("tag:yaml.org,2002:null")` → `"null"`,
      `Some("tag:yaml.org,2002:bool")` → `"boolean"`,
      `Some("tag:yaml.org,2002:int")` → `"integer"`,
      `Some("tag:yaml.org,2002:float")` → `"number"`,
      `_` → `"string"`.
- [x] In `schema_validation.rs`: replace the
      `!is_null && !is_bool && !is_integer && !is_float`
      check at line ~795 with
      `tag.as_deref() == Some("tag:yaml.org,2002:str")`.
- [x] In `schema_validation.rs`: replace `is_null(value)`
      and `is_bool(value)` at lines ~1603-1605 with tag
      comparisons. Keep `parse_integer` / `parse_float`
      calls at lines ~1610-1612 for value extraction.
- [x] In `analysis/symbols.rs`: replace
      `classify_plain_scalar(value)` at line ~159 with a
      match on `tag.as_deref()` for `SymbolKind` mapping.
      The destructuring pattern needs to add `tag` to the
      `Node::Scalar { value, .. }` arm.
- [x] In `validation/validators.rs`: replace
      `!is_null(value)` at line ~373 with
      `tag.as_deref() != Some("tag:yaml.org,2002:null")`.
- [x] Verify the `style` checks in YAML 1.1 warning code
      still work correctly — those check
      `style == ScalarStyle::Plain` before calling
      `is_yaml11_bool(value)`. These are not affected by
      tag migration since they test 1.1-specific forms,
      not Core schema types.
- [x] `cargo test -p rlsp-yaml` passes.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

**Commit:** `08a84fd`

### Task 3: Thin `scalar_helpers.rs` and update docs

Remove the now-unused type-classification functions from
`scalar_helpers.rs`. Update the module doc and the
follow-up queue.

- [x] Remove `classify_plain_scalar`, `PlainScalarKind`,
      `is_null`, `is_bool`, `is_integer`, `is_float` from
      `scalar_helpers.rs`. Keep `parse_integer`,
      `parse_float`, `is_yaml11_bool`,
      `yaml11_bool_canonical`, `is_yaml11_octal`.
- [x] Update the module doc comment — it currently says
      "Scalar type inference helpers for YAML 1.2 Core
      schema." Change to reflect the remaining scope (value
      parsing + YAML 1.1 compatibility).
- [x] Remove the unit tests for deleted functions from the
      `#[cfg(test)]` module in `scalar_helpers.rs`.
- [x] Verify no remaining imports of the deleted items
      anywhere in the workspace.
- [x] Update the follow-up queue: remove the Plan 2 bullet
      from `.ai/memory/project_followup_plans.md`.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

**Commit:** `a21a07f`

## Decisions

- **Default `Schema::Core`, not `Failsafe`.** Core is the
  spec's recommended default (§10.3). Failsafe is available
  for callers that want no type inference.
- **Remove `load_with_schema()`.** It was a convenience for
  the opt-in era. With Core as default, `load()` already
  resolves. Callers wanting Failsafe or JSON use
  `LoaderBuilder::schema(s)`.
- **Keep `parse_integer` / `parse_float`.** Tags tell you
  the type; these tell you the value. JSON Schema
  validation needs both (e.g., `minimum: 0` needs the
  actual integer to compare).
- **Keep YAML 1.1 helpers.** `is_yaml11_bool`,
  `yaml11_bool_canonical`, `is_yaml11_octal` detect 1.1
  forms for compatibility warnings — orthogonal to Core
  schema resolution.
- **Three tasks.** Task 1 (parser default change) is
  independently testable and lands the breaking API change.
  Task 2 (rlsp-yaml migration) replaces callsites. Task 3
  (cleanup) removes dead code and closes the follow-up.
