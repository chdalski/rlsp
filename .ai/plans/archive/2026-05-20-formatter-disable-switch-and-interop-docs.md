**Repository:** root
**Status:** Completed (2026-05-20)
**Created:** 2026-05-20

# Formatter disable switch and interop documentation

## Goal

Let users disable rlsp-yaml's built-in formatter through a
single workspace setting so they can use an external
formatter (Prettier, dprint, an in-house tool) for save-time
formatting while keeping every other LSP feature
(diagnostics, hover, completion, code actions) active. Add a
`formatEnable` setting that gates the three LSP formatting
request handlers, and document both the setting and the
print-width-alignment caveat that arises when rlsp-yaml's
code actions coexist with an external formatter.

## Context

### Current state

- rlsp-yaml unconditionally responds to
  `textDocument/formatting`, `textDocument/rangeFormatting`,
  and `textDocument/onTypeFormatting`. There is no way to
  turn the formatter off without uninstalling the extension
  or losing other LSP features.
- Workspace settings live in the `Settings` struct in
  `rlsp-yaml/src/server.rs`. Existing formatter-related
  fields use a flat naming convention with the `format_`
  prefix on the Rust side (`format_print_width`,
  `format_single_quote`, …) and `formatXxx` on the
  client/JSON side (`formatPrintWidth`, `formatSingleQuote`,
  …).
- The three formatting handlers live in
  `rlsp-yaml/src/server.rs` at lines 1078, 1165, and 1274
  respectively.
- A fourth reader of the `Settings` struct's `format_*`
  fields is the `code_action` handler in
  `rlsp-yaml/src/server.rs` (around line 975, with the
  `settings.lock()` call near line 999). It assembles a
  `YamlFormatOptions` for code-action text edits using the
  same `format_print_width`, `format_single_quote`,
  `format_preserve_quotes`, `format_bracket_spacing`,
  `format_enforce_block_style`,
  `format_remove_duplicate_keys`, and
  `format_indent_sequences` fields the formatting handlers
  use. This reader must **not** check `formatEnable` —
  gating code actions is an explicit non-goal (see
  Non-Goals). It is enumerated here so the developer
  recognizes the full blast radius of widening `Settings`
  and does not accidentally extend the gate to code
  actions.
- LSP capability advertisement is in
  `rlsp-yaml/src/server.rs` around lines 596–601 (the three
  `document_*_formatting_provider` fields).
- Settings are ingested twice: once via
  `initializationOptions` at LSP `initialize`, and once via
  `workspace/didChangeConfiguration` at runtime. Both paths
  deserialize into the same `Settings` struct.
- VS Code extension config is in
  `rlsp-yaml/integrations/vscode/package.json` (configuration
  properties block, lines 35–156) and
  `rlsp-yaml/integrations/vscode/src/config.ts` (the
  `ServerSettings` interface and `getConfig()` function).
- Zed integration passes settings through generically (no
  per-setting schema), so no Zed change is needed — the
  setting will flow through the LSP `initialization_options`
  unchanged.

### Prior decision being honored

The follow-up memory item "Document rlsp-yaml ↔ prettier
(and other formatters) interop" notes that the
2026-04-27 code-action-respects-user-format-config plan made
code actions (e.g. `block_to_flow`) emit text using
rlsp-yaml's `formatPrintWidth`. Users who keep Prettier as
their save-time formatter then maintain two parallel
print-width settings (`formatPrintWidth` for code-action
output shape, `printWidth` for save reformatting); a
mismatch produces mid-edit jitter. The interop doc must
cover this.

### Industry convention

The Red Hat yaml-language-server, the VS Code HTML/JSON
language services, and TypeScript's tsserver all expose a
`format.enable` style setting that defaults to `true`.
Capabilities remain advertised; the handler returns no edits
when the setting is `false`. We follow the same pattern —
matching user expectations from neighboring LSPs and
avoiding the complexity of dynamic capability
re-registration on settings change.

### Specifications referenced

- LSP 3.17 §`textDocument/formatting`,
  `textDocument/rangeFormatting`,
  `textDocument/onTypeFormatting` — the three requests being
  gated. The spec permits returning `null` to indicate no
  edits, which is the response shape used when the
  formatter is disabled.

### Key files

- `rlsp-yaml/src/server.rs` — `Settings` struct, capability
  registration, three formatting handlers
- `rlsp-yaml/integrations/vscode/package.json` —
  configuration properties block
- `rlsp-yaml/integrations/vscode/src/config.ts` —
  `ServerSettings` interface and `getConfig()`
