**Repository:** root
**Status:** Completed (2026-03-16)
**Created:** 2026-03-16

## Goal

Add on-type formatting to rlsp-yaml so that pressing Enter after
a colon, dash, or block scalar indicator automatically indents
the next line. This is the most impactful formatting feature for
YAML editing — correct indentation is critical and error-prone.

## Context

- tower-lsp 0.20 provides `on_type_formatting` method returning
  `Result<Option<Vec<TextEdit>>>`
- `DocumentOnTypeFormattingParams` contains position, typed char,
  and `FormattingOptions` (tab_size, insert_spaces)
- The trigger character is `\n` (newline) — the server receives
  the notification after the user presses Enter
- When the trigger is `\n`, the position is on the NEW line
  (the line just created), and we need to look at the PREVIOUS
  line to decide indentation
- Capability: `DocumentOnTypeFormattingOptions` with
  `first_trigger_character: "\n"`
- Single task — small, self-contained feature

### Key files

- `src/on_type_formatting.rs` — new module with pure function
- `src/server.rs` — capability + handler
- `src/lib.rs` — module declaration

### Patterns to follow

- Pure function: `format_on_type(text, position, ch, tab_size) -> Vec<TextEdit>`
- Indentation logic from `code_actions.rs` (line length, trim patterns)

## Steps

- [x] Clarify requirements with user
- [x] Analyze codebase
- [x] Task 1: Add on-type formatting module and server integration (a408482)

## Tasks

### Task 1: Add on-type formatting module and server integration

**A) New module `src/on_type_formatting.rs`:**

Pure function:
```rust
pub fn format_on_type(text: &str, position: Position, ch: &str, tab_size: u32) -> Vec<TextEdit>
```

Auto-indent logic (only trigger on `\n`):
1. Look at the previous line (position.line - 1)
2. Determine the previous line's indentation level
3. Add extra indent (+tab_size spaces) when previous line:
   - Ends with `:` (mapping key expecting value)
   - Ends with `: |` or `: >` (block scalar start)
   - Ends with `: |-` or `: >-` (block scalar strip)
   - Is a sequence item `- ` with no nested content after
4. Maintain same indent when previous line is a complete
   key-value pair (e.g., `key: value`)
5. Return a TextEdit that sets the indentation on the current
   line (replace from col 0 to current position with correct
   indent)

**B) Server integration:**
- Add `pub mod on_type_formatting;` to `lib.rs`
- Add capability in `capabilities()`:
  ```rust
  document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
      first_trigger_character: "\n".to_string(),
      more_trigger_character: None,
  }),
  ```
- Add handler that reads text from document_store and calls
  the pure function

**Tests:**
1. After `key:` → indents by tab_size
2. After `key: value` → maintains same indent
3. After `- item` → maintains same indent
4. After `key: |` → indents by tab_size
5. After `key: >` → indents by tab_size
6. Nested indentation preserved (indent of previous line + extra)
7. Non-newline character returns empty vec
8. Empty document returns empty vec

## Decisions

- **Only `\n` trigger** — colon and dash triggers would fire
  mid-typing and feel intrusive; newline is the natural
  formatting point
- **tab_size from FormattingOptions** — respects editor settings
  rather than hardcoding 2 spaces
- **Pure function** — consistent with all other feature modules
