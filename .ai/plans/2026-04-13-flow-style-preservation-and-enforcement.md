**Repository:** root
**Status:** InProgress
**Created:** 2026-04-13

# Flow style preservation and enforcement levels

## Goal

Fix the formatter to preserve flow-style collections (`[a, b]`, `{a: 1}`) by default instead of unconditionally converting them to block style. Add a configurable enforcement level for the `flowMap`/`flowSeq` diagnostics (`"off"`, `"warning"`, `"error"`) and a separate `formatEnforceBlockStyle` toggle that auto-converts flow collections to block style during "Format Document." This closes the gap with RedHat's yaml-language-server severity settings while fixing the current bug where formatting silently applies what should be an opt-in style preference.

## Context

### The bug

The formatter (`rlsp-yaml/src/editing/formatter.rs`) always emits block style for non-empty sequences and mappings — `sequence_to_doc()` (line 606) and `mapping_to_doc()` (line 544) have no conditional path for flow output. Meanwhile `validate_flow_style()` in `validators.rs` emits these as warnings with codes `flowSeq`/`flowMap`, and code actions in `code_actions.rs` offer "Quick Fix" conversions. The formatter bypasses this user-choice model entirely.

### Root cause

The parser emits `CollectionStyle::Flow` / `CollectionStyle::Block` on `Event::SequenceStart` and `Event::MappingStart` (defined in `rlsp-yaml-parser/src/event.rs`), but the loader (`rlsp-yaml-parser/src/loader.rs`) discards this information when building the AST — `Node::Mapping` and `Node::Sequence` in `node.rs` have no `style` field. The formatter has no way to know the original style.

### What needs to change

1. **Parser AST** — add a `style: CollectionStyle` field to `Node::Mapping` and `Node::Sequence`
2. **Loader** — thread the `CollectionStyle` from events into the AST nodes
3. **Formatter** — use `style` to decide between block and flow output; add flow-style rendering for sequences and mappings
4. **Settings** — add `flowStyle` severity setting (`"off"` / `"warning"` / `"error"`) and `formatEnforceBlockStyle` boolean
5. **Server** — wire new settings into diagnostics (severity control, skip when `"off"`) and formatter (override style when enforce is on)
6. **VS Code extension** — expose both new settings in `package.json` and `config.ts`
7. **Documentation** — update `docs/configuration.md`

### Specifications and reference implementations