- `rlsp-yaml/docs/configuration.md` — workspace settings
  reference and Formatting section
- `rlsp-yaml/README.md` — formatter mention in the
  configuration example
- `rlsp-yaml/docs/feature-log.md` — user-facing feature
  decisions

### Test surface

Existing integration tests for formatting (search the
`rlsp-yaml/tests/` directory and `#[cfg(test)] mod tests`
blocks in `server.rs`) establish the pattern for exercising
formatting requests end-to-end through the LSP entry point.
The new tests reuse that pattern — the developer should
follow the existing approach rather than inventing a new
one.

## Steps

- [x] Add `formatEnable` (default `true`) to the Rust
      `Settings` struct
- [x] Gate the three formatting handlers (`formatting`,
      `range_formatting`, `on_type_formatting`) to return
      `Ok(None)` when `formatEnable` is `false`
- [x] Add integration tests exercising each of the three
      handlers with `formatEnable` set to `false`, and a
      sanity test that default-on behavior is unchanged
- [x] Add `rlsp-yaml.formatEnable` to the VS Code
      `package.json` configuration block
- [x] Extend the VS Code `ServerSettings` interface and
      `getConfig()` to map the new setting
- [x] Document `formatEnable` in `docs/configuration.md`
- [x] Add an "Interop with external formatters" subsection
      to `docs/configuration.md` covering the disable
      switch, the code-action print-width alignment caveat,
      and the explicit non-promises (no `.prettierrc` /
      `.editorconfig` reading in this plan)
- [x] Reference the disable switch in `rlsp-yaml/README.md`
      where the formatter is mentioned
- [x] Add a `feature-log.md` entry for the new setting

## Tasks

### Task 1: Implement the `formatEnable` setting end-to-end

**Commit:** 33cceb6

Add the new setting to the Rust server and the VS Code
extension, gate the three formatting LSP handlers, and add
integration tests that exercise each handler through the
production LSP entry point. The setting defaults to `true`
so existing users see no behavioral change.

- [x] `Settings` struct in `rlsp-yaml/src/server.rs` gains
      a `format_enable: Option<bool>` field deserialized
      from the JSON key `formatEnable`
- [x] `formatting()`, `range_formatting()`, and
      `on_type_formatting()` each check the setting at
      request time and return `Ok(None)` when
      `formatEnable` resolves to `false`. The setting check
      is the first action after extracting the URI — it
      runs before document lookup or option construction
      so disabled-state requests do no extra work
- [x] Capability advertisement at lines 596–601 of
      `server.rs` is unchanged. Document the rationale in
      the test module: matches industry convention, avoids
      dynamic capability re-registration on config change.
      Setting takes effect immediately for any new request
      without reinitialization
- [x] Integration tests in the appropriate test file
      (follow the existing pattern in
      `rlsp-yaml/tests/`):
  - `formatting` returns `None` when `formatEnable: false`
  - `range_formatting` returns `None` when
    `formatEnable: false`
  - `on_type_formatting` returns `None` when
    `formatEnable: false`
  - Each handler returns its usual edits when
    `formatEnable` is unset (default-on) and when
    explicitly `true`
- [x] VS Code `package.json` adds the
      `rlsp-yaml.formatEnable` property with type
      `boolean`, default `true`, and a description that
      mentions the LSP behavior and references
      `docs/configuration.md` for the interop story
- [x] VS Code `ServerSettings` interface adds
      `formatEnable: boolean` and `getConfig()` reads it
      with default `true`
- [x] `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` all pass with zero warnings
- [x] `pnpm run lint`, `pnpm run format`, `pnpm run build`,
      and `pnpm run test` in
      `rlsp-yaml/integrations/vscode` all pass
- [x] **Advisors:** consult test-engineer for a test list
      before implementing; get test-engineer sign-off on the
      completed implementation before submitting to the
      reviewer. No security-engineer consultation required
      (no trust boundary, no untrusted input, no
      cryptography — only handler gating)

### Task 2: Document the setting and external-formatter interop

**Commit:** 5a71f68

Add user-facing documentation describing the new setting,
the recommended workflow for users who use an external
formatter, and the print-width alignment caveat raised by
code actions that consume `formatPrintWidth`. No source
code changes.

