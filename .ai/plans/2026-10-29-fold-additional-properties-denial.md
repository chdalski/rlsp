**Repository:** root
**Status:** InProgress
**Created:** 2026-10-29

# One Representation of `false` for Every Sub-Schema Keyword

## Goal

`false` under `additionalProperties`, `additionalItems`, and
`unevaluatedProperties` has a representation of its own, so a `$ref` to
`false` there reports something other than a literal `false`. Fold it
into the always-failing schema, so `false` has one representation
everywhere and a `$ref` to `false` under these keywords reports exactly
what a literal `false` reports.

## Context

- `rlsp-yaml/src/schema.rs`: `JsonSchema::always_fails` and
  `JsonSchema::always_failing()` represent `false` under every
  sub-schema keyword except the three, which hold
  `Option<AdditionalProperties>`
  (`enum AdditionalProperties { Denied, Schema(Box<JsonSchema>) }`);
  `unevaluatedItems` holds `Option<Box<JsonSchema>>` parsed with
  `parse_node`. `rlsp_yaml::schema` is public API.
- A literal `false` reports `schemaAdditionalProperty` at the key for
  `additionalProperties`, `schemaUnevaluatedProperty` at the key for
  `unevaluatedProperties`, and `schemaAdditionalProperties` at the item
  for `additionalItems`; a `$ref` to `false` there reports
  `schemaNotAllowed` at the value, from `validate_node`
  (`rlsp-yaml/src/schema_validation.rs`).
- Hover and completion read these keywords only through the path
  resolver in `rlsp-yaml/src/schema/path.rs`, where a `false` schema
  dead-ends the walk.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: `false` has one representation under every sub-schema
      keyword
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Fold the denial into the always-failing schema

Remove `AdditionalProperties`, so `false` under the three keywords
parses to the always-failing schema like under every other keyword,
and decide each keyword diagnostic from the resolved sub-schema. A
literal `false` keeps every diagnostic, hover, and completion result
it has today, except that the resolver spends one step of its second
pass's allowance on it; a `$ref` to `false` under the three keywords
reports what a literal `false` reports.

- [x] `rlsp_yaml::schema` has no `AdditionalProperties` type, and
      `false` under `additionalProperties`, `additionalItems`, and
      `unevaluatedProperties` parses to the always-failing schema, as
      under every other sub-schema keyword
- [x] For each of the three keywords, a test pins that a `$ref` to
      `false` yields the same diagnostics (code, message, range, and
      severity) as a literal `false`, and a test pins that a literal
      `false` whose location is the target of a `$ref` elsewhere in the
      schema yields its keyword diagnostic
- [x] For each of the three keywords, tests pin that a `$ref` to `false`
      behaves like a literal `false` at the validation depth limit,
      inside an `anyOf` branch, and in the validation budget steps it
      spends
- [x] The tests that pin today's literal-`false` behavior pass with
      their assertions unchanged — only how they build the schema
      changes: `the_inline_denials_keep_their_diagnostic_and_wording`,
      the `additionalProperties`, `additionalItems`, and
      `unevaluatedProperties` tests in
      `schema_validation/mapping_constraints.rs`,
      `schema_validation/array_constraints.rs`,
      `schema_validation/reference_tests.rs`, `schema_validation.rs`,
      and `schema_validation/budget_tests.rs`, and the resolver, hover,
      and completion tests at `additionalProperties: false` and
      `additionalItems: false`
- [x] The generic `false` parse tests cover the three keywords, and the
      tests that pinned only the removed type or the replaced
      `schemaNotAllowed` for a `$ref` to `false` under them
      (`inline_false_for_the_denying_keywords_stays_denied`,
      `a_reference_to_false_for_the_denying_keywords_is_a_schema`,
      `a_reference_to_false_for_the_denying_keywords_fails_the_value`)
      are gone
- [x] The doc comments of `JsonSchema` and of the three fields tell a
      library reader how to recognize `false` in them
- [x] No code, comment, or doc refers to the removed
      `AdditionalProperties` type or to a separate denial
      representation of `false` under the three keywords
- [x] `rlsp-yaml/docs/feature-log.md` has an entry that tells the
      reader that a `$ref` to `false` under the three keywords now
      reports the keyword's diagnostic
- [x] The commit is marked breaking, and its `BREAKING CHANGE:` footer
      tells library users that `AdditionalProperties` is removed and
      how to recognize `false` in the three fields
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

## Non-Goals

- Treating other schemas that always fail but are not `false` (such as
  `{"not": {}}`) as a denial

## Decisions

- **One representation:** the three fields become
  `Option<Box<JsonSchema>>`, parsed like `unevaluatedItems`; the public
  type is removed, a breaking change of `rlsp_yaml::schema`. Declined:
  keeping `Denied` and only aligning the diagnostic for a `$ref` to
  `false` — it needs no extra memory but keeps two representations.
- **A literal `false` costs what it costs elsewhere:** under the three
  keywords it now allocates one `JsonSchema` (about 1 KB) and counts
  against the parse budget, like `false` under every other keyword.
- **The keyword diagnostic is decided at the keyword, after resolving
  the sub-schema, never in `validate_node`:** a literal `false` then
  stays clear of `validate_node`'s entry guards (depth limit,
  diagnostic cap, probe, budget step) and keeps its behavior; resolving
  covers a literal `false` that is itself a `$ref` target. A `$ref` to
  `false` under these keywords follows the literal behavior in full: it
  reports at the validation depth limit instead of leaving the result
  undecided (a probe fails instead of staying undecided), spends no
  validation budget step, and names the parent path plus the key or
  index. Declined: passing a denial into `validate_node` (changes every
  caller and the literal behavior) and a shared helper returning denial
  or schema (rebuilds the removed type).
- **The resolver uses the generic `false` rule:** `false` under
  `additionalProperties` and `additionalItems` dead-ends like any
  `false` schema, and the resolver's own denial case goes away. Each
  such `false` costs one more step of the second pass's allowance, so
  after this change a walk that ends exactly at today's allowance
  returns nothing.
