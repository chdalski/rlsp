**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-11

## Goal

Enhance the existing `textDocument/documentSymbol` implementation to provide a richer outline experience in VS Code: show scalar values as detail text, use label-key heuristics to name sequence items meaningfully (e.g., `nginx` instead of `[0]`), support non-mapping root documents, and wrap multi-document files in per-document symbols.

## Context

- The document symbol handler already exists and is wired end-to-end (`rlsp-yaml/src/analysis/symbols.rs:16-171`, server handler at `rlsp-yaml/src/server.rs:1319-1347`).
- The capability is registered (`document_symbol_provider: Some(OneOf::Left(true))`).
- 24+ unit tests and 6+ integration tests already cover the current behavior.
- Current gaps relative to user expectations:
  1. `detail` field is always `None` — VS Code shows no value preview in the outline.
  2. Sequence items are always named `[0]`, `[1]` — no label-key heuristic to show `name`/`id` values.
  3. Non-mapping root documents (sequence root, scalar root) return no symbols.
  4. Multi-document files flat-merge all symbols — no per-document grouping.
- The AST provides all information needed: `Node::Scalar` has `value`, `Node::Mapping` entries have key-value pairs, `Document` has `explicit_start` and version info, `LineIndex` for span conversion.
- **LSP spec reference:** [textDocument/documentSymbol](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_documentSymbol)
- The `document_symbols` doc comment at `symbols.rs:11-14` states non-mapping roots return empty — Task 2 changes this behavior and must update the comment.
- The VS Code extension (`rlsp-yaml/integrations/vscode/`) has no tests or fixtures that assert the `detail` field of document symbol responses — no extension-side changes needed.

## Steps

- [x] Clarify requirements with user
- [x] Add detail text for scalar values
- [x] Add label-key heuristic for sequence items
- [ ] Support non-mapping root documents
- [ ] Wrap multi-document files in document-level symbols

## Tasks

### Task 1: Add detail text for scalar values and label-key heuristic for sequence items ✅ `adfc1e0`

Enhance `make_symbol` to populate the `detail` field with the scalar value (truncated to ~60 chars for readability) when the value node is a `Scalar`. For `Mapping` values, show the entry count (e.g., `"3 keys"`). For `Sequence` values, show the item count (e.g., `"2 items"`).

Enhance `make_sequence_children` to detect a "label key" in mapping-typed sequence items. When a sequence item is a `Mapping` and its first entry's key is a scalar matching one of `["name", "id", "key"]`, use that entry's scalar value as the symbol name instead of `[i]`. Fall back to `[i]` when no label key matches or the value is not a scalar. This is a fixed priority list, not configurable — YAGNI.

- [x] `make_symbol` populates `detail` with truncated scalar value for `Scalar` nodes
- [x] `make_symbol` populates `detail` with `"N keys"` for `Mapping` values and `"N items"` for `Sequence` values
- [x] `make_sequence_children` uses label-key heuristic: checks first entry key of mapping items against `["name", "id", "key"]`, uses matching value as symbol name
- [x] `make_sequence_children` falls back to `[i]` when no label key matches or item is not a mapping
- [x] When label-key is used, `detail` on the sequence item shows the original index (e.g., `"[0]"`) so the position is not lost; when `[i]` is used as the name (fallback), `detail` is `None` (position is already visible in the name)
- [x] Unit test: scalar value ≤60 chars appears verbatim in `detail`
- [x] Unit test: scalar value >60 chars is truncated with `…` suffix in `detail`
- [x] Unit test: mapping value shows `"N keys"` in `detail`; sequence value shows `"N items"` in `detail`
- [x] Unit test: sequence item whose first key is `name` uses that key's scalar value as symbol name, not `[i]`
- [x] Unit test: sequence item whose first key does not match `["name", "id", "key"]` falls back to `[i]`
- [x] Unit test: sequence item whose first key is `name` but value is not a scalar falls back to `[i]`
- [x] Integration test: send `textDocument/documentSymbol` for a Kubernetes-style YAML with `containers: [{name: nginx, ...}]` and verify `name` field is used as label and detail fields are populated
- [x] All existing unit tests pass; tests that previously asserted `detail: None` are updated to assert the new expected value — no test may be deleted or disabled
- [x] Tests that previously asserted `[0]`/`[1]` names on YAML whose first key matches the heuristic list (specifically `sequence_of_mappings_indexed_children_with_grandchildren` which uses `users: [{name: Alice, ...}]`) are updated to assert the label-key-derived name

### Task 2: Support non-mapping root documents and multi-document wrappers

Enhance `yaml_to_symbols` to handle `Sequence` root nodes by producing indexed child symbols (reusing `make_sequence_children`). `Scalar` and `Alias` root nodes produce a single symbol with the value/alias name.

Enhance `document_symbols` to wrap each document's symbols in a top-level `DocumentSymbol` when the input has 2+ documents. The wrapper symbol uses `SymbolKind::NAMESPACE` with name `"Document N"` (1-indexed), range covering the full document span, and children containing the document's symbols. Single-document files remain unwrapped (no behavior change for the common case).

- [ ] `yaml_to_symbols` handles `Sequence` root by calling `make_sequence_children`
- [ ] `yaml_to_symbols` handles `Scalar` root by producing a single symbol with the scalar value as name
- [ ] `document_symbols` wraps each document in a `NAMESPACE` symbol when doc count >= 2
- [ ] Single-document files produce the same flat output as before (no wrapper)
- [ ] Document wrapper range spans from first to last byte of the document
- [ ] Unit tests: sequence root, scalar root, multi-doc with wrapper (2 docs), multi-doc with wrapper (3+ docs), single-doc without wrapper
- [ ] Integration test: multi-document YAML returns `Document 1`, `Document 2` wrapper symbols
- [ ] Update the `document_symbols` doc comment to reflect that `Sequence` and `Scalar` root nodes now produce symbols
- [ ] All existing unit tests pass; tests that previously asserted flat multi-doc structure are updated to assert the new wrapped structure — no test may be deleted or disabled
- [ ] Update `feature-log.md` with an entry describing all four document symbol enhancements (detail text, label-key, non-mapping root, multi-doc wrappers)
- [ ] Remove the `Document symbols` follow-up entry from `.ai/memory/project_followup_plans.md`

## Decisions

- **Label-key list is fixed, not configurable** — `["name", "id", "key"]` covers Kubernetes, Docker Compose, GitHub Actions, and most real-world YAML. Adding a setting adds configuration complexity for marginal benefit. Can be revisited if users request it.
- **Label-key checks only the first entry** — checking all entries would be O(n) per item and the "label" key is almost always first in real YAML. If the first entry's key doesn't match, fall back to `[i]`.
- **Multi-doc wrapper uses `NAMESPACE`** — `SymbolKind::FILE` is for the file itself. `NAMESPACE` is the closest semantic match for "a document within a stream."
- **Detail truncation at ~60 chars** — long scalar values (block scalars, URLs) would overflow the outline panel. Truncate with `…` suffix.
- **No wrapper for single-document files** — most YAML files are single-document. Adding a wrapper would add one level of nesting with no information value, cluttering the outline.