- [x] `docs/configuration.md` gains a `formatEnable` entry
      in the workspace settings reference, placed adjacent
      to the other `format*` settings. The entry states
      the default, the LSP requests it gates, and what the
      server still does when the setting is `false`
      (diagnostics, hover, completion, code actions all
      continue to work; code-action edits still emit in
      rlsp-yaml's own style)
- [x] `docs/configuration.md` gains an "Interop with
      external formatters" subsection in the Formatting
      section that covers:
  - How to disable the built-in formatter via
    `formatEnable: false`
  - That VS Code users typically also set
    `editor.defaultFormatter` for `[yaml]` to their
    preferred external formatter (and a brief note that
    other editors have analogous mechanisms)
  - The code-action print-width caveat: when
    `formatEnable: false`, code-action text edits still
    use `formatPrintWidth`. Users running an external
    formatter should keep that formatter's print width and
    rlsp-yaml's `formatPrintWidth` aligned to avoid
    mid-edit jitter (action wraps at 80, save reformats at
    100, or vice versa). Lists the equivalent settings for
    Prettier (`printWidth`, `singleQuote`,
    `bracketSpacing`) so users can keep them in sync
  - An explicit "Not in scope" note: rlsp-yaml does not
    read `.prettierrc`, `.dprint.json`, or `.editorconfig`
    today. `.editorconfig` support is a separate, planned
    feature
- [x] `rlsp-yaml/README.md` — in the existing formatter
      mention, add one sentence pointing to
      `formatEnable` and the configuration doc for the
      interop story
- [x] `rlsp-yaml/docs/feature-log.md` gains a single entry
      for `formatEnable` with date and one-line
      description of user-facing behavior
- [x] Verify cross-references: the configuration.md entry,
      the README mention, and the feature-log entry use
      the same name (`formatEnable`) and the same default
      (`true`). No stale references to "format is always
      on" or "formatter cannot be disabled" remain in any
      doc file
- [x] Remove the "Document rlsp-yaml ↔ prettier (and other
      formatters) interop" item from
      `/workspace/.ai/memory/project_followup_plans.md` —
      this plan fully delivers the follow-up, and leaving
      the entry in place would mislead future sessions
      into thinking the work is still open. (The memory
      file lives outside the repository and is updated by
      a separate lead-side edit, not committed as part of
      Task 2 — but the change is the responsibility of
      this task's completion)
- [x] **Advisors:** no advisor consultation required
      (documentation-only task, no behavior change, no test
      coverage gap)

## Decisions

- **Setting name:** `formatEnable` (flat camelCase under
  the `rlsp-yaml.` namespace), matching the existing
  `formatPrintWidth`, `formatSingleQuote`, etc. convention.
  Rejected `format.enable` (nested object): would force
  the entire settings layout to change for one new key, and
  the existing flat layout is already established.
- **Default:** `true`. Preserves current behavior for every
  existing user; opting out is the deliberate action.
- **Granularity:** one switch covers all three formatting
  handlers (full document, range, on-type). Rejected
  per-handler switches: no user need has been identified
  for partial-formatter modes, and YAGNI applies.
- **Capability advertisement:** unconditional. Matches the
  Red Hat yaml-language-server and VS Code's HTML/JSON
  language services. Dynamic capability re-registration on
  config change is out of scope — the runtime cost of
  letting handlers return `Ok(None)` when disabled is
  negligible, and clients that toggle the setting see the
  change on the next request without any LSP-level
  handshake.
- **Interop doc scope:** documents `formatEnable` and the
  code-action print-width alignment caveat (from the
  follow-up memory item). Explicitly out of scope:
  automatic `.prettierrc` reading, automatic
  `.editorconfig` reading. `.editorconfig` support has its
  own future plan, which is the reason this plan exists as
  a standalone deliverable.
- **Why this is two tasks, not one:** the Rust+VS Code
  implementation and the docs are independently
  committable. Task 1 ships behavior; Task 2 documents the
  surface. Bundling would produce a single commit that
  mixes Rust source, TypeScript source, and Markdown,
  obscuring the review surface for each.

## Non-Goals

- `.editorconfig` support — covered by a separate planned
  follow-up. The interop doc explicitly states this.
- Reading `.prettierrc`, `.dprint.json`, or any other
  external formatter config to align settings
  automatically. The doc instructs users to align settings
  manually.
- Dynamic capability registration/unregistration in
  response to `workspace/didChangeConfiguration`. Handler
  gating is sufficient.
- Per-handler granularity (a separate switch for full /
  range / on-type formatting). Not requested by any user.
- Changing the behavior of code actions that emit text
  edits using `formatPrintWidth`. The interop doc explains
  the existing behavior; it does not propose changes.
- Zed extension schema changes. Settings pass through
  generically; the new key reaches the server without any
  Zed-side update.
