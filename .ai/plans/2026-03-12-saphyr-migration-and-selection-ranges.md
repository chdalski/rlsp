**Repository:** root
**Status:** Completed (2026-03-13)
**Created:** 2026-03-12
**Author:** Architect

## Goal

Migrate the `rlsp-yaml` YAML language server from `yaml-rust2` to
`saphyr` as the YAML parser, then implement LSP Selection Ranges
using saphyr's span/marker support for accurate cursor-to-node
mapping. The migration unlocks position-aware AST nodes that were
previously unavailable, improving accuracy for existing features
and enabling AST-based (rather than indentation-based) selection
ranges.

## Context

### Current yaml-rust2 usage

Six modules directly reference `yaml_rust2::Yaml`:

| Module | Usage |
|--------|-------|
| `parser.rs` | `YamlLoader::load_from_str`, `ScanError::marker()` for diagnostics |
| `document_store.rs` | Stores `Vec<Yaml>` alongside raw text |
| `hover.rs` | Pattern-matches `Yaml` enum for AST traversal + type display |
| `completion.rs` | Pattern-matches `Yaml` enum for sibling key analysis |
| `symbols.rs` | Pattern-matches `Yaml::Hash`, `Yaml::Array` for document symbol tree |
| `validators.rs` | `validate_key_ordering` takes `&[Yaml]`, matches `Yaml::Hash` keys |

Four modules do NOT use yaml-rust2 (pure text analysis):
`references.rs`, `rename.rs`, `folding.rs`, `document_links.rs`

### saphyr API (verified against 0.0.6 source)

`saphyr` is the maintained successor/fork of yaml-rust2. Its API
has **diverged significantly** from yaml-rust2 -- see "Migration
scope" below for the full variant mapping.

- **Crate:** `saphyr` (re-exports everything needed including
  `ScanError`, `Marker`). No separate `saphyr-parser` dep required.
- **`Yaml<'input>` / `YamlOwned`:** `Yaml` has a lifetime parameter
  (borrows from input `&str`). `YamlOwned` is the owned variant
  (uses `String` instead of `Cow<str>`) suitable for storage.
- **Enum variants differ from yaml-rust2:** Scalars are wrapped in
  `Value(Scalar::...)`, `Array` → `Sequence`, `Hash` → `Mapping`.
  See migration scope table for full mapping.
- **Loader:** `YamlOwned::load_from_str(text)` via `LoadableYamlNode`
  trait (not `YamlLoader::load_from_str`).
- **`ScanError::marker()`** returns `Marker` with `index()`, `line()`,
  `col()` -- same API as yaml-rust2.
- **`MarkedYaml<'input>` / `MarkedYamlOwned`:** Each node carries a
  `Span { start: Marker, end: Marker }` with byte offset, line, and
  column. `MarkedYamlOwned` is the owned variant for storage.
  This is what enables AST-based selection ranges.

### Migration scope

**UPDATE:** The migration is NOT mechanical. saphyr 0.0.6's `Yaml`
enum has completely different variant names from yaml-rust2:

| yaml-rust2 | saphyr 0.0.6 |
|------------|-------------|
| `Yaml::String(s)` | `YamlOwned::Value(Scalar::String(s))` |
| `Yaml::Integer(i)` | `YamlOwned::Value(Scalar::Integer(i))` |
| `Yaml::Real(String)` | `YamlOwned::Value(Scalar::FloatingPoint(OrderedFloat<f64>))` |
| `Yaml::Boolean(b)` | `YamlOwned::Value(Scalar::Boolean(b))` |
| `Yaml::Null` | `YamlOwned::Value(Scalar::Null)` |
| `Yaml::Array(vec)` | `YamlOwned::Sequence(vec)` |
| `Yaml::Hash(map)` | `YamlOwned::Mapping(map)` |
| `Yaml::BadValue` | `YamlOwned::BadValue` |
| `Yaml::Alias(n)` | `YamlOwned::Alias(n)` |

