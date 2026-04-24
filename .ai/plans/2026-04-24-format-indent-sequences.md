**Repository:** root
**Status:** Completed (2026-04-24)
**Created:** 2026-04-24

## Goal

Add a `formatIndentSequences` boolean setting (default `true`) to rlsp-yaml's formatter. When `true`, block sequences that are values of mapping keys are indented one level under the key (current behavior): `key:\n  - item`. When `false`, block sequences start at the same indent level as the key (indentless/compact style): `key:\n- item`. The formatter always normalizes to the chosen style — no "preserve" mode. This is one of the most common YAML style preferences and is supported by RedHat yaml-language-server.

## Context

- **YAML 1.2 permits both styles.** Both indented (`key:\n  - item`) and indentless (`key:\n- item`) block sequences are valid YAML. The spec allows either form; the choice is purely stylistic.
- **Current behavior:** The formatter hardcodes indented style via `indent()` wrappers in `formatter.rs` when a mapping value is a block sequence.
- **Three `indent()` call sites** in `formatter.rs` wrap sequence content:
  - Lines 1342-1346: explicit-key mapping entry with block sequence value (`? key:\n  - item`)
  - Lines 1451-1458: implicit-key mapping entry with block sequence value (`key:\n  - item`)
  - Lines 1614-1621: sequence item containing a nested block sequence (`-\n  - nested`)
  The first two are the `formatIndentSequences` targets (mapping-value sequences). The third is a sequence-in-sequence nesting that should always indent regardless of the setting — removing indentation there would produce ambiguous YAML where the nested sequence items merge visually with the parent sequence.
- **Settings sync locations** (from CLAUDE.md Settings Sync table):
  - `YamlFormatOptions` in `formatter.rs` — source of truth
  - VS Code `package.json` contributes.configuration + `config.ts` `ServerSettings` interface and `getConfig()`
  - `Settings` struct in `server.rs` — workspace settings deserialization
  - Server formatting handlers in `server.rs` (two locations: `text_formatting` ~line 1053, `range_formatting` ~line 1139)
  - Fixture files in `tests/fixtures/formatter/`
- **Existing settings pattern:** `format_enforce_block_style` and `format_remove_duplicate_keys` follow the same bool-option pattern through all four sync locations. The new setting follows the same pattern.
- **Specifications:** YAML 1.2.2 §8.2.1 (Block Sequences)

## Non-Goals

- **Nested sequence-in-sequence indentation** — the `indent()` at line 1614-1621 (sequence item containing a block sequence) is not affected. Removing indentation there produces visually ambiguous YAML.
- **Preserve mode** — no "keep the source style" option. The formatter normalizes to the chosen style, consistent with all other formatting options.
- **Interaction with `formatEnforceBlockStyle`** — when `formatEnforceBlockStyle` converts a flow sequence to block, the resulting block sequence respects `formatIndentSequences`. This is natural behavior, not a special interaction that needs its own logic.

## Steps

- [x] Add `format_indent_sequences` field to `YamlFormatOptions` and `Settings`
- [x] Wire the setting through server formatting handlers
- [x] Add VS Code extension configuration and config sync
- [x] Implement conditional indentation in formatter logic
- [x] Add fixture tests for both modes and setting interactions
- [x] Update documentation (`configuration.md`, `feature-log.md`)
- [x] Update follow-up queue

## Tasks

### Task 1: Add setting to Rust structs, server wiring, and formatter logic — commit 575080e

Add the `format_indent_sequences` field through the Rust settings pipeline and implement the conditional indentation.

