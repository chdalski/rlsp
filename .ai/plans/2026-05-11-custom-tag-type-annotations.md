**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-11

## Goal

Add type annotation support to the `customTags` setting so users can declare the expected YAML structure for each custom tag (e.g., `"!include scalar"`, `"!Ref mapping"`, `"!Sub sequence"`). When a type annotation is present and the tagged node's structure doesn't match, emit a diagnostic. Tags without a type annotation continue to work as today — suppressing the `unknownTag` warning without structure validation.

## Context

- The `customTags` setting already flows end-to-end: VS Code `package.json` → `config.ts` → LSP `initializationOptions` → `Settings.custom_tags: Vec<String>` → `validate_custom_tags()` in `rlsp-yaml/src/validation/validators.rs:251-267`.
- Modeline support exists: `# yaml-language-server: $tags=!include,!ref` via `extract_custom_tags()` in `rlsp-yaml/src/schema/association.rs:45-57`. Tags from both sources are merged into a `HashSet<String>` at `server.rs:503-504`.
- Current validation (`collect_tag_diagnostics` at `validators.rs:270-319`) walks the AST, checks each node's `tag` field against the allowed set, and emits `unknownTag` warnings for unrecognized tags. It does not inspect the node's structure.
- The AST node types map directly to the annotation types: `Node::Scalar` → `scalar`, `Node::Mapping` → `mapping`, `Node::Sequence` → `sequence`.
- RedHat's yaml-language-server uses the same `"!tag type"` format with space-separated suffix. Compatibility with this format is desirable.
- All call sites that pass `allowed_tags` to `validate_custom_tags` and must be updated: `server.rs` (~line 505), unit tests in `validators.rs`, `tests/corpus_invariants.rs` (~lines 163, 775, 1082), `benches/insight.rs` (~line 48), `benches/hot_path.rs` (~line 45), and `tests/parser_boundary_audit.rs` (~line 801, signature string assertion). This list is exhaustive — only `server.rs` and the `validators.rs` unit tests pass non-empty tag sets; all other call sites use empty sets, so the new `tagTypeMismatch` code path is unreachable in those callers.
- **LSP spec reference:** [Custom tags are not part of the LSP spec — this is a yaml-language-server convention]

## Steps

- [x] Clarify requirements with user
- [ ] Parse type annotations from custom tag strings
- [ ] Validate tagged node structure against declared type
- [ ] Update modeline parsing for type annotations
- [ ] Update documentation and VS Code extension

## Tasks

### Task 1: Parse type annotations and validate tagged node structure

Introduce a `CustomTag` struct that holds the tag name and an optional expected node type (`scalar`, `mapping`, `sequence`). Parse each entry in `custom_tags: Vec<String>` by splitting on the last space — `"!include scalar"` → name `"!include"`, expected type `Scalar`; `"!include"` → name `"!include"`, expected type `None` (any structure allowed).

Modify `validate_custom_tags` to accept parsed `CustomTag` entries instead of a plain `HashSet<String>`. When a tag matches by name but the node type doesn't match the declared type, emit a new diagnostic code `tagTypeMismatch` with a message like `"Tag !Ref expects a mapping, but got a scalar"`. The existing `unknownTag` diagnostic continues to fire for tags not in the list at all.

Modify the merge point in `server.rs:503-504` to parse both workspace settings and modeline tags into `CustomTag` structs before merging. Modeline tags use the same `"!tag type"` format: `# yaml-language-server: $tags=!include scalar,!Ref mapping`.