Key differences:
- **Lifetime:** `Yaml<'input>` has a lifetime. Must use `YamlOwned`
  (no lifetime, `String` instead of `Cow<str>`) for document storage.
- **Real type change:** yaml-rust2 stored floats as `String`, saphyr
  stores as `OrderedFloat<f64>`. Code using `r.clone()` becomes
  `f.to_string()`.
- **Loader API:** `YamlLoader::load_from_str` does not exist. Use
  `YamlOwned::load_from_str(text)` via `LoadableYamlNode` trait.
- **ScanError/Marker:** APIs are identical -- no changes needed.

All 5 modules with pattern matches (hover.rs, completion.rs,
symbols.rs, validators.rs, document_store.rs) require substantive
rewriting of every `Yaml::` variant match. Test helpers in hover.rs,
symbols.rs, completion.rs, and validators.rs (14 calls) also need
updating.

### LSP types for selection ranges

Available in `tower-lsp` v0.20 / `lsp-types` 0.94:
- `SelectionRange { range: Range, parent: Option<Box<SelectionRange>> }`
- `SelectionRangeParams { text_document, positions, ... }`
- `SelectionRangeProviderCapability::Simple(bool)`
- `LanguageServer` trait has `fn selection_range(&self, params: SelectionRangeParams) -> Result<Option<Vec<SelectionRange>>>`

### TypeScript reference

`yaml-language-server/src/languageservice/services/yamlSelectionRanges.ts`
walks the AST with `node.visit()` using node offsets/lengths to build
selection ranges from innermost to outermost, then reverses. With
saphyr's `MarkedYaml` spans, the Rust implementation can follow a
similar AST-walking approach.

### Build constraints

Rust edition 2024, clippy pedantic + nursery with zero warnings
enforced. 260 tests currently passing.

## Steps

- [x] Replace `yaml-rust2` dependency with `saphyr` + `saphyr-parser` in `Cargo.toml`
- [x] Update `parser.rs` to use saphyr's API
- [x] Update `document_store.rs` to use saphyr's `Yaml` type
- [x] Update `hover.rs` to use saphyr's `Yaml` type
- [x] Update `completion.rs` to use saphyr's `Yaml` type
- [x] Update `symbols.rs` to use saphyr's `Yaml` type
- [x] Update `validators.rs` to use saphyr's `Yaml` type
- [x] Update integration test `lsp_lifecycle.rs` if needed
- [x] Verify all 260 existing tests pass with saphyr
- [x] Add `MarkedYaml` parsing to `parser.rs` (marked loader)
- [x] Add `MarkedYaml` storage and accessor to `document_store.rs`
- [x] Create `rlsp-yaml/src/selection.rs` with AST-based pure function
- [x] Write unit tests for the selection ranges module (17 tests in selection.rs)
- [x] Register `pub mod selection` in `rlsp-yaml/src/lib.rs`
- [x] Add `selection_range_provider` capability in `server.rs`
- [x] Implement `selection_range` handler in `server.rs` (passing marked AST)
- [x] Add capability test in `server.rs` tests

## Tasks

### Task 1: Migrate from yaml-rust2 to saphyr

Replace the `yaml-rust2` dependency with `saphyr` and rewrite all
modules that reference `yaml_rust2` to use saphyr's `YamlOwned` API.
This is a substantive rewrite -- variant names and types have changed.
See "Migration scope" in Context for the full variant mapping.

**What to change:**

1. **`rlsp-yaml/Cargo.toml`:** Remove `yaml-rust2 = "0.9"`, add
   `saphyr` (latest version). `saphyr` re-exports `ScanError` and
   `Marker` -- no separate `saphyr-parser` dependency needed.

