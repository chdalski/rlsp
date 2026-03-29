# Missing JSON Schema Keywords

**Repository:** root
**Status:** InProgress
**Created:** 2026-03-29

## Goal

Add the remaining JSON Schema keywords that provide user
benefit in a YAML language server: `minProperties`,
`maxProperties` (object cardinality constraints), and
`additionalItems` (Draft-04/07 tuple array restriction).
These are the last gaps in the validation implementation.

## Context

- The `JsonSchema` struct in `schema.rs:121` holds all
  parsed schema fields — `minProperties`/`maxProperties`
  are completely absent; `additionalItems` is absent
- `validate_mapping` in `schema_validation.rs:1292`
  validates object properties — cardinality checks would
  go here, mirroring `validate_array_constraints` at
  line 389 which already handles `minItems`/`maxItems`
- Array validation in `schema_validation.rs:280` handles
  `prefix_items` (positional tuple schemas) and `items`
  (schema for remaining elements) — but when a Draft-04/07
  schema uses array-form `items` (parsed as `prefix_items`)
  with `additionalItems: false`, elements beyond the tuple
  are currently unchecked
- The `AdditionalProperties` enum (`Denied` | `Schema`)
  already models the false-or-schema pattern used by
  `additionalProperties` — `additionalItems` has the same
  shape
- Schema parsing in `schema.rs:780` converts array-form
  `items` to `prefix_items` for backward compatibility —
  `additionalItems` should only apply when `prefix_items`
  was populated from array-form `items` (not from
  `prefixItems`, which uses `items` in Draft 2020-12)
- Existing test patterns for array constraints
  (`schema_validation.rs:3754`) and additional properties
  (`schema_validation.rs:2189`) provide templates for the
  new tests

## Steps

- [x] Add `minProperties`/`maxProperties` to schema
  struct, parsing, and validation — 1963542
- [x] Add `additionalItems` to schema struct, parsing,
  and validation — 6bcfef4
- [ ] Update feature-log with new keywords

## Tasks

### Task 1: `minProperties` / `maxProperties`

Add object cardinality validation, mirroring the existing
`minItems`/`maxItems` pattern for arrays.

**Files:** `schema.rs`, `schema_validation.rs`

**New fields in `JsonSchema`:**
- `min_properties: Option<u64>`
- `max_properties: Option<u64>`

**Parsing:** Extract `"minProperties"` and
`"maxProperties"` as `u64` from the schema object,
alongside existing object keywords.

**Validation in `validate_mapping`:** Count mapping entries
and check against min/max. Place before the property
iteration loop for early feedback.

**Diagnostic codes:** `schemaMinProperties`,
`schemaMaxProperties`

- [ ] Add fields to `JsonSchema` struct
- [ ] Parse in schema parsing function
- [ ] Validate in `validate_mapping`
- [ ] Unit tests (below min, above max, at boundary)
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 2: `additionalItems` (Draft-04/07)

Add support for restricting extra elements in tuple-style
arrays. In Draft-04/07, `items` as array defines positional
schemas, and `additionalItems` controls elements beyond
those positions. In Draft 2020-12, this role was taken by
`items` (with `prefixItems` replacing array-form `items`),
so `additionalItems` only applies to Draft-04/07 schemas.

**Files:** `schema.rs`, `schema_validation.rs`

**New field in `JsonSchema`:**
- `additional_items: Option<AdditionalProperties>` — reuse
  the existing enum (`Denied` | `Schema`) since the
  semantics are identical

**Parsing:** Extract `"additionalItems"` — if `false`,
store `Denied`; if object, parse as sub-schema and store
`Schema(...)`. Only parse when `prefix_items` was populated
from array-form `items` (not from `prefixItems`), to avoid
conflicting with Draft 2020-12 semantics.

**Validation:** In the sequence validation block (after
`prefix_items` and before `items` fallthrough), if `items`
is `None` and `additional_items` is present, apply it to
elements beyond `prefix_len`:
- `Denied` → emit diagnostic for each extra element
- `Schema(s)` → validate each extra element against `s`

**Diagnostic code:** `schemaAdditionalItems`

- [ ] Add field to `JsonSchema` struct
- [ ] Parse `"additionalItems"` in schema parsing function
- [ ] Validate in sequence validation block
- [ ] Unit tests (denied, schema, interaction with
  array-form items, no effect with prefixItems)
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 3: Update feature-log

Record the new keywords in `rlsp-yaml/docs/feature-log.md`.

- [ ] Add entry for `minProperties`/`maxProperties`
- [ ] Add entry for `additionalItems`

## Decisions

- **Reuse `AdditionalProperties` enum for `additionalItems`:**
  Both keywords have identical semantics (false-or-schema).
  Creating a separate `AdditionalItems` enum would add a
  type with the same shape for no benefit. The name mismatch
  is acceptable since the field name `additional_items`
  makes the context clear.

- **`additionalItems` only for array-form `items`:**
  In Draft 2020-12, `additionalItems` is ignored when
  `prefixItems` is present (because `items` takes its role).
  We mirror this by only parsing `additionalItems` when
  `prefix_items` came from array-form `items`, not from
  `prefixItems`.
