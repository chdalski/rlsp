**Repository:** root
**Status:** Completed (2026-04-23)
**Created:** 2026-04-23

# Add opt-in schema tag resolution to the loader (§10)

## Goal

Add opt-in YAML 1.2.2 §10 schema tag resolution to
`rlsp-yaml-parser`'s loader so that callers can request
Failsafe, JSON, or Core schema resolution and receive AST
nodes with resolved tag URIs. Today, untagged nodes carry
`tag: None` and the bare `!` non-specific tag is preserved
as the literal string `"!"`; after this plan, a caller
that passes `Schema::Core` gets every node's tag resolved
to a concrete URI (`tag:yaml.org,2002:int`,
`tag:yaml.org,2002:str`, etc.) per the spec's §10 rules.

The default `load()` behavior is unchanged — callers that
do not request schema resolution see the same `tag: None`
they see today. This closes the four Lenient §10 findings
and the one Not Implemented finding in the conformance
audit without breaking any existing consumer.

## Context

- **Conformance audit** (`rlsp-yaml-parser/docs/yaml-spec-conformance.md`):
  five §10 entries are not Conformant —
  - Not Implemented: Failsafe `!` non-specific tag resolution
    (bare `!` stored as `"!"`, not resolved by kind)
  - Lenient ×4: JSON-schema plain-scalar type inference,
    JSON-schema untagged-collection resolution,
    Core-schema plain-scalar type inference,
    Core-schema untagged-collection resolution

- **Spec text** (cached at `.ai/references/yaml-1.2.2-spec.md`):
  - §10.1 Failsafe: `!` resolves to `!!str`/`!!seq`/`!!map`
    by kind; `?` left unresolved.
  - §10.2 JSON: plain scalars matched against regex table
    (null, bool, int, float); no-match → error. Untagged
    collections → `!!seq`/`!!map` by kind.
  - §10.3 Core (recommended default): extended regex table
    (null/Null/NULL/~, true/True/TRUE, octal, hex, inf,
    nan); no-match → `!!str` fallback. Untagged collections
    → `!!seq`/`!!map` by kind.

- **Resolution rules by node type and schema:**

  | Node type | Source tag | Failsafe | JSON | Core |
  |---|---|---|---|---|
  | Plain scalar | `None` (`?`) | `!!str` | regex table; error if no match | regex table; `!!str` fallback |
  | Quoted/block scalar | `None` (`?`) | `!!str` | `!!str` | `!!str` |
  | Any scalar | `Some("!")` | `!!str` | `!!str` | `!!str` |
  | Sequence | `None` | `!!seq` | `!!seq` | `!!seq` |
  | Sequence | `Some("!")` | `!!seq` | `!!seq` | `!!seq` |
  | Mapping | `None` | `!!map` | `!!map` | `!!map` |
  | Mapping | `Some("!")` | `!!map` | `!!map` | `!!map` |
  | Any node | explicit tag | unchanged | unchanged | unchanged |

  Here `!!str` = `"tag:yaml.org,2002:str"`, etc.

- **Existing loader API:**
  - `LoaderBuilder::new().lossless().build().load(input)` is
    the current chain. `LoaderOptions` has `max_nesting_depth`,
    `max_anchors`, `max_expanded_nodes`, `mode: LoadMode`.
  - The builder pattern is the natural extension point — a
    `.schema(Schema::Core)` method adds the option without
    changing existing call sites.

- **Existing `scalar_helpers.rs` in `rlsp-yaml`:**
  `rlsp-yaml/src/scalar_helpers.rs` already contains a
  `classify_plain_scalar` function that runs the full Core
  schema regex table (`is_null`, `is_bool`, `is_integer`,
  `is_float`). The parser's implementation must be
  independent (separate crate), but the regex patterns are
  the same — the spec's tables are the source of truth for
  both. After this plan lands, a follow-up plan (Plan 2)
  will migrate `rlsp-yaml` to read tags from the AST and
  retire the duplicated regex table.

- **Tag representation:** resolved tags are stored as
  `Option<String>` on `Node` — same field, same type.
  Unresolved nodes have `tag: None`; resolved nodes have
  `tag: Some("tag:yaml.org,2002:int")`. The seven Core
  schema tag URIs (`str`, `int`, `float`, `bool`, `null`,
  `seq`, `map`) are fixed strings. Internally the resolver
  can use `&'static str` or a `Tag` enum to avoid
  allocation, converting to `String` only at the
  `Node`-construction boundary. The `tag_loc` field stays
  `None` for resolved tags (they have no source position).