2. **`parser.rs`:** Replace `use yaml_rust2::{Yaml, YamlLoader}` with
   `use saphyr::{YamlOwned, LoadableYamlNode, ScanError}`. Change
   `YamlLoader::load_from_str(text)` to `YamlOwned::load_from_str(text)`.
   Change `ParseResult.documents` type from `Vec<Yaml>` to
   `Vec<YamlOwned>`. `ScanError`/`Marker` APIs are identical.

3. **`document_store.rs`:** Change stored type from `Vec<Yaml>` to
   `Vec<YamlOwned>`. Update all type signatures and accessors.

4. **`hover.rs`:** Rewrite all pattern matches: `Yaml::Hash` →
   `YamlOwned::Mapping`, `Yaml::Array` → `YamlOwned::Sequence`,
   `Yaml::String(s)` → `YamlOwned::Value(Scalar::String(s))`, etc.
   Change `Yaml::String(key.clone())` key construction to
   `YamlOwned::Value(Scalar::String(key.clone()))`. Update test
   helper `parse_docs` to use `YamlOwned::load_from_str`.

5. **`completion.rs`:** Update type references from `Yaml` to
   `YamlOwned`. Update test helper `parse_docs` to use
   `YamlOwned::load_from_str`.

6. **`symbols.rs`:** Rewrite all pattern matches in `yaml_to_symbols`,
   `make_symbol`, `make_sequence_children`, `yaml_symbol_kind`,
   `yaml_key_to_string`. `Yaml::Real(r).clone()` becomes
   `Scalar::FloatingPoint(f) => f.to_string()`. Update test helper.

7. **`validators.rs`:** Rewrite `check_yaml_ordering` matches.
   `Yaml::Real(r) => Some(r.clone())` becomes
   `Scalar::FloatingPoint(f) => Some(f.to_string())`. Update all
   14 test helper calls from `yaml_rust2::YamlLoader::load_from_str`
   to `YamlOwned::load_from_str` with trait import.

8. **`tests/lsp_lifecycle.rs`:** Check if it references yaml-rust2
   directly. Update if needed.

**Acceptance criteria:**
- All 260 existing tests pass
- `cargo build` succeeds
- `cargo clippy` reports zero warnings
- No yaml-rust2 references remain in source code
- All pattern matches rewritten from yaml-rust2 `Yaml::` variants
  to saphyr `YamlOwned::` variants (substantive rewrite, not just
  import changes)

**Key files:**
- `/workspace/rlsp-yaml/Cargo.toml`
- `/workspace/rlsp-yaml/src/parser.rs`
- `/workspace/rlsp-yaml/src/document_store.rs`
- `/workspace/rlsp-yaml/src/hover.rs`
- `/workspace/rlsp-yaml/src/completion.rs`
- `/workspace/rlsp-yaml/src/symbols.rs`
- `/workspace/rlsp-yaml/src/validators.rs`
- `/workspace/rlsp-yaml/tests/lsp_lifecycle.rs`

---

### Task 2: Implement AST-based selection ranges using MarkedYamlOwned spans

Create the `selection.rs` module with a pure function that uses
saphyr's `MarkedYamlOwned` AST nodes and their `Span` info for
accurate cursor-to-node mapping. Integrate into the server.

**NOTE:** `MarkedYaml<'input>` has a lifetime parameter (like `Yaml`).
Must use `MarkedYamlOwned` for storage in `DocumentStore`.

**What to build:**

1. **Extend parsing to produce `MarkedYamlOwned`:**
   - Update `parser.rs` to also parse with saphyr's marked loader
     that produces `Vec<MarkedYamlOwned>` where each node carries a
     `Span { start: Marker, end: Marker }` with exact byte offset,
     line, and column
   - Update `document_store.rs` to store `Vec<MarkedYamlOwned>`
     alongside the existing `Vec<YamlOwned>` and raw text, and
     expose it via a `get_marked_yaml()` accessor (following the
     `get_yaml()` pattern)

