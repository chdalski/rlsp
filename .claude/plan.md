# Plan: Document Links Feature

## Context

Implement LSP `textDocument/documentLink` feature for rlsp-yaml. This allows users to click on URLs in YAML files to open them in browser/editor.

The RedHat yaml-language-server only detects JSON Schema `$ref` pointers (intra-document links). We need broader URL detection for http/https/file schemes in both values and comments.

## Architecture Notes

**Current patterns observed:**
- Features are implemented as functions in dedicated modules (e.g., `folding.rs`, `hover.rs`, `references.rs`)
- Server capabilities are declared in `server.rs::Backend::capabilities()`
- LSP handlers are registered as async methods on the `Backend` struct
- Text is retrieved from the `DocumentStore` in each handler
- Tests are co-located in module files using `#[cfg(test)]` modules
- Integration tests live in `tests/` directory

**Document Links approach:**
- Create new module `src/document_links.rs` with URL detection logic
- Add handler in `server.rs` similar to `folding_range` handler
- Export module from `lib.rs`
- Register `document_link_provider` capability
- Import `DocumentLink` type from `tower_lsp::lsp_types`

**URL detection strategy:**
- Use regex to find URLs matching `http://`, `https://`, `file://` schemes
- Scan both YAML string values and comment lines
- Calculate position ranges using line/column offsets
- Return `DocumentLink` objects with target URL and range

## Tasks

### Task 1: Create document links module with URL detection
**What:** Implement the core URL detection logic in a new `src/document_links.rs` module with comprehensive tests.

**Acceptance Criteria:**
- Create `src/document_links.rs` with a `pub fn find_document_links(text: &str) -> Vec<DocumentLink>` function
- Detect URLs with schemes: `http://`, `https://`, `file://`
- Detect URLs in both:
  - YAML string values (quoted and unquoted scalars)
  - Comment lines (lines with `#`)
- Return `DocumentLink` objects with correct `range` (start/end positions) and `target` (URL string)
- Handle multi-document YAML (multiple `---` sections)
- Unit tests covering:
  - URLs in string values (quoted and unquoted)
  - URLs in comments
  - Multiple URLs per line
  - Multi-document YAML
  - No URLs (empty result)
  - Edge cases (URLs at line start/end, special characters)

### Task 2: Integrate document links into LSP server
**What:** Wire the document links feature into the LSP server handlers and capabilities.

**Acceptance Criteria:**
- Export `document_links` module from `src/lib.rs`
- Import `DocumentLink` type in `server.rs` from `tower_lsp::lsp_types`
- Add `document_link_provider: Some(OneOf::Left(true))` to `Backend::capabilities()`
- Implement `async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>>` handler
- Handler retrieves text from document store and calls `crate::document_links::find_document_links`
- Returns `None` if document not found, otherwise returns the links
- Add capability test in `server.rs` tests: `should_advertise_document_link_provider`
- Add integration test in `tests/lsp_lifecycle.rs` verifying document links work end-to-end

## Progress

- [x] Task 1: Create document links module with URL detection - **COMPLETE**
  - All three dev-team members signed off
  - 41 tests passing
  - Security review approved
  - Implementation at `/workspace/rlsp-yaml/src/document_links.rs`
- [ ] Task 2: Integrate document links into LSP server - **READY TO START**