- **Event stream is unchanged.** Resolution is a loader
  concern per §3.5 ("composing the representation graph").
  `parse_events()` continues to emit `tag: None` for
  untagged nodes and `tag: Some("!")` for bare `!`. The
  event-level conformance entries are unaffected.

- **JSON schema error handling:** when the JSON schema
  cannot resolve a plain scalar (no regex match), the spec
  says "the YAML processor should consider them to be an
  error." This surfaces as a `LoadError` variant so the
  caller sees a clean error, not a silent `tag: None`.

## Non-Goals

- Typed values (`Value::Int(42)`, `Value::Bool(true)`).
  This plan resolves tags as URI strings; value parsing
  is a separate concern for a future layer.
- Changing `parse_events()`. Event-level tags are correct
  per the spec's model (events are presentation-level;
  resolution is composition-level).
- Changing the default behavior of `load()`. Callers that
  do not opt in see the same `tag: None` they see today.
- Migrating `rlsp-yaml` to consume resolved tags. That is
  Plan 2.
- Adding custom schema support (user-defined regex tables
  or resolvers). The three spec schemas are the scope.

## Steps

- [x] Task 1 — add `Schema` enum and Core schema regex
      module with unit tests
- [x] Task 2 — wire schema resolution into the loader and
      add integration tests
- [x] Task 3 — add JSON and Failsafe schema variants
- [x] Task 4 — update conformance doc, feature-log,
      follow-up queue

## Tasks

### Task 1: Add `Schema` enum and Core schema regex module

Add the public `Schema` enum and an internal `schema`
module containing the Core schema regex-matching functions.
This task builds the classification infrastructure; Task 2
wires it into the loader.

- [x] Create `rlsp-yaml-parser/src/schema.rs` with:
  - A public `Schema` enum with variants `Failsafe`, `Json`,
    `Core`. No `None` variant — the absence of schema
    resolution is represented by `Option<Schema>` in the
    loader options.
  - A public `ResolvedTag` enum with variants `Str`, `Int`,
    `Float`, `Bool`, `Null`, `Seq`, `Map`, each carrying its
    `&'static str` URI constant (e.g.,
    `"tag:yaml.org,2002:int"`). This avoids per-node string
    allocation — the loader converts to `String` only when
    storing in the `Node::tag` field.
  - A `ResolvedTag::as_str(&self) -> &'static str` method.
  - An internal `resolve_scalar(schema: Schema, style:
    ScalarStyle, value: &str, source_tag: Option<&str>) ->
    Result<Option<ResolvedTag>, UnresolvedScalar>` function.
    Returns `Ok(Some(tag))` when resolution produces a tag,
    `Ok(None)` when the source tag is explicit (no
    resolution needed), and `Err(UnresolvedScalar)` when
    JSON schema has no regex match.
  - An internal `resolve_collection(schema: Schema, kind:
    CollectionKind, source_tag: Option<&str>) ->
    Option<ResolvedTag>` function for sequences and
    mappings. `CollectionKind` is a private enum
    `{ Sequence, Mapping }`.
  - The Core regex matchers: `is_core_null(value) -> bool`,
    `is_core_bool(value) -> bool`,
    `is_core_int(value) -> bool`,
    `is_core_float(value) -> bool`. These implement the
    exact patterns from §10.3.2's regex table.
  - The JSON regex matchers reuse the Core ones where the
    patterns overlap and have their own where they differ
    (JSON int is `0 | -? [1-9] [0-9]*`; Core int adds
    `+`, octal `0o`, hex `0x`).
- [x] Register the module in `lib.rs` with `pub mod schema;`
      and re-export `Schema` and `ResolvedTag` from the crate
      root.
- [x] Unit tests in `schema.rs` (inline `#[cfg(test)]`
      module) cover every row of both the Core and JSON regex
      tables — each regex pattern gets at least one positive
      and one negative case. Include edge cases: empty string
      (Core null), `~` (Core null), `0o777` (Core octal int),
      `0x1A` (Core hex int), `.inf`/`.Inf`/`.INF` (Core
      float), `.nan`/`.NaN`/`.NAN` (Core float), leading
      zeros rejected for decimal (`007`), sign-only strings
      (`+`, `-`).
- [x] `cargo test -p rlsp-yaml-parser` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

**Commit:** `bf79f31`

### Task 2: Wire schema resolution into the loader

Add a `schema` option to `LoaderBuilder`/`LoaderOptions`,
call the resolver from `parse_node`, and add integration
tests through the `load()` entry point.

- [x] Add `schema: Option<Schema>` field to `LoaderOptions`
      (default `None` — no resolution, preserving current
      behavior).
