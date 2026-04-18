**Repository:** root
**Status:** Completed (2026-03-16)
**Created:** 2026-03-16

## Goal

When completing a key in a mapping that has required siblings,
offer an additional completion item that inserts all remaining
required properties at once as an LSP snippet with tab-stop
placeholders. This saves users from adding required properties
one by one.

## Context

- `completion.rs:315-372` — `collect_schema_properties` builds
  completion items from schema properties; no items currently
  use `insert_text` or `insert_text_format`
- `server.rs:180-183` — `CompletionOptions` does not advertise
  snippet support; needs updating
- `schema.rs:70-92` — `JsonSchema` has `required: Option<Vec<String>>`
  and `properties: Option<HashMap<String, Self>>`
- LSP snippet syntax: `key: ${1:value}` with tab-stop placeholders
- Snippet format: key-value pairs only, no trailing content
- The `present` set (keys already in the mapping) is available
  in `collect_schema_properties` — required keys already present
  must be excluded from the snippet

## Steps

- [x] Clarify snippet format with user
- [ ] Implement snippet completion

## Tasks

### Task 1: Add multi-required snippet completion

1. **server.rs** (~line 180): Update `CompletionOptions` to
   signal snippet support. The `CompletionOptions` struct does
   not have a direct snippet flag — snippet support is signaled
   by setting `insert_text_format` on individual completion
   items. No server capability change needed beyond what exists.

2. **completion.rs** — Add `InsertTextFormat` to imports.
   In `schema_key_completions` (after `collect_schema_properties`
   returns), check if the schema has required properties that
   are missing from `present`. If 2+ required properties are
   missing, build an additional `CompletionItem` with:
   - `label`: something like "Insert all required properties"
   - `kind`: `CompletionItemKind::SNIPPET`
   - `insert_text_format`: `Some(InsertTextFormat::SNIPPET)`
   - `insert_text`: snippet body with all missing required
     properties, each on its own line with tab-stop placeholders
   - `sort_text`: `Some("!".to_string())` — `!` sorts before
     alphanumeric, putting it at the top
   - `detail`: count of properties being inserted

   Snippet body format (for indent level matching the cursor):
   ```
   name: ${1:value}\nage: ${2:0}\nemail: ${3:value}
   ```
   Use schema type info to pick smarter default placeholders:
   - string → `""`
   - integer/number → `0`
   - boolean → `false`
   - object → `{}`
   - array → `[]`
   - no type info → `value`

3. **Tests** — Add tests:
   - Schema with 3 required props, 0 present → snippet item
     with all 3 in insert_text
   - Schema with 3 required props, 2 present → no snippet
     (only 1 missing, not worth a snippet)
   - Schema with 0 required props → no snippet item
   - Verify snippet has correct tab-stop numbering
   - Verify `insert_text_format` is `SNIPPET`

Files:
- `rlsp-yaml/src/completion.rs`

Acceptance criteria:
- [ ] Snippet completion item appears when 2+ required props missing
- [ ] Snippet uses correct LSP placeholder syntax
- [ ] Placeholder defaults match schema types
- [ ] sort_text puts snippet at top of list
- [ ] No snippet when <2 required props are missing
- [ ] `cargo clippy` and `cargo test` pass

## Decisions

- **Snippet body format** — key-value pairs only, no trailing
  newline or cursor position after last placeholder
- **Threshold** — only offer snippet when 2+ required properties
  are missing; a single missing property is already covered by
  the normal completion item
- **Sort position** — top of list via `!` prefix, since this
  is the most useful completion when multiple required props
  are missing
- **Default placeholders** — type-aware defaults from schema
