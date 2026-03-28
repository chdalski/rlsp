# Complete JSON Schema Support (Draft-04 through 2020-12)

**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-28

## Goal

Bring rlsp-yaml's JSON Schema validation to full spec
compliance across Draft-04, Draft-06, Draft-07, Draft
2019-09, and Draft 2020-12. The current implementation
parses Draft-04/07 but validates only a subset of
keywords — several parsed fields (pattern, minimum,
maximum, minLength, maxLength) are never checked, and
many spec keywords are missing entirely.

## Context

- `JsonSchema` struct in `schema.rs` (line 72) holds
  parsed schema data — some fields exist but are unused
  in validation
- `schema_validation.rs` implements the validation walk
  via `validate_node` — type, enum, required, properties,
  additionalProperties, items, and allOf/anyOf/oneOf
  composition are implemented
- `parse_schema_with_root` in `schema.rs` (line 493)
  handles parsing — needs extension for new keywords
- Completion (`completion.rs`) uses schema properties,
  enum values, and required fields — `patternProperties`
  and `const` should feed into completion too
- Hover (`hover.rs`) uses schema descriptions — new
  keywords like `const` should show in hover
- The existing `$ref` resolver (line 669) only handles
  local JSON Pointers (`#/definitions/Foo`) — Draft
  2019-09+ adds `$anchor` and `$dynamicRef`
- Security: `pattern` and `patternProperties` compile
  user-supplied regexes — need ReDoS guards (timeout or
  regex size limit)

### Keyword inventory by draft

**Draft-04 (missing):**
`patternProperties`, `minItems`, `maxItems`, `uniqueItems`,
`not`, `multipleOf`, `exclusiveMinimum` (boolean),
`exclusiveMaximum` (boolean), `dependencies`

**Draft-04 (parsed but not validated):**
`pattern`, `minimum`, `maximum`, `minLength`, `maxLength`

**Draft-06 (missing):**
`const`, `contains`, `propertyNames`,
`exclusiveMinimum` (number), `exclusiveMaximum` (number)

**Draft-07 (missing):**
`if`, `then`, `else`

**Draft 2019-09 (missing):**
`dependentRequired`, `dependentSchemas`,
`minContains`, `maxContains`, `$anchor`,
`unevaluatedProperties`, `unevaluatedItems`, `$vocabulary`

**Draft 2020-12 (missing):**
`prefixItems`, `$dynamicRef`, `$dynamicAnchor`

## Steps

- [ ] Implement scalar constraints (pattern, minLength, maxLength, minimum, maximum, exclusiveMinimum, exclusiveMaximum, multipleOf, const)
- [ ] Implement `not` keyword
- [ ] Implement `patternProperties`
- [ ] Implement array constraints (minItems, maxItems, uniqueItems)
- [ ] Implement `propertyNames`
- [ ] Implement `dependencies` / `dependentRequired` / `dependentSchemas`
- [ ] Implement `if` / `then` / `else`
- [ ] Implement `contains` / `minContains` / `maxContains`
- [ ] Implement `prefixItems`
- [ ] Implement `$anchor` / `$dynamicRef` / `$dynamicAnchor`
- [ ] Implement `unevaluatedProperties` / `unevaluatedItems`
- [ ] Implement `$vocabulary`

## Tasks

### Task 1: Scalar constraints

Activate validation for already-parsed fields and add
missing scalar keywords. All changes are scalar-value
checks with no structural validation changes.

**Files:** `schema.rs`, `schema_validation.rs`

**New fields in `JsonSchema`:**
- `exclusive_minimum: Option<f64>` (Draft-06+ number form)
- `exclusive_maximum: Option<f64>` (Draft-06+ number form)
- `exclusive_minimum_draft04: Option<bool>` (Draft-04 boolean form)
- `exclusive_maximum_draft04: Option<bool>` (Draft-04 boolean form)
- `multiple_of: Option<f64>`
- `const_value: Option<serde_json::Value>`