- [ ] Add `CustomTag` struct with `name: String` and `expected_type: Option<TagNodeType>` where `TagNodeType` is an enum `{ Scalar, Mapping, Sequence }`
- [ ] Add parsing function `parse_custom_tag(input: &str) -> CustomTag` that splits on trailing ` scalar`/` mapping`/` sequence`
- [ ] Parsing is case-insensitive for the type suffix (`Scalar`, `SCALAR`, `scalar` all work)
- [ ] Unknown type suffixes (e.g., `"!foo unknown"`) treat the entire string as the tag name (backward compatible — a tag could legitimately contain spaces in theory, though rare)
- [ ] Update `validate_custom_tags` signature to accept `&[CustomTag]` instead of `&HashSet<String>`
- [ ] Tag name lookup uses the `CustomTag.name` field for matching (same `unknownTag` logic as before)
- [ ] When a tag name matches but `expected_type` is `Some` and the node type doesn't match, emit `tagTypeMismatch` diagnostic (warning severity)
- [ ] When a tag name matches and `expected_type` is `None`, no structure check (existing behavior preserved)
- [ ] Update `server.rs` merge point to parse `Vec<String>` from settings and modeline into `Vec<CustomTag>`, deduplicate by tag name
- [ ] Update `extract_custom_tags` in `association.rs` to preserve the full `"!tag type"` string (it already does — just verify)
- [ ] Unit tests: parse `"!include scalar"`, `"!include"`, `"!Ref mapping"`, `"!Sub sequence"`, case-insensitive suffix, unknown suffix treated as tag name
- [ ] Unit tests: tag type mismatch produces `tagTypeMismatch` diagnostic; matching type produces no diagnostic; no type annotation produces no diagnostic
- [ ] Unit tests: `unknownTag` still fires for tags not in the list
- [ ] Integration test: configure `customTags` with type annotations via settings, open document with matching and mismatching tagged nodes, verify both `unknownTag` and `tagTypeMismatch` diagnostics
- [ ] Update the three `validate_custom_tags` call sites in `rlsp-yaml/tests/corpus_invariants.rs` (~lines 163, 775, 1082) to pass `&[]` (empty slice satisfies the "no custom tags" case); remove the now-unused `let allowed_tags: HashSet<String> = HashSet::new()` bindings that fed them
- [ ] Update `validate_custom_tags` call sites in `rlsp-yaml/benches/insight.rs` (~line 48) and `rlsp-yaml/benches/hot_path.rs` (~line 45) to use the new `&[CustomTag]` signature
- [ ] Update the `generic_validate_fn_detected` test in `rlsp-yaml/tests/parser_boundary_audit.rs` (~line 801) to reflect the new `validate_custom_tags` signature
- [ ] All existing tests and benchmarks pass — tests that construct `HashSet<String>` for `validate_custom_tags` are updated to use the new `CustomTag` type
- [ ] Update `docs/configuration.md`: document the type annotation syntax in the `customTags` section with examples
- [ ] Update `docs/configuration.md`: update the `### Custom tags modeline` section to show the type annotation form (e.g., `$tags=!include scalar,!Ref mapping`)
- [ ] Update `docs/configuration.md`: add `tagTypeMismatch` to the diagnostic codes table
- [ ] Update VS Code `package.json`: enhance the `customTags` description to mention type annotations
- [ ] Update `rlsp-yaml/README.md`: amend one quickstart `customTags` example to show the type annotation form
- [ ] Update `rlsp-yaml/integrations/vscode/README.md`: update the `customTags` settings table description to mention type annotations
- [ ] Update `feature-log.md` with an entry describing the custom tag type annotation feature

## Non-Goals

- Configurable label-key list for customTags (YAGNI)
- Schema-level integration (e.g., inferring tag types from JSON Schema `x-tags` extensions)
- Nested type annotations (e.g., `"!transform mapping(scalar, sequence)"`)
- Auto-completion of custom tag names

## Decisions

- **Single task** — the change is cohesive: parsing, validation, and documentation are tightly coupled and don't benefit from splitting into separate commits.
- **`"!tag type"` format matches RedHat convention** — users migrating from RedHat's yaml-language-server can reuse their `customTags` configuration without changes.
- **Case-insensitive type suffix** — `scalar`, `Scalar`, `SCALAR` all work. Prevents user frustration from capitalization mismatches.
- **Unknown suffix = tag name** — `"!foo unknown"` is treated as a tag named `"!foo unknown"`, not a parsing error. This preserves backward compatibility for edge cases where tag names might contain spaces.
- **`tagTypeMismatch` is a separate diagnostic code** — distinct from `unknownTag` so users can suppress them independently via `# rlsp-yaml-disable tagTypeMismatch`.
- **Warning severity for `tagTypeMismatch`** — consistent with `unknownTag`. Not an error because custom tag semantics are application-defined, not YAML-spec-defined.
- **Deduplication by tag name** — if both workspace settings and modeline declare the same tag with different types, the modeline wins (closer to the document, more specific). This matches the existing modeline-overrides-workspace pattern.
- **Completed I11 plan references old signature** — the committed plan `2026-05-08-corpus-invariant-validator-stability-i11.md` documents the `validate_custom_tags(&docs, &allowed_tags)` call shape with `HashSet<String>`. This is a historical record; the plan file is not updated since it reflects the state at the time of that work.
