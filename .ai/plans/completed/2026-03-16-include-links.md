**Repository:** root
**Status:** Completed (2026-03-16)
**Created:** 2026-03-16

## Goal

Add `!include` document links so that file paths following
`!include` tags in YAML appear as clickable links in the editor.
This lets users navigate to included files directly from the
YAML document.

## Context

- `src/document_links.rs` already has `find_document_links(text) -> Vec<DocumentLink>`
  that detects URLs via regex
- The server handler in `server.rs` has access to the document URI
  (needed for resolving relative paths) but doesn't pass it to
  `find_document_links` currently
- `!include` tags appear as `!include path/to/file.yaml` in raw text
- Paths can be absolute or relative to the current document
- Need to resolve relative paths against the document's directory
- Must skip `!include` inside quoted strings (the validators module
  has `is_inside_quotes` logic we can reference)
- Single task — extends existing module

### Key files

- `src/document_links.rs` — extend existing function
- `src/server.rs` — pass document URI to the function

### Patterns to follow

- Existing URL detection regex pattern in `document_links.rs`
- Quote-aware scanning from `validators.rs`

## Steps

- [x] Clarify requirements with user
- [x] Analyze codebase
- [x] Task 1: Add !include link detection to document_links (31adaa7)

## Tasks

### Task 1: Add !include link detection to document_links

**A) Extend `find_document_links` in `src/document_links.rs`:**

Change signature to accept optional base URI:
```rust
pub fn find_document_links(text: &str, base_uri: Option<&Url>) -> Vec<DocumentLink>
```

Add `!include` detection after existing URL detection:
1. Scan each line for `!include ` pattern (with trailing space)
2. Skip occurrences inside quoted strings
3. Extract the file path after `!include `
4. Resolve the path:
   - If absolute path, convert to `file://` URL
   - If relative path, resolve against `base_uri`'s directory
   - If `base_uri` is None, skip relative paths
5. Create a `DocumentLink` with:
   - Range covering just the file path (not the `!include` prefix)
   - Target: the resolved `file://` URL
   - Tooltip: `Some("Open included file")`

**B) Update server handler in `src/server.rs`:**

Pass `Some(&uri)` to `find_document_links`:
```rust
let links = crate::document_links::find_document_links(&text, Some(&uri));
```

**Tests:**
1. `!include foo.yaml` with base URI produces link to resolved path
2. `!include /absolute/path.yaml` produces file:// link
3. `!include ../relative.yaml` resolves correctly
4. `!include` inside quoted string is skipped
5. No `!include` in document returns only URL links
6. `base_uri: None` skips relative include paths
7. Multiple `!include` on different lines

## Decisions

- **Optional base_uri** — backwards compatible; when None,
  only absolute include paths produce links
- **Range covers path only** — clicking the path feels more
  natural than clicking the `!include` tag
- **Tooltip** — "Open included file" gives clear affordance
- **No filesystem validation** — we don't check if the file
  exists; the link is best-effort (same as URL links)