**Parsing in `parse_schema_with_root`:**
- `exclusiveMinimum`: if number → `exclusive_minimum`; if bool → `exclusive_minimum_draft04`
- `exclusiveMaximum`: same pattern
- `multipleOf`: f64
- `const`: clone the Value

**Validation in `validate_node` (add after enum check, before mapping checks):**
- String node + `pattern` → compile regex, test match. Use `regex` crate with size limit (pattern length ≤ 1024 chars as ReDoS guard).
- String node + `minLength`/`maxLength` → check `str.chars().count()`
- Numeric node + `minimum`/`maximum` → f64 comparison
- Numeric node + `exclusive_minimum`/`exclusive_maximum` → strict comparison
- Numeric node + Draft-04 boolean exclusives → modify min/max to strict
- Numeric node + `multiple_of` → `(value / multiple_of - (value / multiple_of).round()).abs() < f64::EPSILON`
- Any node + `const_value` → convert to JSON and compare

**Diagnostic codes:** `schemaPattern`, `schemaMinLength`, `schemaMaxLength`, `schemaMinimum`, `schemaMaximum`, `schemaMultipleOf`, `schemaConst`

- [ ] Add new fields to `JsonSchema` struct
- [ ] Parse new keywords in `parse_schema_with_root`
- [ ] Add string constraint validation (pattern, minLength, maxLength)
- [ ] Add numeric constraint validation (minimum, maximum, exclusive variants, multipleOf)
- [ ] Add const validation
- [ ] Unit tests for each constraint type
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 2: `not` keyword

**Files:** `schema.rs`, `schema_validation.rs`

**New field:** `not: Option<Box<JsonSchema>>`

**Parsing:** Parse `"not"` object as sub-schema.

**Validation:** In `validate_composition`, run sub-schema
validation into a scratch vec. If scratch is empty (value
matches), emit a diagnostic — the value should NOT match.

**Diagnostic code:** `schemaNot`

- [ ] Add `not` field to `JsonSchema`
- [ ] Parse in `parse_schema_with_root`
- [ ] Validate in `validate_composition`
- [ ] Unit tests
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 3: `patternProperties`

**Files:** `schema.rs`, `schema_validation.rs`, `completion.rs`

**New field:** `pattern_properties: Option<Vec<(String, JsonSchema)>>`

Using a `Vec` of tuples (not `HashMap`) to preserve pattern
order from the schema. Each tuple is (regex_pattern, schema).

**Parsing:** Iterate `"patternProperties"` object entries,
parse each value as sub-schema.

**Validation in `validate_mapping`:**
- For each key not matched by `properties`, test against
  each pattern in `pattern_properties`
- If matched, validate value against the pattern's schema
- Only check `additionalProperties` for keys matched by
  neither `properties` nor `patternProperties`

**Completion:** When building completions for a mapping,
include properties from `patternProperties` schemas where
the pattern is a simple literal (skip complex regex
patterns in completion).

**Security:** Regex patterns from schemas are untrusted
input. Limit pattern length (≤ 1024 chars) and use
`regex` crate which has built-in backtracking limits.

- [ ] Add `pattern_properties` field to `JsonSchema`
- [ ] Parse in `parse_schema_with_root`
- [ ] Update `validate_mapping` to check pattern properties
- [ ] Adjust `additionalProperties` check to account for pattern matches
- [ ] Unit tests (matching, non-matching, interaction with additionalProperties)
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 4: Array constraints (`minItems`, `maxItems`, `uniqueItems`)

**Files:** `schema.rs`, `schema_validation.rs`

**New fields:**
- `min_items: Option<u64>`
- `max_items: Option<u64>`
- `unique_items: Option<bool>`

**Validation:** In `validate_node`, after sequence-specific
checks:
- Check `seq.len()` against min/max
- For `uniqueItems`: convert each item to JSON, collect
  into a set, compare lengths

**Diagnostic codes:** `schemaMinItems`, `schemaMaxItems`, `schemaUniqueItems`

- [ ] Add fields to `JsonSchema`
- [ ] Parse in `parse_schema_with_root`
- [ ] Add sequence constraint validation
- [ ] Unit tests
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 5: `propertyNames`

