**Repository:** root
**Status:** InProgress
**Created:** 2026-10-28

# Keep Boolean `false` Sub-Schemas

## Goal

The schema parser cannot represent the boolean schema `false` ("no
value is allowed here"): it drops it, so a `false` sub-schema is
silently ignored for every keyword except `additionalProperties`,
`additionalItems`, and `unevaluatedProperties`, and
`prefixItems: [A, false, C]` shifts to `[A, C]`. Keep every `false`
sub-schema in its place for every keyword, so validation rejects what
it rejects and hover and completion treat it as "nothing allowed
here"; and place the diagnostics of `propertyNames` and
`dependentSchemas` on the key or mapping they concern instead of the
start of the file.

## Context

- `rlsp-yaml/src/schema.rs`: `parse_node` returns
  `JsonSchema::default()` for `true` and `None` for `false`; every
  sub-schema keyword builds through it, and the callers drop `None`.
  `is_schema_value` decides what discovery and the tuple reading
  (`reads_tuple_items`) treat as a schema; `parse_target` builds the
  targets of `$ref`. `additionalProperties`, `additionalItems`, and
  `unevaluatedProperties` keep `false` already, as
  `AdditionalProperties::Denied`.
- Consumers of `JsonSchema`: validation (`src/schema_validation/`,
  entry `validate_node`), the path resolver (`src/schema/path.rs`,
  `walk`), and completion's property walk
  (`src/completion/schema_completions.rs`, `walk_properties`); hover
  and completion reach schemas through the resolver.
- `rlsp_yaml::schema` is a public module of the published crate;
  `JsonSchema` has only public fields and is not `#[non_exhaustive]`.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: `propertyNames` and `dependentSchemas` diagnostics sit on
      their key or mapping
- [ ] Task 2: `false` sub-schemas keep their place and meaning
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Place `propertyNames` and `dependentSchemas` diagnostics

`mapping_constraints.rs` validates `propertyNames` against a synthetic
key node and `dependentSchemas` against a reconstructed mapping, both
with the span at the start of the input, so their diagnostics appear
at line 0 even in a later document.

- [x] A `propertyNames` diagnostic covers the key it concerns, and a
      `dependentSchemas` diagnostic covers the mapping it concerns,
      also in a later document of the file; unit tests pin both
      ranges
- [x] Every other existing test passes with its assertions unchanged
- [x] Build, tests, clippy, and fmt pass

### Task 2: Keep `false` sub-schemas in their place

`JsonSchema` gets a public marker for the always-failing schema;
`parse_node` and `parse_target` build it from `false` through one
shared mapping of boolean values to schemas, and `is_schema_value`
counts `false` as a schema.

- [ ] For every sub-schema keyword, a `false` value parses to the
      always-failing schema in its place: `items`, `prefixItems` and
      the tuple form of `items` (keeping each index), `contains`,
      `allOf`/`anyOf`/`oneOf` entries, `not`, `if`, `then`, `else`,
      `unevaluatedItems`, `propertyNames`, `properties` and
      `patternProperties` values, `dependentSchemas` and
      `dependencies` schema values, and `contentSchema`; a `$ref` or
      `$dynamicRef` that lands on `false` resolves to it; parse tests
      pin each keyword, inline and behind a reference
- [ ] A root schema that is `false` is still rejected as before, and
      a root `$ref` to `false` resolves
- [ ] A value validated against a `false` schema gets one diagnostic
      with a new code `schemaNotAllowed`, severity Warning, and the
      message "Value at {path} is not allowed by the schema"; this
      applies to each item beyond the prefix under `items: false`, the
      item at the index of a `false` `prefixItems` or tuple entry, the
      value of a property whose schema is `false`, the value of a key
      a `false` pattern matches, each unevaluated item under
      `unevaluatedItems: false`, the node under a `false` `allOf`
      entry, the node when a selected `then` or `else` is `false`,
      each key under `propertyNames: false`, the mapping when a key
      with a `false` `dependentSchemas` or `dependencies` schema is
      present, and the decoded content under `contentSchema: false`
      (path of the scalar followed by `(content)`, located like other
      `contentSchema` diagnostics); validation tests pin each
- [ ] Keywords that probe a `false` sub-schema keep their own
      diagnostics: `contains: false` reports `schemaContains` on every
      array unless `minContains` is 0, `anyOf`/`oneOf` report their
      existing "does not match" diagnostics counting `false` branches,
      `not: false` reports nothing, and `if: false` applies `else`;
      validation tests pin each
- [ ] Validation tests also pin the index of a `prefixItems` entry
      after a `false` entry, a `false` pattern excluding
      `additionalProperties` for its keys, `false` properties and
      patterns counting as evaluated for `unevaluatedProperties`, and
      `items: false` and `false` `prefixItems` entries counting as
      evaluated for `unevaluatedItems`
- [ ] Inline `false` for `additionalProperties`, `additionalItems`, and
      `unevaluatedProperties` keeps its current diagnostic and wording
- [ ] The resolver dead-ends at a `false` schema, inline or behind a
      reference, so a later sibling branch can still answer; a `false`
      entry at a positional index is that node's own answer of
      nothing; a `false` property is the node's own answer and its
      branches are not tried; resolver tests pin each
- [ ] Completion does not offer a key whose property schema is
      `false`, also not in the snippet that inserts the missing
      required keys, and hover shows no schema section at a `false`
      schema; pinned by unit tests through `complete_at` and
      `hover_at`
- [ ] The existing tests whose assertions describe the old reading are
      changed and no others: the `prefix_items_without_a_schema` case
      of `tuple_items_are_discovered_where_the_build_reads_them` uses
      a non-schema value and a new case pins `false`; the
      `boolean_false` case of
      `ref_to_a_value_that_is_no_schema_stays_unresolved` moves to a
      test that pins the resolved reference; and
      `every_child_schema_position_has_a_case` lists the new field
- [ ] The parse budget charges a `false` node like a `true` node,
      pinned by `false` cases in the charge-boundary test
- [ ] An integration test in `tests/lsp_lifecycle/` validates a
      document against a seeded schema with a `false` sub-schema and
      receives the `schemaNotAllowed` diagnostic at the value
- [ ] The doc comments of the new field and of `parse_schema` tell a
      library user what the marker means and that a reader ignoring it
      reads the schema as `true`, and no doc comment still describes
      `false` as dropped or as no schema (among them `parse_node`,
      `reads_tuple_items`, and `resolve_schema_path`)
- [ ] `rlsp-yaml/docs/configuration.md` lists `schemaNotAllowed`, and
      `rlsp-yaml/docs/feature-log.md` has an entry that tells the
      reader that `false` sub-schemas now take effect for every
      keyword
- [ ] Build, tests, clippy, and fmt pass

## Non-Goals

- Folding `AdditionalProperties::Denied` into the always-failing schema
  (separate plan after this one)
- Listing the diagnostic codes missing from `configuration.md`'s code
  table other than `schemaNotAllowed` (queued follow-up)
- The range of `contentSchema` diagnostics, which lies in the embedded
  document for every `contentSchema` diagnostic
- A resolver result that distinguishes "nothing allowed" from "no
  schema found"

## Decisions

- **A marker field on `JsonSchema`:** one parser change covers every
  keyword and the target table, and only three consumers read it.
  Declined: encoding `false` as `{not: {}}` (a misleading `not`
  diagnostic and shape-sniffing in the resolver) and an enum per
  sub-schema slot (18 field types and every consumer change). The new
  public field is a breaking change of `rlsp_yaml::schema`.
- **Warning:** the same severity as "Additional property … is not
  allowed", which `properties: {x: false}` next to
  `additionalProperties: false` already produced.
- **A root `$ref` to `false` resolves:** every document then gets the
  diagnostic at its root, as JSON Schema defines; only a root that is
  `false` itself stays rejected.
- **"Nothing" stays `None` for hover and completion:** at a `false`
  schema completion falls back to structural suggestions from the
  document, as at `additionalItems: false`.
- **A `false` pattern dead-ends like any pattern:** with
  `{"^a": false, "^ab": S}`, hover and completion for `abc` show `S`;
  validation still reports `abc`.
- **Changed results the plan accepts:** a `$ref` to `false` fails
  where it was unresolved and permissive; `prefixItems: [false]` holds
  a schema, so the array form of `items` next to it is not read as a
  tuple; `properties: {x: false}` reports `x` with `schemaNotAllowed`
  where it reported an additional or unevaluated property or applied
  the `additionalProperties` schema; `anyOf`/`oneOf` messages count
  `false` branches; `items: false` and `unevaluatedItems: false` use
  the new generic wording; `items: false` and `false` `prefixItems`
  entries count as evaluated, so `unevaluatedItems` no longer reports
  those items.