- [x] Add `pub format_indent_sequences: bool` to `YamlFormatOptions` in `formatter.rs` with doc comment: "Indent block sequences that are values of mapping keys. Default: true."
- [x] Add `format_indent_sequences: true` to the `Default` impl
- [x] Add `pub format_indent_sequences: Option<bool>` to `Settings` in `server.rs` with doc comment
- [x] Wire the setting in both formatting handlers in `server.rs` (~lines 1053 and 1139) using the same `.and_then().unwrap_or(true)` pattern as other bool settings
- [x] In `formatter.rs`, modify the explicit-key block-sequence branch (lines 1340-1346): when `options.format_indent_sequences` is `false`, emit `hard_line()` + `sequence_to_doc()` without wrapping in `indent()`
- [x] In `formatter.rs`, modify the implicit-key block-sequence branch (lines 1451-1458): same conditional — skip the `indent()` wrapper when `format_indent_sequences` is `false`
- [x] Do NOT modify the sequence-in-sequence branch (lines 1614-1621) — nested sequences always indent
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` all pass with zero warnings/failures

### Task 2: Add VS Code extension settings and TypeScript config sync — commit 7bb551b

Wire the new setting through the VS Code extension so it appears in the settings UI and is sent to the LSP server.

- [x] Add `formatIndentSequences` to the `ServerSettings` interface in `config.ts`
- [x] Add `formatIndentSequences: cfg.get<boolean>('formatIndentSequences', true)` to `getConfig()` in `config.ts`
- [x] Add `rlsp-yaml.formatIndentSequences` to `package.json` `contributes.configuration.properties` with type `boolean`, default `true`, and description: "Indent block sequences under mapping keys. When true (default), sequences are indented: `key:\\n  - item`. When false, sequences start at the key's indent level: `key:\\n- item`."
- [x] `pnpm run build`, `pnpm run lint`, `pnpm run test` all pass in `rlsp-yaml/integrations/vscode/`

### Task 3: Add fixture tests for indentless mode and setting interactions — commit 688287f

Add formatter fixtures covering the new setting in isolation and in combination with interacting settings.

- [x] Add fixture `structure-indent-sequences-false.md` with `format_indent_sequences: false` — test basic mapping-with-sequence-value normalization to indentless style. Input: indented sequence under a mapping key; expected: indentless output
- [x] Add fixture `structure-indent-sequences-true.md` with `format_indent_sequences: true` (explicit non-default to document the behavior) — test normalization to indented style. Input: indentless sequence under a mapping key; expected: indented output
- [x] Add fixture `structure-indent-sequences-false-nested.md` with `format_indent_sequences: false` — test that nested mappings and sequences inside sequence items retain correct indentation even when the top-level sequence is indentless
- [x] Add fixture `interact-indent-sequences-enforce-block-style.md` with `format_indent_sequences: false` + `format_enforce_block_style: true` — test that flow sequences converted to block use indentless style
- [x] Add fixture parser support: add `"format_indent_sequences"` match arm in `formatter_fixtures.rs` and add the field to `frontmatter_parses_all_default_settings` assertion
- [x] All fixture tests pass via `cargo test`

### Task 4: Update documentation and follow-up queue — commit ab6ad0f

Document the new setting in configuration.md, add a feature-log entry, and remove the follow-up queue item.

- [x] Add `### formatIndentSequences` section to `rlsp-yaml/docs/configuration.md` in the formatting settings area (after `formatEnforceBlockStyle`), following the same pattern: Type, Default, description with both styles shown, JSON example
- [x] Add entry to `rlsp-yaml/docs/feature-log.md`: "`formatIndentSequences` option" with description of both modes and the default
- [x] Remove the `formatIndentSequences` bullet from `.ai/memory/project_followup_plans.md`
- [x] The `configuration.md` section shows both `true` and `false` output forms with correct indentation, matches the default stated in `YamlFormatOptions`, and uses the same structure (Type / Default / description / JSON example) as the `formatEnforceBlockStyle` section
- [x] The `feature-log.md` entry describes both indented (`true`) and indentless (`false`) modes and states the default

## Decisions

- **Default `true`** — matches current behavior (indented sequences). Existing users see no change unless they opt into indentless style.
- **Boolean, not enum** — two modes (indented/indentless) map naturally to a boolean. No "preserve" mode — the follow-up queue explicitly says "Always normalize — no preserve mode."
- **Naming: `formatIndentSequences`** — follows the `format*` prefix convention for formatter settings (`formatEnforceBlockStyle`, `formatRemoveDuplicateKeys`). The name matches RedHat yaml-language-server's equivalent setting.
- **Sequence-in-sequence excluded** — the `indent()` at lines 1614-1621 (block sequence nested inside a sequence item) always applies regardless of the setting. Removing indentation there would produce `- \n- nested` which is visually indistinguishable from two sibling items in the parent sequence.
- **Four tasks** — Task 1 (Rust implementation) must land first since Task 2 (VS Code) depends on the setting existing in the server. Task 3 (fixtures) depends on Task 1. Task 4 (docs) depends on all prior tasks.