**Files:** `schema.rs`, `schema_validation.rs`

**New field:** `property_names: Option<Box<JsonSchema>>`

**Validation in `validate_mapping`:** For each key in the
mapping, wrap it as a YAML string node and validate against
the `propertyNames` schema.

**Diagnostic code:** `schemaPropertyNames`

- [ ] Add field to `JsonSchema`
- [ ] Parse in `parse_schema_with_root`
- [ ] Validate in `validate_mapping`
- [ ] Unit tests
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 6: `dependencies` / `dependentRequired` / `dependentSchemas`

**Files:** `schema.rs`, `schema_validation.rs`

**New fields:**
- `dependent_required: Option<HashMap<String, Vec<String>>>`
- `dependent_schemas: Option<HashMap<String, JsonSchema>>`

**Parsing:** Handle three forms:
1. `"dependencies"` (Draft-04): if value is array → `dependent_required`; if object → `dependent_schemas`
2. `"dependentRequired"` (2019-09): → `dependent_required`
3. `"dependentSchemas"` (2019-09): → `dependent_schemas`

Merge Draft-04 and 2019-09 forms into the same fields.

**Validation in `validate_mapping`:**
- `dependent_required`: if trigger key is present, check all dependency keys are present
- `dependent_schemas`: if trigger key is present, validate entire mapping against dependency schema

**Diagnostic codes:** `schemaDependentRequired`, `schemaDependentSchemas`

- [ ] Add fields to `JsonSchema`
- [ ] Parse all three keyword forms
- [ ] Validate dependent_required in `validate_mapping`
- [ ] Validate dependent_schemas in `validate_mapping`
- [ ] Unit tests (Draft-04 form, 2019-09 form, both)
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 7: `if` / `then` / `else`

**Files:** `schema.rs`, `schema_validation.rs`

**New fields:**
- `if_schema: Option<Box<JsonSchema>>`
- `then_schema: Option<Box<JsonSchema>>`
- `else_schema: Option<Box<JsonSchema>>`

