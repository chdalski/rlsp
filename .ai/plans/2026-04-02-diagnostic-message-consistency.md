# Diagnostic Message Consistency

**Repository:** root
**Status:** InProgress
**Created:** 2026-04-02

## Goal

Standardize all user-facing diagnostic messages in
rlsp-yaml for consistency, actionability, and sufficient
context. An audit found mixed sentence structures,
draft-dependent phrasing for the same error, passive
non-actionable messages, and missing diagnostic codes.

## Context

- Audit report:
  `.ai/reports/2026-04-02-diagnostic-message-audit.md`
- Messages are spread across three files:
  `schema_validation.rs` (~35 message types),
  `validators.rs` (7 message types), `parser.rs` (1)
- Current inconsistencies:
  1. Mixed subject phrasing: "Type mismatch: expected ..."
     vs "Value at ..." vs "Array at ..." vs "Missing
     required property '...' at ..."
  2. Numeric min/max messages differ between Draft-04 and
     Draft-06+ for the same logical error
  3. Flow style messages are passive ("detected") not
     actionable
  4. anyOf/oneOf failures give no detail
  5. `not` schema message is opaque
  6. Parser errors lack a diagnostic code
  7. `SchemaError` Display is lowercase while diagnostics
     are capitalized

### Target style

Standardize on: `{TypedSubject} at {path} {verb phrase}`.

- TypedSubject = Value, Array, Object, String, Property
  (matches what the diagnostic is about)
- Path uses existing `format_path()` (dot-separated,
  `<root>` for empty)
- Verb phrase describes the problem

Examples:
- `Value at spec.replicas does not match type: expected
  integer, got string`
- `Array at spec.containers has 0 items, minimum is 1`
- `String at metadata.name is too short: 1 char (minimum 3)`
- `Value at spec.type must be one of: ClusterIP, NodePort,
  LoadBalancer`

## Steps

- [x] Standardize schema validation messages
- [x] Standardize validator messages
- [x] Add diagnostic code to parser errors
- [x] Capitalize SchemaError Display
- [x] Verify `cargo clippy --all-targets` and `cargo test`

## Tasks

### Task 1: Standardize schema validation messages — `44ceffe`

Consult the test engineer for a test list — this task
modifies ~35 message strings and every existing test that
asserts on message content will need updating.

**File:** `schema_validation.rs`

**Changes by message group:**

**Type mismatch** (line 279):
- Old: `Type mismatch: expected {type}, got {type} at {path}`
- New: `Value at {path} does not match type: expected {type},
  got {type}`

**Numeric min/max** — unify Draft-04 and Draft-06+ messages:
- Old (Draft-04): `Value at {path} is below minimum {min}
  ({bound})` where bound is "inclusive"/"exclusive"
- Old (Draft-06+): `Value at {path} must be greater than
  {min} (exclusive minimum)`
- New (both): `Value at {path} is below minimum {min}`
  and `Value at {path} is below exclusive minimum {min}`
  (similarly for maximum). Drop the parenthetical — the
  word "exclusive" in the message is clearer than
  "(exclusive)".

**anyOf/oneOf failures** — add branch count:
- Old: `Value at {path} does not match any of the allowed
  schemas`
- New: `Value at {path} does not match any of the {n}
  allowed schemas (anyOf)`
- Old: `Value at {path} does not match any of the oneOf
  schemas`
- New: `Value at {path} does not match any of the {n}
  oneOf schemas`
- Old: `Value at {path} matches more than one of the oneOf
  schemas`
- New: `Value at {path} matches {n} of the {total} oneOf
  schemas (expected exactly 1)`

**`not` schema** — make concrete:
- Old: `Value at {path} must not match the excluded schema`
- New: `Value at {path} must not match the schema defined
  in 'not'`

**No subject-phrasing changes needed** for: Array, Object,
String, Property messages — these already use typed
subjects. Only the "Type mismatch:" label prefix and the
"Missing required property" structure need alignment.

**Required property** (line 1455):
- Old: `Missing required property '{key}' at {path}.
  Expected properties: {list}.`
- New: `Object at {path} is missing required property
  '{key}'. Expected: {list}.`

**Verification:**
- Update all test assertions that match on message text
- [x] `cargo fmt`
- [x] `cargo clippy --all-targets` — zero warnings
- [x] `cargo test` — all 1101 tests pass

### Task 2: Standardize validator and parser messages — `f262004`

**Files:** `validators.rs`, `parser.rs`

**Flow style** (validators.rs lines 216, 233):
- Old: `Flow mapping style detected`
- New: `Flow mapping style: use block style instead`
- Old: `Flow sequence style detected`
- New: `Flow sequence style: use block style instead`

**Parser errors** (parser.rs line 26):
- Add `code: Some(NumberOrString::String("yamlSyntax".to_string()))`
  to the parse error diagnostic. Currently `code` is not
  set (falls through to `Diagnostic::default()` which is
  `None`).

**SchemaError Display** (schema.rs line 71):
- Capitalize first letter of each variant's message:
  - `fetch failed:` → `Fetch failed:`
  - `schema response exceeded size limit` →
    `Schema response exceeded size limit`
  - `schema parse failed:` → `Schema parse failed:`
  - `schema nesting depth exceeded limit` →
    `Schema nesting depth exceeded limit`
  - `remote fetch count exceeded limit` →
    `Remote fetch count exceeded limit`
  - `unexpected content type:` →
    `Unexpected content type:`
  - `URL not permitted:` — already capitalized

**Verification:**
- Update affected test assertions
- [x] `cargo fmt`
- [x] `cargo clippy --all-targets` — zero warnings
- [x] `cargo test` — all 1101 tests pass

## Decisions

- **Keep typed subjects** (Array, Object, String) rather
  than changing everything to "Value" — the typed subject
  tells the user what kind of YAML node has the problem,
  which is useful context. Only normalize the outliers
  (Type mismatch label, Missing required property
  structure) to use the same `{Subject} at {path}` pattern.

- **Drop parenthetical bound labels** — "(inclusive)" and
  "(exclusive)" are JSON Schema jargon. Using "exclusive
  minimum" as a phrase is clearer to users who don't know
  the spec.

- **Don't enumerate failed anyOf/oneOf branches** — listing
  why each branch failed would make messages very long and
  is a known hard problem. Adding the branch count is a
  pragmatic middle ground.