- [x] Add `LoaderBuilder::schema(mut self, s: Schema) ->
      Self` method.
- [x] In `LoadState::parse_node`, after constructing each
      `Node` variant, if `self.options.schema` is `Some`:
  - For `Node::Scalar`: call `resolve_scalar(schema,
    style, &value, tag.as_deref())`. On `Ok(Some(t))`,
    set `tag = Some(t.as_str().to_owned())`. On
    `Ok(None)`, leave `tag` as-is (explicit tag). The
    `Err(UnresolvedScalar)` branch (JSON schema only)
    is wired in Task 3 when `LoadError::UnresolvedScalar`
    is added; for now the `Err` case is unreachable
    because only Core and Failsafe are tested in this
    task and neither produces it.
  - For `Node::Mapping`: call `resolve_collection(schema,
    CollectionKind::Mapping, tag.as_deref())`. If
    `Some(t)`, set `tag = Some(t.as_str().to_owned())`.
  - For `Node::Sequence`: same with
    `CollectionKind::Sequence`.
- [x] `tag_loc` stays `None` for resolved tags — resolved
      tags have no source position. Only explicitly-written
      tags have a location.
- [x] Add a convenience function `load_with_schema(input,
      schema)` in the `loader` module that calls
      `LoaderBuilder::new().lossless().schema(schema)
      .build().load(input)`.
- [x] Re-export `load_with_schema` from the crate root.
- [x] Integration tests in
      `rlsp-yaml-parser/tests/schema_resolution.rs` cover:
  - Core schema: plain `42` → `tag:yaml.org,2002:int`,
    plain `true` → `!!bool`, plain `hello` → `!!str`,
    plain `3.14` → `!!float`, plain `null` → `!!null`,
    plain `~` → `!!null`, plain empty → `!!null`.
  - Core schema: quoted `"42"` → `!!str` (not `!!int`).
  - Core schema: block literal `|` scalar → `!!str`.
  - Core schema: explicitly tagged `!!str 42` → tag
    preserved as `tag:yaml.org,2002:str` (not overridden).
  - Core schema: bare `!` tag → resolved by kind (`!!str`
    for scalar, `!!seq` for sequence, `!!map` for mapping).
  - Core schema: untagged mapping → `tag:yaml.org,2002:map`,
    untagged sequence → `tag:yaml.org,2002:seq`.
  - Core schema: nested structure — mapping with sequence
    values containing mixed scalar types, all tags resolved.
  - Core schema edge cases: `0o777` → `!!int`, `0x1A` →
    `!!int`, `.inf` → `!!float`, `.nan` → `!!float`,
    `+12` → `!!int`, `-0` → `!!int`.
  - No schema (default `load()`): all tags remain `None` —
    regression test confirming no behavior change.
  - `load_with_schema` convenience function produces the
    same results as the builder chain.
- [x] `cargo test -p rlsp-yaml-parser` passes.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

**Commit:** `5a7e5b3`

### Task 3: Add JSON and Failsafe schema variants

Extend the loader integration tests to cover JSON and
Failsafe schema resolution. JSON schema's error-on-no-match
behavior requires a new `LoadError` variant.

- [x] Add `LoadError::UnresolvedScalar { value: String, pos:
      Pos }` variant for the JSON schema no-match case.
      Error message: `"JSON schema: plain scalar does not
      match any type pattern"`.
- [x] Wire the `Err(UnresolvedScalar)` branch in
      `LoadState::parse_node`'s scalar arm to return
      `LoadError::UnresolvedScalar { value, pos }`. This
      branch was left unreachable in Task 2 (Core and
      Failsafe never produce it); Task 3 makes it live.
- [x] Verify the Failsafe path: `resolve_scalar` with
      `Schema::Failsafe` resolves every scalar to `!!str`
      regardless of content or style. Collections resolve by
      kind.
- [x] Verify the JSON path: `resolve_scalar` with
      `Schema::Json` matches `null`, `true|false`,
      `-? (0 | [1-9][0-9]*)`, and the float pattern;
      unmatched plain scalars return
      `Err(UnresolvedScalar)`. Quoted/block scalars →
      `!!str`. Collections → by kind.