**Validation in `validate_composition`:**
1. Validate node against `if_schema` into scratch vec
2. If scratch is empty (matches if): validate against `then_schema` if present
3. If scratch is non-empty (doesn't match if): validate against `else_schema` if present

**Diagnostic codes:** Diagnostics come from the then/else sub-schema validation, not from if/then/else itself.

- [ ] Add fields to `JsonSchema`
- [ ] Parse in `parse_schema_with_root`
- [ ] Validate in `validate_composition`
- [ ] Unit tests (if+then, if+else, if+then+else, no match)
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 8: `contains` / `minContains` / `maxContains`

**Files:** `schema.rs`, `schema_validation.rs`

**New fields:**
- `contains: Option<Box<JsonSchema>>`
- `min_contains: Option<u64>`
- `max_contains: Option<u64>`

**Validation:** In sequence-specific checks:
1. Count items matching `contains` schema (validate into scratch, count empties)
2. Default `minContains` to 1 when `contains` is present
3. Check count against `minContains`/`maxContains`

**Diagnostic codes:** `schemaContains`

- [ ] Add fields to `JsonSchema`
- [ ] Parse in `parse_schema_with_root`
- [ ] Add contains validation in sequence checks
- [ ] Unit tests
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 9: `prefixItems`

**Files:** `schema.rs`, `schema_validation.rs`

**New field:** `prefix_items: Option<Vec<JsonSchema>>`

**Validation:** In sequence-specific checks, validate each
array element at index `i` against `prefix_items[i]` if
`i < prefix_items.len()`. Items beyond `prefix_items`
length fall through to `items` schema (if present).

This replaces the Draft-04 tuple form where `items` was
an array. For backwards compatibility, during parsing: if
`items` is an array (not an object), treat it as
`prefix_items`.

- [ ] Add `prefix_items` field to `JsonSchema`
- [ ] Parse `"prefixItems"` and array-form `"items"` as prefix_items
- [ ] Validate positional items
- [ ] Ensure items schema applies to remainder
- [ ] Unit tests
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 10: `$anchor` / `$dynamicRef` / `$dynamicAnchor`

**Files:** `schema.rs`

**Changes to ref resolution infrastructure:**
- During parsing, collect `$anchor` values into a
  `HashMap<String, JsonSchema>` anchor registry on the
  root schema
- `$dynamicAnchor` registers an anchor that can be
  overridden by a re-definition in an outer scope
- `$dynamicRef` resolves like `$ref` but checks the
  dynamic scope chain for `$dynamicAnchor` overrides
- Update `resolve_ref` to check anchor registry when ref
  starts with `#` followed by a non-`/` character

**New fields:**
- `anchor: Option<String>`
- `dynamic_anchor: Option<String>`
- `dynamic_ref: Option<String>`

This task requires careful design of the scope chain for
dynamic resolution. Consult the JSON Schema 2020-12 spec
section on dynamic references.

- [ ] Add anchor registry to schema parsing context
- [ ] Parse `$anchor`, `$dynamicAnchor`, `$dynamicRef`
- [ ] Update ref resolution to check anchors
- [ ] Implement dynamic scope chain for `$dynamicRef`
- [ ] Unit tests (static anchors, dynamic override)
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 11: `unevaluatedProperties` / `unevaluatedItems`

**Files:** `schema.rs`, `schema_validation.rs`

This is the most complex keyword to implement correctly.
It requires tracking which properties/items were
"evaluated" during composition (allOf/anyOf/oneOf/
if/then/else) and flagging any that weren't.

**New fields:**
- `unevaluated_properties: Option<AdditionalProperties>`
- `unevaluated_items: Option<Box<JsonSchema>>`

**Approach:** Add an `EvaluationContext` struct that tracks
evaluated property names and array indices. Pass it through
the validation walk. After composition, check remaining
properties/items against the unevaluated schema.

This is high complexity and may require refactoring
`validate_node` to accept and return evaluation context.

- [ ] Design `EvaluationContext` struct
- [ ] Add fields to `JsonSchema`
- [ ] Parse in `parse_schema_with_root`
- [ ] Thread evaluation context through validation walk
- [ ] Implement unevaluated checks after composition
- [ ] Unit tests (with allOf, anyOf, oneOf, if/then/else)
- [ ] Verify `cargo clippy` and `cargo test` pass

### Task 12: `$vocabulary`

**Files:** `schema.rs`

**Approach:** Parse `$vocabulary` from the schema's
`$schema` meta-schema URI. Build a vocabulary registry
that maps vocabulary URIs to keyword sets. When a
vocabulary is not required and not recognized, its
keywords are ignored. When a vocabulary is required
and not recognized, emit a warning.

This is the lowest priority — most real-world schemas
don't use custom vocabularies. The main value is
correctly handling the standard vocabularies defined
in 2019-09 and 2020-12.

- [ ] Parse `$vocabulary` from meta-schema
- [ ] Build vocabulary registry with standard vocabularies
- [ ] Conditionally enable/disable keywords based on vocabulary
- [ ] Unit tests
- [ ] Verify `cargo clippy` and `cargo test` pass

## Decisions

- **Dual-form `exclusiveMinimum`/`exclusiveMaximum`:**
  Parse Draft-04 boolean form and Draft-06+ number form
  into separate fields. Apply both during validation.
  This avoids needing to detect which draft a schema
  targets.

- **`patternProperties` storage:** Use `Vec<(String, Schema)>`
  not `HashMap` to preserve pattern ordering from the
  schema, which matters for `additionalProperties`
  interaction.

- **ReDoS guard:** Limit regex pattern length to 1024
  characters and rely on the `regex` crate's built-in
  backtracking limits. Schema-supplied patterns are
  untrusted input.

- **Draft-04 `items` as array:** Treat array-form `items`
  as `prefixItems` during parsing for backwards
  compatibility with Draft-04 tuple validation.

- **`dependencies` merging:** Merge Draft-04 `dependencies`
  and Draft 2019-09 `dependentRequired`/`dependentSchemas`
  into unified fields during parsing so validation logic
  only needs one code path.
