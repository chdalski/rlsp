**Repository:** root
**Status:** Completed (2026-03-16)
**Created:** 2026-03-16

## Goal

Format JSON examples in hover with proper indentation using
fenced code blocks instead of single-line compact JSON. This
makes complex examples (objects, arrays) readable at a glance.

## Context

- `hover.rs:587-598` — `json_value_to_display_string` uses
  `value.to_string()` for objects/arrays → compact single-line
- `hover.rs:567-582` — examples are rendered as markdown list
  items: `- {compact_json}`
- Constants: `MAX_EXAMPLES = 3`, `MAX_EXAMPLE_LEN = 100`
- Hover uses `MarkupKind::Markdown` — fenced code blocks are
  supported
- Simple values (strings, numbers, booleans) don't need
  pretty-printing — only objects and arrays benefit

## Steps

- [x] Clarify formatting with user (fenced code blocks)
- [ ] Implement hover formatting

## Tasks

### Task 1: Pretty-print JSON examples in hover

1. **hover.rs** — Modify the examples rendering block
   (~lines 567-582). For each example value:
   - If it's an object or array: use `serde_json::to_string_pretty()`
     and wrap in a fenced code block (````json ... ````)
   - If it's a simple value (string, number, bool, null):
     keep the existing list-item format (`- value`)

   The truncation via `MAX_EXAMPLE_LEN` should still apply to
   the pretty-printed output — but since pretty-printed JSON
   is multi-line, truncation should be by total character count
   of the pretty string, not per-line.

2. **Update `json_value_to_display_string`** or replace it
   with inline logic in the examples block that dispatches
   on value type.

3. **Tests** — Update existing hover tests that assert on
   example formatting. Add tests:
   - Object example → verify fenced code block with indentation
   - Array example → verify fenced code block
   - Simple value → verify list-item format (no code block)
   - Mixed examples → verify correct dispatch per type

Files:
- `rlsp-yaml/src/hover.rs`

Acceptance criteria:
- [ ] Object/array examples use fenced json code blocks
- [ ] Simple values use list-item format
- [ ] Pretty-printed JSON is properly indented
- [ ] Truncation still works for very long examples
- [ ] Existing tests updated and pass
- [ ] `cargo clippy` and `cargo test` pass

## Decisions

- **Fenced code blocks** — user confirmed; provides syntax
  highlighting in editors
- **Simple values stay as list items** — no benefit to code
  blocks for `true`, `42`, `"hello"`
- **Truncation** — apply to full pretty-printed string; if
  truncated, skip the code block and fall back to inline
  (a truncated code block looks broken)
