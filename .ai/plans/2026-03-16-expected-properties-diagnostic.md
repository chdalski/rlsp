**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-16

## Goal

Enrich "Missing required property" diagnostics to list all
expected (required) properties from the schema. This helps
users see at a glance which properties they need to add,
rather than having to look up the schema themselves.

## Context

- `schema_validation.rs:151-172` emits one `schemaRequired`
  diagnostic per missing required property with the message:
  `Missing required property 'X' at /path`
- The `required` list from the schema is already available
  at the call site — we just need to format it into the
  message
- The `MAX_ENUM_DISPLAY` pattern already exists for
  truncating long lists in enum diagnostics — we should
  follow the same pattern for required property lists
- This is a low-risk, small change touching only the
  message format string and associated tests

## Steps

- [x] Clarify message format with user (all required properties)
- [ ] Implement message enhancement and tests

## Tasks

### Task 1: Add expected properties to required-property diagnostic

Modify the `schemaRequired` diagnostic message in
`schema_validation.rs` to append the full list of required
properties. Update existing tests that assert on message
content, and add a new test verifying the expected
properties appear in the message.

Files:
- `rlsp-yaml/src/schema_validation.rs` — production code
  (line ~159-170) and tests

Acceptance criteria:
- [ ] Diagnostic message includes "Expected properties: a, b, c"
- [ ] Long required lists are truncated (reuse MAX_ENUM_DISPLAY)
- [ ] Existing tests pass with updated assertions
- [ ] New test verifies expected properties in message
- [ ] `cargo clippy` and `cargo test` pass

## Decisions

- **Show all required, not just missing** — user confirmed;
  showing the full required list gives context even when
  only one property is missing
- **Truncation** — reuse `MAX_ENUM_DISPLAY` (5) to cap the
  listed properties, consistent with enum value display
