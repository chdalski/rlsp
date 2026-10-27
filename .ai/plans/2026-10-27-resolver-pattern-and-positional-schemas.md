**Repository:** root
**Status:** InProgress
**Created:** 2026-10-27

# Schema Path Resolver Follows Pattern, Additional, and Positional Schemas

## Goal

The shared schema path resolver behind hover and completion follows
only `properties`, `items`, and the `allOf`/`anyOf`/`oneOf` branches,
so hover shows no schema docs and completion offers no schema
suggestions under keys described by `patternProperties` or
`additionalProperties` — GitHub Actions `jobs.<id>`, docker-compose
`services.<name>`, Kubernetes labels — nor anywhere below them
(`jobs.build.steps[0].uses`), nor for array items whose schema
depends on their position (`prefixItems`, or the Draft-04/07 tuple
`items: [...]` with `additionalItems`). Make the resolver follow
these three kinds of schema, so hover and completion work at those
positions, while every path that resolves today keeps its result
except the positional corrections named in Decisions.

## Context

- `rlsp-yaml/src/schema/path.rs`: `resolve_schema_path(parsed,
  schema, &[SchemaPathSegment])` with
  `SchemaPathSegment::{Key(String), Item}`; hover
  (`src/hover.rs`) and completion (`src/completion.rs`,
  `src/completion/`) are its only callers.
- `rlsp-yaml/src/schema.rs`: `JsonSchema::pattern_properties`
  (`Vec<(String, JsonSchema)>`, sorted by pattern text — the crate's
  `serde_json` keeps object keys sorted), `additional_properties`
  (`AdditionalProperties::{Denied, Schema}`; `true` parses as an
  empty schema), `prefix_items` (also holds the Draft-04/07 tuple
  form of `items`), `additional_items` (read only next to that tuple
  form), and `items`.
- `rlsp-yaml/src/schema_validation/support.rs`: `compile_pattern` and
  `is_valid_regex_value` compile schema patterns under
  `MAX_PATTERN_LEN` and `REGEX_SIZE_LIMIT` in priced size tiers
  (`COMPILE_TIERS`), paid from validation's `Budget`, and cache them
  in the thread-local `REGEX_CACHE`. Patterns come from untrusted
  remote schemas.
- Validation's semantics for these keywords:
  `src/schema_validation/mapping_constraints.rs`
  (`validate_pattern_properties` and the `additionalProperties`
  check) and `src/schema_validation/array_constraints.rs`.
