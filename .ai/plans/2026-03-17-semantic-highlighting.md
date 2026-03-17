**Repository:** root
**Status:** Completed (2026-03-17)
**Created:** 2026-03-17

## Goal

Add semantic token support so editors can provide richer
syntax highlighting for YAML elements — mapping keys,
string/number/boolean values, anchors, aliases, tags,
and comments each get distinct token types.

## Context

- `tower-lsp` 0.20 with `proposed` feature provides
  `semantic_tokens_full` method and all needed LSP types
  (`SemanticTokensParams`, `SemanticTokensResult`,
  `SemanticTokensLegend`, `SemanticTokenType`, etc.)
- Available token types: PROPERTY, STRING, NUMBER, KEYWORD,
  VARIABLE, TYPE, COMMENT, OPERATOR, NAMESPACE, etc.
- Available modifiers: DECLARATION, DEFINITION, DEPRECATED,
  READONLY, MODIFICATION, etc.
- Existing codebase pattern: pure function modules that take
  `&str` (and optionally parsed YAML) and return LSP types
- `references.rs` already has anchor/alias scanning logic
  (regex-based) that can inform the token classification
- The LSP semantic tokens protocol uses delta encoding:
  each token is encoded as (deltaLine, deltaStart, length,
  tokenType, tokenModifiers) relative to the previous token
- We only need `semantic_tokens_full` — delta and range
  support are optional optimizations for later

## Steps

- [x] Clarify approach with user
- [x] Implement semantic token provider (3402902)

## Tasks

### Task 1: Add semantic token module and capability

1. **semantic_tokens.rs** (new module): Create with:

   - `pub fn legend() -> SemanticTokensLegend` — returns
     the token type and modifier arrays. Token types:
     `[PROPERTY, STRING, NUMBER, KEYWORD, VARIABLE, TYPE,
     COMMENT]`. Modifiers: `[DECLARATION]`.

   - `pub fn semantic_tokens(text: &str) -> Vec<SemanticToken>`
     — scans the text line by line, classifying tokens:

     Token classification rules (line-based scan):
     - **Comments**: lines starting with optional whitespace
       then `#` — classify the `#...` portion as COMMENT
     - **Anchors** (`&name`): VARIABLE + DECLARATION modifier
     - **Aliases** (`*name`): VARIABLE (no modifier)
     - **Tags** (`!tag`): TYPE
     - **Mapping keys**: text before the mapping colon (`: `)
       → PROPERTY
     - **Scalar values**: text after the mapping colon:
       - Numbers (integers, floats) → NUMBER
       - Booleans (`true`/`false`/`yes`/`no`/`on`/`off`) and
         `null`/`~` → KEYWORD
       - Strings (quoted or unquoted) → STRING
     - **Block scalar indicators** (`|`, `>`, `|-`, `>-`,
       `|+`, `>+`): OPERATOR
     - **Sequence dashes** (`- `): skip (too noisy)

   The function returns `Vec<SemanticToken>` with the LSP
   delta-encoded format (each token relative to the previous).

2. **lib.rs**: Add `pub mod semantic_tokens;`

3. **server.rs — capabilities()**: Add
   `semantic_tokens_provider` to `ServerCapabilities`:
   ```rust
   semantic_tokens_provider: Some(
       SemanticTokensServerCapabilities::SemanticTokensOptions(
           SemanticTokensOptions {
               legend: crate::semantic_tokens::legend(),
               full: Some(SemanticTokensFullOptions::Bool(true)),
               range: None,
               ..SemanticTokensOptions::default()
           }
       )
   ),
   ```

4. **server.rs — semantic_tokens_full()**: Implement the
   handler following the standard pattern:
   - Get text from document_store
   - Call `crate::semantic_tokens::semantic_tokens(&text)`
   - Return `SemanticTokensResult::Tokens(SemanticTokens { result_id: None, data: tokens })`

5. **Tests** in `semantic_tokens.rs`:
   - Comment line → COMMENT token
   - Mapping key → PROPERTY token
   - String value → STRING token
   - Number value → NUMBER token
   - Boolean value → KEYWORD token
   - Null value → KEYWORD token
   - Anchor → VARIABLE with DECLARATION
   - Alias → VARIABLE without DECLARATION
   - Tag → TYPE token
   - Block scalar indicator → OPERATOR token
   - Mixed line with key, value, and comment
   - Multi-line document with various elements
   - Empty document → empty tokens
   - Delta encoding is correct (positions relative to
     previous token)

6. **server.rs tests**: Add capability advertisement test
   for semantic_tokens_provider.

Files:
- `rlsp-yaml/src/semantic_tokens.rs` (new)
- `rlsp-yaml/src/lib.rs`
- `rlsp-yaml/src/server.rs`

Acceptance criteria:
- [ ] Semantic tokens returned for all YAML element types
- [ ] Delta encoding correct per LSP spec
- [ ] Legend matches the tokens produced
- [ ] Capability advertised in ServerCapabilities
- [ ] `cargo clippy` and `cargo test` pass

## Decisions

- **Token types** — PROPERTY for keys (standard for structured
  data), VARIABLE for anchors/aliases (they are references),
  TYPE for tags (they denote YAML types), KEYWORD for
  booleans/null (language constants)
- **Line-based scan** — simpler than AST-based, handles
  comments and tags that the YAML parser strips out. The
  existing anchor/alias regex patterns from `references.rs`
  inform the approach.
- **Full tokens only** — delta and range support are
  optimizations; full is sufficient for correctness
- **No modifier for aliases** — only anchors get DECLARATION
  since they define the reference target
