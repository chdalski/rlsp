**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-16

## Goal

Mark deprecated JSON Schema properties in completion results
with `CompletionItemTag::DEPRECATED` (strikethrough in editors)
and de-prioritize them via `sort_text` so they appear at the
bottom of the list. This helps users avoid deprecated properties
without hiding them entirely.

## Context

- `completion.rs:324-346` iterates `schema.properties` and
  builds `CompletionItem` structs — this is where the tag
  and sort_text need to be set
- `JsonSchema` (schema.rs:70-92) currently has no `deprecated`
  field — needs to be added
- `parse_schema_with_root` (schema.rs:376-378) parses fields
  from JSON — needs to read `"deprecated"` as `Option<bool>`
- LSP types: `CompletionItemTag::DEPRECATED` exists in
  `tower_lsp::lsp_types`, and `CompletionItem` has a `tags`
  field (`Option<Vec<CompletionItemTag>>`)
- `sort_text` on `CompletionItem` controls ordering — prefix
  deprecated items with `"~"` (sorts after alphanumeric)
- This is a single vertical slice: schema parsing + completion
  logic + tests

## Steps

- [x] Clarify behavior with user (tag + de-prioritize)
- [ ] Implement deprecated property handling

## Tasks

### Task 1: Add deprecated flag support to schema and completion

Three touch points:

1. **schema.rs** — Add `pub deprecated: Option<bool>` to
   `JsonSchema` struct. Parse it in `parse_schema_with_root`
   as `obj.get("deprecated").and_then(Value::as_bool)`.

2. **completion.rs** — In `collect_schema_properties`
   (~line 339-345), when building each `CompletionItem`:
   - Check `prop_schema.deprecated == Some(true)`
   - If deprecated: set `tags: Some(vec![CompletionItemTag::DEPRECATED])`
     and `sort_text: Some(format!("~{key}"))` (tilde sorts
     after all alphanumeric keys)
   - If not deprecated: leave `tags` and `sort_text` as default

3. **Tests** — Add tests in both files:
   - schema.rs: test that `deprecated: true` parses correctly
   - completion.rs: test that deprecated properties get the
     DEPRECATED tag and `~` sort_text prefix; test that
     non-deprecated properties don't get tags

Files:
- `rlsp-yaml/src/schema.rs`
- `rlsp-yaml/src/completion.rs`

Acceptance criteria:
- [ ] `JsonSchema.deprecated` field exists and parses
- [ ] Deprecated completions have `CompletionItemTag::DEPRECATED`
- [ ] Deprecated completions have `sort_text` starting with `~`
- [ ] Non-deprecated completions unchanged
- [ ] `cargo clippy` and `cargo test` pass

## Decisions

- **Tag + de-prioritize** — user confirmed; best UX since
  deprecated properties remain discoverable but don't crowd
  out current properties
- **Sort prefix `~`** — tilde (U+007E) sorts after all
  ASCII letters and digits, pushing deprecated items to the
  bottom without a complex comparator