- JSON Schema 2020-12, Applicator vocabulary
  (https://json-schema.org/draft/2020-12/json-schema-core#section-10.3):
  `prefixItems`, `items`, `patternProperties`,
  `additionalProperties`.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: the pattern compiler and its cache live in the schema
      layer
- [x] Task 2: item path segments carry their index; hover and
      completion share one path type
- [x] Task 3: keys resolve through `patternProperties` and
      `additionalProperties`
- [x] Task 4: items resolve by position
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Move the pattern compiler and its cache into the schema layer

The resolver lives in `schema`, which `schema_validation` depends on,
so it cannot use validation's pattern compiler. Move the pattern
limits, the size tiers, the compiled-pattern cache, and one
lookup-or-compile entry point into a crate-internal module
`src/schema/pattern.rs`; the entry point takes the payment for each
compile attempt as a closure, so validation pays from its `Budget`
and the resolver can pay from its own allowance. Behavior is
unchanged.

- [x] `MAX_PATTERN_LEN`, `REGEX_SIZE_LIMIT`, the size tiers, the
      pattern cache, and the lookup-or-compile entry point are
      defined in `src/schema/pattern.rs` and visible only inside
      the crate; validation's `compile_pattern` and
      `is_valid_regex_value` use them and keep no copy
- [x] The regex tests in `src/schema_validation/budget_tests.rs`
      (`format_regex_*`, `schema_pattern_pays_its_compile_price_once_per_run`),
      the pattern tests in `mapping_constraints.rs` and
      `scalar_constraints.rs`, and every other validation test pass
      with their assertions unchanged; only import paths change
- [x] Build, tests, clippy, and fmt pass

### Task 2: Give item path segments their index and share one path type

The item segment becomes `Item(usize)`, so a later task can pick the
schema for a position. Hover's own `PathSegment` and its conversion
are replaced by `SchemaPathSegment`. The resolver ignores the index
in this task, and `find_node_at_path` keeps returning nothing for an
item segment. Behavior is unchanged.

- [x] Every builder of an item segment supplies the item's index:
      hover's AST walk, the cursor locator for a key or value below
      an item, an existing item (`InSequenceItem`), and a blank line
      in a sequence (`InBlankSequence`), where the index is the
      number of items that start before the cursor
- [x] Hover has no path type of its own; its walk and its displayed
      path use `SchemaPathSegment`, and the doc comment of the item
      segment says what its index is
- [x] Unit tests pin the index the cursor locator reports for an
      existing item and for a blank line before the first item,
      between two items, and after the last item, at the root and
      under a mapping key
- [x] Every hover, completion, cursor-locator, navigation, and
      resolver test passes with its assertions unchanged; only path
      literals change, and the doc comment of `find_node_at_path`
      matches what it does with an item segment
- [x] Build, tests, clippy, and fmt pass

### Task 3: Resolve keys through `patternProperties` and `additionalProperties`

A key that no declared property resolves is resolved through the
patterns and `additionalProperties`, in a second pass of the walk
(see Decisions). Pattern work is paid from a per-call allowance the
way validation pays from its budget: a pattern's compile price is
charged once per call, at its first use in the call, whether it is
compiled or found in the cache, and every test of a pattern against
a key costs one step plus the matching work for the key's length at
the pattern's tier price. When the second pass reaches a node, it
tests all of that node's patterns against the key; if the allowance
refuses any of them, the node offers neither its patterns nor its
`additionalProperties`.

- [x] Resolver unit tests in `src/schema/path.rs` pin: a key matched
      by a pattern; no match; the first matching pattern in
      pattern-text order wins, and a later matching pattern is used
      when the path dead-ends below the first; invalid and
      over-long patterns count as not matching, so
      `additionalProperties` applies; a matching pattern excludes
      `additionalProperties` at that node even when the path
      dead-ends below it; `additionalProperties` as a schema and as
      `false`; an own property shadows a pattern; a property that a
      later branch declares wins over an earlier branch's pattern
      and over its `additionalProperties`, including
      `additionalProperties: true`; a node one of whose patterns the
      allowance refuses yields neither its patterns nor its
      `additionalProperties`, even when an earlier pattern matched
      and resolves the path; self-referencing schemas through `patternProperties` and
      through `additionalProperties`, as cycles, long chains, and
      wide ladders, end
- [x] Every path that resolves without patterns and
      `additionalProperties` resolves to the same schema as before,
      and every existing resolver, hover, and completion test passes
      with its assertions unchanged
- [x] Tests pin the resolver's charging: a compile the allowance
      cannot pay for is not made; a pattern found in the cache is
      charged its recorded compile price at its first use in a
      call and not again in that call; each test of a pattern costs
      one step plus its matching work; pattern validity equals a
      single compile under `REGEX_SIZE_LIMIT`
- [x] The allowance is a named constant whose doc comment states the
      basis of its size, and the integration schemas of this task
      resolve within it
- [x] Hover shows the docs, and completion offers the keys and enum
      values, of a schema reached through a pattern key, through
      `additionalProperties`, and below them, including a `$ref`
      target between the pattern key and the resolved schema and
      one below it; pinned by unit tests through `hover_at` and
      `complete_at`
- [x] Integration tests in `tests/lsp_lifecycle/hover.rs` and
      `tests/lsp_lifecycle/completion.rs`, against a seeded,
      inline GitHub-workflow-like schema (`jobs` with a key pattern
      to a `$ref` job that is a `oneOf` of two objects, `steps` as
      an array of a `$ref` step) and a compose-like schema
      (`services` with an `^x-` pattern beside
      `additionalProperties: false`), pin hover docs and key
      completion under `jobs.<id>`, `jobs.<id>.steps[0]`, and
      `services.<name>`
- [x] The doc comment of `resolve_schema_path` describes the two
      passes, the patterns, and `additionalProperties` as they work
- [x] `rlsp-yaml/docs/feature-log.md` has an entry that tells the
      reader that hover and completion now work under keys described
      by `patternProperties` and `additionalProperties`, and which
      pattern wins when several match
- [x] Build, tests, clippy, and fmt pass

### Task 4: Resolve items by position

An item segment at index `i` takes the node's own `prefixItems[i]`
when `i` is within the prefix, otherwise its `items`, otherwise its
`additionalItems` schema; `additionalItems: false` beyond the prefix
yields nothing; a node with none of these falls back to the
composition branches as today. This belongs to the first pass of the
walk.

- [x] Resolver unit tests pin: `prefixItems` inside and beyond the
      prefix, with and without `items`; the tuple form with an
      `additionalItems` schema and with `additionalItems: false`; a
      plain `items` at every index; an own positional schema
      shadowing a branch's `items`; a tuple inside a composition
      branch; and a positional schema behind a `$ref` target
- [x] Each result listed in the Decisions entry "Positional schemas
      change these results" is pinned by a test; every other
      existing test passes with its assertions unchanged
- [x] Hover shows the docs of a tuple item by its position, and
      completion offers item `i`'s keys in an existing item and on a
      blank line at position `i`, pinned by unit tests through
      `hover_at` and `complete_at`
- [x] Integration tests in `tests/lsp_lifecycle/hover.rs` and
      `tests/lsp_lifecycle/completion.rs` pin hover docs and key
      completion for the second item of a `prefixItems` sequence
- [x] The doc comment of `resolve_schema_path` describes how an
      item's schema is picked by its position
- [x] The feature-log entry for this change also tells the reader
      that hover and completion pick an array item's schema by its
      position
- [x] Build, tests, clippy, and fmt pass

## Non-Goals

- Following `if`/`then`/`else`, `dependentSchemas`, and
  `unevaluatedProperties`/`unevaluatedItems`
- Keeping `false` sub-schemas in place — today
  `prefixItems: [A, false, C]` parses as `[A, C]`, a `false` pattern
  schema is dropped, and `items: false` reads as absent (the queued
  boolean-`false` plan covers this class)
- Applying patterns in the order the schema writes them
- Structural completion keys of a sequence item taken from the first
  document instead of the cursor's (separate plan after this one)
- Bounding the pattern cache (queued plan)
- Offering key names for pattern or `additionalProperties` keys in
  completion

## Decisions

- **Two passes:** the first pass walks declared schemas only — own
  `properties`, positional items, and the composition branches, as
  today; only when it finds nothing does a second pass walk the
  same order plus each node's matching patterns and then its
  `additionalProperties`. A single per-node order would let an
  earlier branch's pattern or `additionalProperties`, including
  `additionalProperties: true`, shadow a property a later branch
  declares, and
  the two passes keep every path that resolves today on its result
  and do no pattern work for it.
- **Pattern-text order:** patterns are stored sorted by their text,
  so "first matching pattern" means first in that order. Declined:
  enabling `serde_json`'s `preserve_order`, which changes the
  iteration order of every JSON object in the crate.
- **Several matching patterns:** the first is tried first, a later
  one when the path dead-ends below it, like branch fallback.
- **A matching pattern excludes `additionalProperties`:** at the
  same node, even when the path dead-ends below the pattern, as
  JSON Schema and validation apply it; invalid and over-long
  patterns count as not matching, as in validation.
- **Allowance refusal voids the node's fallbacks:** a node whose
  patterns are tested together and one of which the allowance
  refuses offers neither its patterns nor its
  `additionalProperties`, mirroring validation, which does not
  report a key as additional when its budget ran out; testing all
  of a node's patterns at once keeps the result independent of
  which pattern the path happens to need.
- **Per-call allowance, priced like validation:** the compile price
  is charged once per call also on a cache hit, so the result of a
  request does not depend on which earlier request filled the
  cache, and a schema with many costly patterns is bounded per
  request. Its size is set with the security engineer during
  Task 3.
- **One pattern compiler and cache:** the resolver uses validation's
  limits and prices, and the queued cache-bounding plan bounds one
  store.
- **`Item(usize)`:** every builder knows the index; an optional index
  would be a state nothing produces.
- **`find_node_at_path` returns nothing for an item segment:**
  unchanged results; following the index would change structural
  completion keys in nested sequences.
- **Own positional schema shadows the branches:** the same rule as
  own `items` and own properties today; `additionalItems: false`
  beyond the prefix is an own answer of "nothing".
- **Positional schemas change these results:** each follows the
  JSON Schema meaning of positional items.
  1. Own `prefixItems` and own `items`, index inside the prefix:
     `prefixItems[i]` instead of `items`.
  2. Own `prefixItems` and a branch's `items`, index inside the
     prefix: `prefixItems[i]` instead of the branch's `items`.
  3. Own tuple with an `additionalItems` schema (including
     `additionalItems: true`, an empty schema) and a branch's
     `items`, index beyond the prefix: the `additionalItems` schema
     instead of the branch's `items`.
  4. Own tuple with `additionalItems: false` and a branch's `items`,
     index beyond the prefix: nothing instead of the branch's
     `items`.
  5. A tuple in an earlier branch and `items` in a later branch,
     index inside the tuple's prefix: the tuple's entry instead of
     the later branch's `items`.
  6. The same with an `additionalItems` schema on the earlier
     branch's tuple, index beyond its prefix: that schema instead
     of the later branch's `items`.
- **Inline test schemas:** the integration tests use trimmed
  GitHub-workflow-like and compose-like schemas written in the
  tests; the real upstream schemas are large and change.