2. **New file `rlsp-yaml/src/selection.rs`** with:
   - Public function with signature like
     `selection_ranges(text: &str, documents: Option<&Vec<MarkedYamlOwned>>, positions: &[Position]) -> Vec<SelectionRange>`
     -- matching the pattern of `hover_at(text, documents, position)`
     and `document_symbols(text, documents)` which receive parsed AST
     alongside raw text
   - For each position, **walk the `MarkedYamlOwned` AST** to find
     the innermost node whose `Span` contains the cursor position
     (using marker line/col for comparison)
   - Build the parent chain by following AST parent relationships --
     each ancestor node's `Span` becomes the next `SelectionRange`
     with `parent` pointing to its enclosing node's range
   - Each level becomes a `SelectionRange` with `parent` pointing
     to the enclosing level

3. **AST-based selection hierarchy:**
   - On a scalar value: the node's span -> parent property node span
     -> parent mapping/sequence span -> ... -> document root span
   - On a key: key node span -> property span -> parent mapping span
     -> ... -> document root
   - On a sequence item: item node span -> parent array node span
     -> ... -> document root
   - Multi-document: each YAML document in a multi-doc file is a
     separate root; the outermost range is scoped to that document's
     span, not the entire file

4. **Register in `lib.rs`:** Add `pub mod selection;` alphabetically.

5. **Server integration in `server.rs`:**
   - Add `selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true))`
   - Implement `async fn selection_range` -- fetch text and marked
     YAML from the document store, call the pure function, return
     the result (following the `hover`/`document_symbol` handler
     pattern which passes both text and AST)

6. **Tests:**
   - Unit tests for: simple key-value, nested mappings, sequences,
     sequence of mappings, multi-document, edge cases (empty doc,
     cursor beyond document, cursor on comment/separator, AST
     unavailable due to parse error)
   - Capability test in `server.rs`

**Acceptance criteria:**
- Cursor in a value expands to: value -> key-value pair -> parent mapping -> document
- Cursor in a sequence item expands to: item -> sequence -> parent
- Handles nested structures correctly
- Works with multi-document YAML files
- Range boundaries come from `MarkedYaml` span data, not indentation heuristics
- Graceful fallback when AST is unavailable (parse error): return empty / degenerate ranges
- All tests pass (260 existing + new)
- Zero clippy warnings

**Key files to reference:**
- `/workspace/rlsp-yaml/src/hover.rs` -- pattern for receiving `(text, documents, position)` and walking AST
- `/workspace/rlsp-yaml/src/symbols.rs` -- pattern for converting AST nodes to LSP types with ranges
- `/workspace/rlsp-yaml/src/document_store.rs` -- where to add `MarkedYaml` storage and accessor
- `/workspace/rlsp-yaml/src/parser.rs` -- where to add marked parsing
- `/workspace/rlsp-yaml/src/server.rs` -- capability and handler pattern
- `/workspace/yaml-language-server/src/languageservice/services/yamlSelectionRanges.ts` -- TypeScript reference (AST-walking approach)
- `/workspace/yaml-language-server/test/yamlSelectionRanges.test.ts` -- reference test cases

## Decisions

- **Two task slices, sequential:** Task 1 (migration) must complete
  and be committed before Task 2 (selection ranges) begins. Task 2
  depends on saphyr being in place. This ordering prevents merge
  conflicts and ensures Task 2 builds on a clean saphyr foundation.

- **Migration first, AST spans in Task 2:** Task 1 rewrites all
  modules to use `YamlOwned` -- no span-info adoption yet. Task 2
  then adds `MarkedYamlOwned` parsing and storage, and uses the
  span data for AST-based selection ranges -- the first module to
  leverage saphyr's position-aware nodes.

- **saphyr chosen as yaml-rust2 successor:** saphyr is the actively
  maintained fork with span support. The migration cost is higher
  than initially expected (substantive variant rewrite, not just
  import changes) but the benefit (position-aware AST) is
  significant for selection ranges and future features.