- [YAML 1.2 Specification](https://yaml.org/spec/1.2.2/) — §7.4 (flow sequences), §7.5 (flow mappings)
- [RedHat yaml-language-server](https://github.com/redhat-developer/yaml-language-server) — reference for flow style severity setting

### Key files

| File | Role |
|------|------|
| `rlsp-yaml-parser/src/node.rs` | AST node types — add `style` field |
| `rlsp-yaml-parser/src/event.rs` | `CollectionStyle` enum (already exists) |
| `rlsp-yaml-parser/src/loader.rs` | Loader — thread style from events to nodes |
| `rlsp-yaml/src/editing/formatter.rs` | Formatter — branch on style, add flow rendering |
| `rlsp-yaml/src/validation/validators.rs` | `validate_flow_style()` — respect severity setting |
| `rlsp-yaml/src/server.rs` | Settings struct, `parse_and_publish()`, `formatting()` |
| `rlsp-yaml/src/editing/code_actions.rs` | Code actions — no change needed (already diagnostic-driven) |
| `rlsp-yaml/integrations/vscode/package.json` | VS Code extension settings |
| `rlsp-yaml/integrations/vscode/src/config.ts` | TypeScript settings interface |
| `rlsp-yaml/docs/configuration.md` | User-facing settings documentation |

## Steps

- [x] Add `style: CollectionStyle` to `Node::Mapping` and `Node::Sequence` in the parser AST
- [x] Thread `CollectionStyle` from events through the loader into AST nodes
- [x] Fix all compilation errors from the new field across the workspace
- [x] Add flow-style rendering to the formatter (sequences and mappings)
- [x] Make the formatter branch on `CollectionStyle` — preserve original style by default
- [x] Add `flowStyle` severity setting and `formatEnforceBlockStyle` setting to the server
- [x] Wire severity setting into `validate_flow_style()` (skip when `"off"`, set severity from setting)
- [x] Wire `formatEnforceBlockStyle` into the formatter to override style when enabled
- [x] Expose new settings in VS Code extension
- [x] Update `docs/configuration.md`
- [x] Add/update tests at each layer

## Tasks

### Task 1: Add `CollectionStyle` to parser AST and loader — `728d182`

Add a `style: CollectionStyle` field to `Node::Mapping` and `Node::Sequence` in `rlsp-yaml-parser/src/node.rs`. Update the loader (`loader.rs`) to capture the `style` from `Event::MappingStart` / `Event::SequenceStart` and pass it through to the constructed nodes. Fix all match arms and construction sites across the workspace that destructure or build these node variants — this includes the formatter, validators, code actions, and any test code that constructs nodes.

- [x] Add `style: CollectionStyle` field to `Node::Mapping` and `Node::Sequence` in `node.rs`
- [x] Update loader to capture style from events and populate the new field
- [x] Fix all compilation errors across the workspace (formatter, validators, code actions, tests)
- [x] Add unit tests verifying that flow-style and block-style collections carry the correct `CollectionStyle` through load → AST
- [x] `cargo test` passes, `cargo clippy --all-targets` clean

### Task 2: Flow-style rendering in the formatter — `20004bb`

Add flow-style output paths in the formatter for sequences and mappings. Make the formatter branch on the node's `CollectionStyle`: emit flow output (`[a, b, c]` / `{a: 1, b: 2}`) for `Flow`, emit block output (current behavior) for `Block`. The `bracket_spacing` option already exists in `YamlFormatOptions` — use it for flow mappings.

The key change: `sequence_to_doc()` and `mapping_to_doc()` currently always emit block style. They need a `style` parameter (or the formatter needs access to the style) to branch. For the flow path, use `rlsp-fmt`'s `group()` / `line()` / `indent()` primitives so the pretty-printer automatically decides between single-line and multi-line based on `print_width`:
- Flow sequences: `[item1, item2, item3]` when it fits; multi-line `[\n  item1,\n  item2,\n]` when it doesn't
- Flow mappings: `{key1: val1, key2: val2}` when it fits (with bracket_spacing: `{ key1: val1 }`); multi-line when it doesn't
- Empty collections remain `[]` / `{}` regardless of style
- Nested flow collections inside a flow parent stay flow
- Scalars inside flow collections use plain/quoted style as they do today

The `group(concat([text("["), indent(concat([line(), items...])), line(), text("]")]))` pattern from `rlsp-fmt`'s own doc examples is the exact fit — flat mode renders as `[a, b]`, break mode renders with newlines and indentation.

Add a `format_enforce_block_style: bool` field to `YamlFormatOptions`. When `true`, the formatter ignores the node's style and always emits block (current behavior). When `false` (default), the formatter respects the node's style.

- [x] Add `format_enforce_block_style` field to `YamlFormatOptions` (default `false`)
- [x] Implement flow sequence rendering (comma-separated items in `[...]`)
- [x] Implement flow mapping rendering (comma-separated key-value pairs in `{...}` / `{ ... }`)
- [x] Branch `sequence_to_doc()` and `mapping_to_doc()` on style (or add parallel flow functions)
- [x] When `format_enforce_block_style` is `true`, override style to block
- [x] Add unit tests: round-trip flow sequences, flow mappings, mixed flow/block, empty collections, nested flow, enforce-block-style override, multiline flow (exceeds print_width)
- [x] Integration test: format a document with flow collections, verify they are preserved
- [x] `cargo test` passes, `cargo clippy --all-targets` clean

### Task 3: Settings and server wiring — `73d38db`

Add two new settings to the server:
1. `flowStyle`: `"off"` | `"warning"` | `"error"` (default: `"warning"`) — controls the severity of `flowMap`/`flowSeq` diagnostics, or disables them entirely
2. `formatEnforceBlockStyle`: `bool` (default: `false`) — when `true`, the formatter converts all flow collections to block style on format

Wire these into:
- `parse_and_publish()` — respect severity from `flowStyle` setting; skip `validate_flow_style()` when `"off"`; set diagnostic severity to WARNING or ERROR based on the setting
- `formatting()` — pass `formatEnforceBlockStyle` through to `YamlFormatOptions`

- [x] Add `flow_style` and `format_enforce_block_style` fields to `Settings` struct
- [x] Add getter methods for both settings
- [x] Update `parse_and_publish()`: skip flow style validation when `"off"`, set severity from setting
- [x] Update `formatting()`: pass `format_enforce_block_style` to `YamlFormatOptions`
- [x] Add integration tests: verify diagnostics respect severity setting (off/warning/error), verify formatter respects enforce setting
- [x] `cargo test` passes, `cargo clippy --all-targets` clean

### Task 4: VS Code extension and documentation — `875e216`

Expose the new settings in the VS Code extension and update user-facing documentation.

- [x] Add `rlsp-yaml.flowStyle` (enum: `"off"`, `"warning"`, `"error"`, default `"warning"`) to `package.json`
- [x] Add `rlsp-yaml.formatEnforceBlockStyle` (boolean, default `false`) to `package.json`
- [x] Update `ServerSettings` interface and `getConfig()` in `config.ts`
- [x] Update `docs/configuration.md` with both new settings — descriptions, defaults, examples
- [x] `pnpm run lint` and `pnpm run build` pass in the VS Code extension directory

## Decisions

- **Preserve style by default** — the formatter must not change flow → block unless the user explicitly opts in via `formatEnforceBlockStyle: true`. This is the bug fix.
- **Separate severity from auto-fix** — `flowStyle` controls diagnostic severity; `formatEnforceBlockStyle` controls formatter behavior. They are independent. A user can have `flowStyle: "error"` (hard lint gate) without auto-conversion on format, or `formatEnforceBlockStyle: true` without any diagnostic at all (`flowStyle: "off"`).
- **`flowStyle` replaces the hardcoded WARNING** — the existing `validate_flow_style()` always emits WARNING. The new setting makes severity configurable: `"off"` suppresses entirely, `"warning"` preserves current behavior, `"error"` makes it a hard error.
- **CollectionStyle in the parser AST** — adding a `style` field to `Node::Mapping`/`Node::Sequence` is a breaking change to the public API of `rlsp-yaml-parser`. This is acceptable: it's a semver-minor addition (new field), and the parser is pre-1.0. All match sites need updating, but the compiler catches them all.
- **Flow rendering uses the pretty-printer's `group()` / `line()` / `indent()`** — flow collections use the Wadler-Lindig algorithm to decide between single-line and multi-line output based on `print_width`. Short flow collections stay inline (`[a, b]`); long ones break across lines with indentation. This handles both compact and multiline flow styles correctly. The `bracket_spacing` setting applies to flow mappings.
