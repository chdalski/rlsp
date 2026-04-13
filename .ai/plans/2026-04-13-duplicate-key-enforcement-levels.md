**Repository:** root
**Status:** InProgress
**Created:** 2026-04-13

# Duplicate key enforcement levels

## Goal

Add configurable severity for duplicate key diagnostics (`"off"` / `"warning"` / `"error"`, default `"error"` to match current behavior) and a `formatRemoveDuplicateKeys` toggle that auto-removes duplicate keys during "Format Document" (keeping the last occurrence per the YAML spec). This follows the same pattern established by the flow style enforcement plan.

## Context

### Current behavior

`validate_duplicate_keys()` in `validators.rs` always emits ERROR-level diagnostics with code `duplicateKey`. There is no setting to control severity or disable detection. There is no code action to remove duplicate keys — only the diagnostic squiggle.

The duplicate key validator works on the AST (`Vec<Document<Span>>`), walking each `Node::Mapping` and tracking seen keys via a `HashSet`. It flags the second (and subsequent) occurrences.

### Auto-removal semantics

YAML spec says the last value wins for duplicate keys. The auto-remove feature keeps the **last** occurrence of each key and removes earlier duplicates. This is data-destructive (unlike flow→block style conversion), so it must be explicitly opt-in.

The removal happens as a pre-pass on the AST before the formatter renders it. The formatter already receives `Vec<Document<Span>>` from parsing — we add a filtering step that deduplicates mapping entries in-place.

### Pattern from flow style plan

The flow style plan (completed 2026-04-13) established the settings pattern:
- Severity setting: `flowStyle` → `"off"` / `"warning"` / `"error"` (controls diagnostic severity)
- Auto-fix setting: `formatEnforceBlockStyle` → `bool` (controls formatter behavior)
- Settings wired in `parse_and_publish()` and `formatting()`
- VS Code extension exposes both in `package.json` and `config.ts`

The duplicate key feature follows this exactly.

### Specifications and reference implementations

- [YAML 1.2 Specification](https://yaml.org/spec/1.2.2/) — §3.2.1.3: "The content of a mapping node is an unordered set of key/value node pairs, with the restriction that each of the keys is unique."
- [YAML 1.2 Specification](https://yaml.org/spec/1.2.2/) — §3.2.1.3: For duplicate keys, "the last value is used."

### Key files

| File | Role |
|------|------|
| `rlsp-yaml/src/validation/validators.rs` | `validate_duplicate_keys()` — add severity control |
| `rlsp-yaml/src/server.rs` | Settings struct, `parse_and_publish()`, `formatting()` |
| `rlsp-yaml/src/editing/formatter.rs` | Add duplicate-key removal pre-pass |
| `rlsp-yaml/integrations/vscode/package.json` | VS Code extension settings |
| `rlsp-yaml/integrations/vscode/src/config.ts` | TypeScript settings interface |
| `rlsp-yaml/docs/configuration.md` | User-facing settings documentation |

## Steps

- [x] Add `duplicateKeys` severity setting to the server (off/warning/error, default error)
- [x] Wire severity into `parse_and_publish()` — skip when off, patch severity level
- [ ] Add `formatRemoveDuplicateKeys` setting to the server (bool, default false)
- [ ] Implement duplicate-key removal pre-pass in the formatter
- [ ] Wire `formatRemoveDuplicateKeys` into `formatting()` and `range_formatting()`
- [ ] Expose both settings in VS Code extension
- [ ] Update `docs/configuration.md`
- [ ] Add tests at each layer

## Tasks

### Task 1: Severity setting and server wiring ✅ `efb3ab4`

Add the `duplicateKeys` severity setting and wire it into the diagnostics pipeline. This is pure settings plumbing — identical pattern to `flowStyle`.

- [x] Add `duplicate_keys: Option<String>` to `Settings` struct
- [x] Add getter method `get_duplicate_keys()` returning the severity string
- [x] Update `parse_and_publish()`: respect severity — skip `validate_duplicate_keys()` when `"off"`, keep as ERROR when `"error"` or absent (default), patch to WARNING when `"warning"`
- [x] Add integration tests: verify diagnostics respect all three severity levels
- [x] `cargo test` passes, `cargo clippy --all-targets` clean

### Task 2: Auto-remove duplicate keys in the formatter

Add a `formatRemoveDuplicateKeys` setting and implement the duplicate-key removal pre-pass.

The pre-pass operates on the parsed `Vec<Document<Span>>` before it reaches the formatter's `node_to_doc()`. For each `Node::Mapping`, iterate entries in reverse, track seen keys, and remove earlier entries that share a key with a later entry. This preserves the last occurrence (YAML spec behavior).

Implementation approach: add a `dedup_keys(docs: &mut Vec<Document<Span>>)` function in `formatter.rs` (or a helper module) that recursively walks the AST and deduplicates in-place. Call it in `format_yaml()` when `format_remove_duplicate_keys` is `true`, before the `node_to_doc()` conversion.

- [ ] Add `format_remove_duplicate_keys: bool` to `YamlFormatOptions` (default `false`)
- [ ] Add `format_remove_duplicate_keys: Option<bool>` to `Settings` struct with getter
- [ ] Implement `dedup_mapping_keys()` — walk AST, remove earlier duplicates, keep last
- [ ] Call dedup pre-pass in `format_yaml()` when enabled
- [ ] Wire setting into `formatting()` and `range_formatting()` in server.rs
- [ ] Unit tests: dedup removes correct entries, keeps last, handles nested mappings, no-op when no duplicates, no-op when disabled
- [ ] Integration test: format a document with duplicate keys and verify removal
- [ ] `cargo test` passes, `cargo clippy --all-targets` clean

### Task 3: VS Code extension and documentation

Expose the new settings in the VS Code extension and update documentation.

- [ ] Add `rlsp-yaml.duplicateKeys` (enum: `"off"`, `"warning"`, `"error"`, default `"error"`) to `package.json`
- [ ] Add `rlsp-yaml.formatRemoveDuplicateKeys` (boolean, default `false`) to `package.json`
- [ ] Update `ServerSettings` interface and `getConfig()` in `config.ts`
- [ ] Update `docs/configuration.md` with both new settings
- [ ] `pnpm run lint` and `pnpm run build` pass in the VS Code extension directory

## Decisions

- **Default severity is `"error"`** — matches current behavior. Duplicate keys are a spec violation, not a style preference. Changing the default would silently weaken validation for existing users.
- **Keep last occurrence** — YAML spec says last value wins. This ensures the auto-removed document has identical runtime semantics to the original.
- **Pre-pass on AST, not in-formatter** — the dedup logic is conceptually separate from formatting. Running it as a pre-pass keeps the formatter focused on rendering and makes dedup testable independently.
- **In-place mutation of AST** — the formatter already owns the parsed `Vec<Document<Span>>` (it's parsed fresh in `format_yaml()`). Mutating entries in-place avoids cloning the entire AST.