- [x] Integration tests in the same
      `tests/schema_resolution.rs` file:
  - JSON schema: plain `42` → `!!int`, plain `true` →
    `!!bool`, plain `null` → `!!null`, plain `3.14` →
    `!!float`.
  - JSON schema: plain `hello` → `LoadError::UnresolvedScalar`.
  - JSON schema: plain `0o777` → error (JSON int regex
    does not support octal).
  - JSON schema: quoted `"hello"` → `!!str` (no error —
    only plain scalars go through the regex table).
  - Failsafe schema: plain `42` → `!!str` (not `!!int`),
    plain `true` → `!!str`, quoted `"hello"` → `!!str`.
  - Failsafe schema: untagged sequence → `!!seq`, untagged
    mapping → `!!map`.
  - Failsafe schema: bare `!` tag → resolved by kind.
- [x] `cargo test -p rlsp-yaml-parser` passes.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

**Commit:** `9a0acad`

### Task 4: Update conformance doc, feature-log, and follow-up queue

Reflect the new schema resolution capability in the
conformance audit and the user-facing feature log.

- [x] Update the §10 Failsafe `!` non-specific tag entry:
      Classification from `Not Implemented` to `Conformant`.
      Update Implementation to cite the schema resolver and
      the `Schema::Failsafe` / `Schema::Core` / `Schema::Json`
      paths. Remove the "structural parser only" framing.
      Update Test coverage to cite the new integration tests.
- [x] Update the four Lenient §10 entries (JSON plain
      scalars, JSON collections, Core plain scalars, Core
      collections): Classification from `Lenient` to
      `Conformant`. Same pattern — updated Implementation,
      removed Discrepancy, updated Test coverage.
- [x] Update the `## Summary` table: remove the four §10
      Lenient rows (the Not Implemented Failsafe entry is
      a chapter-body entry only — the Summary table tracks
      Lenient and Strict findings). Update the headline
      count. Current headline (after the tag-handle plan):
      "4 Lenient findings, 0 Strict findings (bug-class),
      3 Strict (security-hardened) findings, total 7
      entries." After removing four Lenient entries, it
      becomes "0 Lenient findings, 0 Strict findings
      (bug-class), 3 Strict (security-hardened) findings,
      total 3 entries."
- [x] Update `rlsp-yaml-parser/docs/feature-log.md`: add a
      user-facing entry documenting opt-in schema resolution
      via `LoaderBuilder::schema()` and
      `load_with_schema()`, covering Failsafe, JSON, and
      Core schemas per §10.
- [x] Remove the stale follow-up queue entry. In
      `.ai/memory/project_followup_plans.md`, delete the
      `[Lenient] Schema resolution not implemented (§10)`
      bullet under `## Open: rlsp-yaml-parser`.
- [x] No source code is modified in this task.
- [x] `cargo test --workspace` passes.
- [x] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

**Commit:** `260c166`

## Decisions

- **Opt-in via `LoaderBuilder::schema()`, default unchanged.**
  `load()` stays backward-compatible (no resolution);
  `load_with_schema(input, Schema::Core)` is the
  convenience entry point. This preserves `rlsp-yaml`'s
  current behavior without requiring a migration in this
  plan.
- **`Schema` enum, not trait.** Three schemas defined by
  the spec, no user-extensible customization — an enum is
  simpler and avoids the vtable overhead of dynamic
  dispatch for every node.
- **`ResolvedTag` enum avoids per-node allocation.** The
  seven tag URIs are `&'static str` constants. Allocation
  happens only once when storing into `Node::tag:
  Option<String>`. If profiling later shows this matters,
  the tag field can change to `Option<Cow<'static, str>>`
  — but that's a micro-optimization not needed now.
- **`Option<Schema>` in `LoaderOptions`, not a `Schema::None`
  variant.** `Schema` represents a real schema; absence is
  `None`. This matches `Option` semantics and avoids a
  "null object" variant.
- **JSON no-match is a `LoadError`, not a silent fallback.**
  §10.2 says "the YAML processor should consider them to be
  an error." Silently falling back to `!!str` would be
  Core, not JSON.
- **Resolution in `parse_node`, not a post-pass.** Resolving
  during node construction avoids a second tree walk and
  means the returned AST is already resolved. The resolver
  needs `style` (to distinguish plain vs quoted) and
  `value` (for regex matching), both of which are available
  at construction time.
- **Four tasks.** Task 1 (regex module) is independently
  testable. Task 2 (Core wiring + integration) is the
  main delivery. Task 3 (JSON + Failsafe) adds the
  remaining schemas. Task 4 (docs) closes the audit gap.
  This mirrors the block/flow/docs decomposition of the
  1024-char plan.
- **`Failsafe` `?` non-specific tag stays as `tag: None`.**
  The spec says Failsafe leaves `?` nodes unresolved; the
  current `tag: None` representation already conforms. Only
  `!` (non-specific) gets resolved by kind.
