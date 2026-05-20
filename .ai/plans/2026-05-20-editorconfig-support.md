**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-20

# `.editorconfig` support for the rlsp-yaml formatter

## Goal

Let users centralize formatting preferences in a single
`.editorconfig` file at the repository root and have
rlsp-yaml honor those values when formatting YAML.
Specifically: when a user opens a YAML file, the formatter
walks up from the file's directory, finds the nearest
`.editorconfig`, and applies the YAML-relevant settings
(`max_line_length`, `end_of_line`, `insert_final_newline`)
unless an explicit workspace setting overrides them.
Changes to `.editorconfig` during a session take effect on
the next format request via a file watcher. Users with
incompatible `.editorconfig` setups can opt out via a new
`formatRespectEditorconfig` setting (default `true`).

## Context

### Current state

- rlsp-yaml does not read `.editorconfig`. The 2026-05-20
  formatter-disable-switch plan's interop documentation
  explicitly states this and notes that `.editorconfig`
  support is a separate, planned feature — this plan
  delivers that.
- The formatter (`rlsp-yaml/src/editing/formatter.rs`)
  emits LF line endings only — `let mut out =
  result_lines.join("\n");` at line 310 (inside the
  private helper `attach_comments`). There is no
  configurable line-ending logic today.
- The formatter always appends a trailing newline, in
  **two independent locations**:
  - `attach_comments` at `formatter.rs:311–312` (private
    helper, line numbers are inside the function that
    starts at line 171)
  - `format_yaml` at `formatter.rs:479–481` (public
    entry point, function starts at line 415)
  The call order is: `format_yaml` builds the formatted
  text, appends a trailing newline at 479–481 if
  missing, then calls `attach_comments` at line 487 as
  its final step. `attach_comments` joins comment-
  annotated lines and appends its own unconditional
  trailing newline at 311–312. The output of
  `attach_comments` is what `format_yaml` returns.
  **Consequence for this plan:** `insert_final_newline:
  false` cannot be implemented by gating only one of
  these two locations — `attach_comments` runs last and
  would re-append a newline no matter what
  `format_yaml` did. The implementation must either
  (a) thread the `insert_final_newline` and
  `line_ending` parameters into `attach_comments`'s
  signature, or (b) keep `attach_comments` internal
  (always LF, always trailing newline) and apply the
  final line-ending and trailing-newline decisions in
  `format_yaml` *after* `attach_comments` returns. The
  plan recommends (b) below.
- The formatter implicitly trims trailing whitespace as a
  side effect of the line-joining logic — no explicit toggle.
- LSP file-watcher infrastructure is already wired:
  `server.rs:688–710` registers
  `**/*.yaml` and `**/*.yml` watchers in the `initialized`
  handler. The `did_change_watched_files` handler at
  `server.rs:716` republishes diagnostics on watched-file
  changes. Adding a third watcher for `**/.editorconfig`
  is a single insertion.
- **`YamlFormatOptions` construction sites in `server.rs`
  (three, not four).** Only two of the three LSP format
  handlers construct `YamlFormatOptions` — the third
  (`on_type_formatting`) computes indentation only and
  never builds the struct. The full enumeration:
  - `formatting()` at line 1084, constructs
    `YamlFormatOptions` at `server.rs:1110`
  - `range_formatting()` at line 1180, constructs
    `YamlFormatOptions` at `server.rs:1210`
  - `code_action()` at line 988, constructs
    `YamlFormatOptions` at `server.rs:1006` (exhaustive
    explicit field initialization for all 9 fields, so
    adding a new field requires updating this site)
  - `on_type_formatting()` at line 1299 calls
    `format_on_type(docs, position, ch, tab_size)` and
    passes no formatter options — `.editorconfig` cannot
    influence on-type indentation through this path, and
    on-type formatting does not reformat lines for
    `max_line_length`, `end_of_line`, or
    `insert_final_newline` anyway
- **Downstream code-action callsites that re-construct
  `YamlFormatOptions`.** `yaml11_octal.rs:61` and
  `yaml11_bool.rs:60` build a `quote_opts` value using
  struct-update syntax (`..options.clone()`), so they
  automatically inherit any new fields from the cloned
  `options` value. They do not need explicit changes for
  new fields. The source of their `options` is the
  `code_action()` handler in `server.rs:1006`, which is
  the construction site that must be updated explicitly
- VS Code's `config.ts:34` reads
  `cfg.get('formatPrintWidth', 80)`. When the user has not
  set the value, VS Code returns the `package.json`
  default (`80`), not `undefined`. The server therefore
  receives `Some(80)` regardless of whether the user set
  the value explicitly. This breaks the
  `explicit LSP > .editorconfig > defaults` precedence for
  `formatPrintWidth` specifically — `.editorconfig`'s
  `max_line_length` would never apply because the LSP side
  always looks "explicit."
- The `Settings` struct in `server.rs` already uses
  `Option<T>` for formatter fields. The server-side
  precedence logic (LSP setting `Some(v)` → use `v`; `None`
  → fall through to default) is already in place. The
  remaining work is on the client side: only send the
  field when the user actually set it.
- Document URIs in the document store are `tower_lsp::Url`
  values; `Url::to_file_path()` converts a `file://` URI
  to a `PathBuf` suitable for walking up to find a sibling
  `.editorconfig`.
- Workspace folders are not tracked anywhere in the
  server. `.editorconfig` discovery walks up from each
  open document's directory rather than rooting from a
  workspace folder, so this gap is not a blocker.

### Settings flow and clarifications confirmed with the user

- **`indent_size`/`indent_style`:** not read from
  `.editorconfig`. The existing contract documented in
  `docs/configuration.md` ("Tab width is taken from the
  `tab_size` field of the LSP `textDocument/formatting`
  request") is preserved. Modern editors with
  `.editorconfig` plugin support already feed
  `.editorconfig` indent values into their own tab width,
  which then flows into the LSP request — we trust that
  pathway rather than reading the file ourselves.
- **`indent_style = tab`:** silently ignored. YAML 1.2
  §6.1 forbids tab indentation; we already silently
  ignore `insertSpaces: false` from the LSP request.
- **`max_line_length`:** maps to `formatPrintWidth` when
  no explicit LSP setting is in effect.
- **`end_of_line`:** maps to the formatter's line-ending
  output (`lf` → `\n`, `crlf` → `\r\n`, `cr` → `\r`).
  Default remains `\n`.
- **`insert_final_newline`:** maps to whether the
  formatter appends a trailing newline. Default `true`.
- **`trim_trailing_whitespace`:** not honored. The
  formatter always trims as a side effect of its own
  logic. Setting `trim_trailing_whitespace = false` in
  `.editorconfig` is documented as ineffective.
- **`charset`:** not honored. The formatter is UTF-8 only.
  Other charsets are documented as unsupported.
- **Live reload:** via `workspace/didChangeWatchedFiles`.
  Edits to `.editorconfig` take effect on the next format
  request without restarting the server.
- **Opt-out:** new setting `formatRespectEditorconfig`
  (default `true`). When `false`, the server ignores
  `.editorconfig` entirely.

### Spec and crate

- `.editorconfig` file format spec:
  <https://editorconfig.org/>. Notable rules:
  - Walk up from the file's directory; stop at the first
    `.editorconfig` containing `root = true` (case-
    insensitive), or at the filesystem root.
  - Within each file, sections are matched against the
    target file path using glob patterns (`[*.yaml]`,
    `[*.{yml,yaml}]`, `[*]`). Later sections override
    earlier ones.
  - Keys and values are case-insensitive; values are
    lower-cased before use.
- Crate choice: **`ec4rs`** (current, well-maintained
  Rust `.editorconfig` parser, handles the full spec
  including glob matching and `root = true` semantics).
  Added to `rlsp-yaml/Cargo.toml`.

### Key files

- `rlsp-yaml/Cargo.toml` — add `ec4rs` dependency
- `rlsp-yaml/src/editing/editor_config.rs` — new module
  for `.editorconfig` resolution, parsing, and caching
- `rlsp-yaml/src/server.rs` — three format handlers,
  watcher registration, `did_change_watched_files`
  invalidation, and the new
  `format_respect_editorconfig` setting
- `rlsp-yaml/src/editing/formatter.rs` — line-ending and
  trailing-newline handling
- `rlsp-yaml/integrations/vscode/package.json` — new
  `rlsp-yaml.formatRespectEditorconfig` property; existing
  `rlsp-yaml.formatPrintWidth` may need its default
  removed or remain (see VS Code task below)
- `rlsp-yaml/integrations/vscode/src/config.ts` — switch
  `formatPrintWidth` to `inspect()`-based reading; add
  `formatRespectEditorconfig` field
- `rlsp-yaml/docs/configuration.md` — `.editorconfig`
  section, new setting reference, update of the existing
  "Interop with external formatters" section
- `rlsp-yaml/README.md` — `.editorconfig` mention
- `rlsp-yaml/docs/feature-log.md` — single user-facing
  entry
- `/workspace/.ai/memory/project_followup_plans.md` —
  remove the `.editorconfig` follow-up item once the plan
  ships

### Test surface

Existing integration tests for the formatter (look in
`rlsp-yaml/tests/`) establish the pattern for exercising
format requests end-to-end through the LSP entry point.
`.editorconfig` fixtures live under
`rlsp-yaml/tests/fixtures/editor_config/` (new
subdirectory). Each fixture pairs an `.editorconfig`
content with a YAML input and expected formatted output.

## Steps

- [x] Add `ec4rs` to `rlsp-yaml/Cargo.toml`
- [x] Implement `editor_config.rs` module: URI →
      filesystem path, walk-up to find `.editorconfig`,
      parse via `ec4rs`, return resolved YAML-relevant
      settings, cache by directory, invalidate function
- [x] Unit tests for `editor_config.rs`: walking,
      `root = true` semantics, glob matching against
      `[*.yaml]` / `[*.yml]` / `[*]`, key-precedence,
      malformed-file handling
- [ ] Fix VS Code `config.ts` to use
      `WorkspaceConfiguration.inspect('formatPrintWidth')`
      and omit the field when the user has not set it
- [ ] Make `formatPrintWidth?: number` optional in
      `ServerSettings` (TypeScript)
- [ ] Vitest tests for the `config.ts` change
- [ ] Wire the format handlers to read `.editorconfig`
      and overlay onto LSP settings with the correct
      precedence (explicit LSP > `.editorconfig` >
      defaults)
- [ ] Extend `YamlFormatOptions` with line-ending and
      trailing-newline fields; thread them through the
      formatter
- [ ] Register a `**/.editorconfig` FileSystemWatcher in
      `initialized` and invalidate the cache in
      `did_change_watched_files`
- [ ] Integration tests with `.editorconfig` fixtures
      exercising `max_line_length`, `end_of_line`,
      `insert_final_newline`, walk-up resolution, and
      `root = true` termination
- [ ] Add `format_respect_editorconfig: Option<bool>`
      (default `true`) to `Settings`; gate the
      `.editorconfig` lookup when `false`
- [ ] Add `rlsp-yaml.formatRespectEditorconfig` to VS Code
      `package.json` and `config.ts`
- [ ] Document `.editorconfig` support in
      `docs/configuration.md`; update the "Interop with
      external formatters" section to remove the
      ".editorconfig not read" non-promise
- [ ] Add a README mention and a `feature-log.md` entry
- [ ] Remove the `.editorconfig` follow-up item from
      `/workspace/.ai/memory/project_followup_plans.md`

## Tasks

### Task 1: `.editorconfig` parser module and cache

**Commit:** d05abd5

Implement the `.editorconfig` resolution, parsing, and
caching infrastructure as a standalone module. No
integration with the format handlers in this task — the
module is exercised entirely through unit tests.

- [x] Add `ec4rs` to `rlsp-yaml/Cargo.toml` with a
      conservative version (latest stable on crates.io
      as of plan execution; the developer pins it)
- [x] Create `rlsp-yaml/src/editing/editor_config.rs`
      exposing:
  - `EditorConfigSettings` struct with the YAML-relevant
    resolved fields:
    - `max_line_length: Option<usize>`
    - `end_of_line: Option<LineEnding>` (an enum: `Lf`,
      `Crlf`, `Cr`)
    - `insert_final_newline: Option<bool>`
  - `resolve(uri: &Url) -> EditorConfigSettings` function:
    - Converts the URI to a `PathBuf` via
      `Url::to_file_path()`. If conversion fails (non-
      file URI), returns an empty settings struct.
    - Walks up the directory tree using `ec4rs`'s
      built-in walker until `root = true` or the
      filesystem root is reached.
    - Returns the resolved settings for the file path.
  - Cache: a `Mutex<HashMap<PathBuf, EditorConfigSettings>>`
    keyed on the directory containing the file (not the
    file itself — directory-level caching is sufficient
    because `.editorconfig` resolution depends only on
    the directory hierarchy).
  - `invalidate_all()` function: clears the cache.
    Called by `did_change_watched_files` in Task 3.
- [x] Handle the failure modes explicitly:
  - Non-file URI (e.g. `untitled:`, `inmemory:`) — return
    empty settings, do not panic
  - File path resolves but no `.editorconfig` exists in
    any parent directory — return empty settings
  - `.editorconfig` exists but is malformed — `ec4rs`
    returns an error; log at `debug` level and treat the
    file as if it did not exist. Do not surface as an LSP
    diagnostic in this task (the file is not the
    document being edited)
  - `indent_style = tab` in a section that matches the
    file — recognized but silently dropped (YAML forbids
    tabs; this is consistent with `insertSpaces: false`
    handling)
- [x] Unit tests in `#[cfg(test)] mod tests` covering:
  - **resolves an empty result when no `.editorconfig`
    exists** — fixture: temp dir with only a YAML file;
    expect all fields `None`
  - **honors `[*.yaml]` section** — fixture: file in dir
    with `.editorconfig` setting
    `max_line_length = 100`; expect
    `max_line_length = Some(100)`
  - **honors `[*.{yml,yaml}]` section** — fixture: both
    `.yml` and `.yaml` files resolve the same settings
  - **walks up multiple directories** — fixture: nested
    dirs, `.editorconfig` two levels up; expect the
    walker finds it
  - **`root = true` terminates walk** — fixture: inner
    `.editorconfig` with `root = true` and a value,
    outer `.editorconfig` with a different value;
    expect only the inner value
  - **later sections override earlier ones in the same
    file** — fixture: `.editorconfig` with `[*]` then
    `[*.yaml]`; YAML-specific section wins
  - **`indent_style = tab` is silently dropped** — fixture
    sets it; the resolved settings do not surface a tab
    style anywhere
  - **malformed `.editorconfig`** — fixture: a file with
    invalid syntax; expect empty resolved settings, no
    panic
  - **non-file URI returns empty** — pass an
    `untitled:Untitled-1` URI; expect empty settings
- [x] **Cause line not applicable** (new feature, not a
      fix). Include a one-line implementation summary in
      the handoff describing the cache strategy and walk
      semantics.
- [x] **Advisors:**
  - **test-engineer** required. **Input gate:** consult
    before implementing for a test list (greenfield code
    with no existing test patterns for filesystem
    walking + `.editorconfig` semantics is exactly the
    high-uncertainty category). **Output gate:** TE
    sign-off before submitting to the reviewer
  - **security-engineer** required. **Input gate:**
    consult before implementing for a risk assessment.
    `.editorconfig` files are untrusted input from the
    filesystem — a malicious file in a checked-out
    repository could attempt to exploit the parser (very
    long lines, pathological glob patterns, deeply
    nested directory walks). **Output gate:** security
    sign-off on the completed implementation
- [x] `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` all pass with zero warnings

### Task 2: VS Code `formatPrintWidth` defaults fix

The current `config.ts:34` sends `formatPrintWidth: 80`
to the server when the user has not set the value (the
`package.json` default fills in). The server treats this
as an explicit LSP setting and the
`explicit > .editorconfig > defaults` precedence rules
out `.editorconfig`'s `max_line_length`. Fix by using
`WorkspaceConfiguration.inspect()` to detect the user-set
state and omit the field when unset.

- [ ] Modify `rlsp-yaml/integrations/vscode/src/config.ts`
      to read `formatPrintWidth` via
      `cfg.inspect<number>('formatPrintWidth')`:
  - If `globalValue`, `workspaceValue`, or
    `workspaceFolderValue` is defined, the user set it;
    include the field in the result with that value
  - Otherwise, omit the field from the returned object
    entirely (use a conditional spread)
- [ ] Update `ServerSettings` (TypeScript) to declare
      `formatPrintWidth?: number` (optional)
- [ ] Leave other formatter settings unchanged in this
      task — only `formatPrintWidth` needs `.editorconfig`
      override capability. Other `format*` settings have
      no `.editorconfig` equivalent in this plan
- [ ] Keep `rlsp-yaml.formatPrintWidth`'s `"default": 80`
      in `package.json` so the VS Code settings UI still
      shows the default visibly to users — `inspect()`
      already distinguishes the package.json default
      from a user-set value
- [ ] Vitest unit tests in
      `rlsp-yaml/integrations/vscode/src/config.test.ts`
      (create if absent, otherwise extend) covering:
  - When the user has not set `formatPrintWidth`,
    `getConfig()` returns an object whose
    `formatPrintWidth` property is `undefined` (i.e.
    `'formatPrintWidth' in result === false`)
  - When the user explicitly sets `formatPrintWidth: 80`,
    the returned object includes `formatPrintWidth: 80`
  - When the user explicitly sets `formatPrintWidth: 100`,
    the returned object includes `formatPrintWidth: 100`
- [ ] No server-side changes in this task — the existing
      `Settings.format_print_width: Option<usize>` field
      already deserializes a missing JSON key to `None`,
      which is the behavior we want
- [ ] **Advisors:**
  - **test-engineer** required. **Input gate:** consult
    for a test list (changes a public-surface client
    behavior; the test approach for mocking VS Code's
    `WorkspaceConfiguration` is non-obvious). **Output
    gate:** TE sign-off before submitting to the reviewer
  - **security-engineer** not required (no trust
    boundary, no untrusted input, no cryptography —
    purely a settings serialization change)
- [ ] `pnpm run lint`, `pnpm run format`, `pnpm run build`,
      and `pnpm run test` in
      `rlsp-yaml/integrations/vscode` all pass

### Task 3: Integrate `.editorconfig` into format handlers and watcher

Wire the `editor_config.rs` module from Task 1 into the
three format handlers, extend `YamlFormatOptions` with
line-ending and trailing-newline fields, modify the
formatter to honor those fields, register a third file
watcher, and invalidate the cache on `.editorconfig`
changes.

- [ ] Extend `YamlFormatOptions` in
      `rlsp-yaml/src/editing/formatter.rs` with:
  - `line_ending: LineEnding` (default `LineEnding::Lf`)
    — same enum as the `editor_config.rs` module exposes
  - `insert_final_newline: bool` (default `true`)
- [ ] Modify the formatter output to honor these fields.
      `format_yaml` calls `attach_comments` as its last
      step (`formatter.rs:487`), and `attach_comments`
      currently joins lines with `"\n"` at line 310 and
      unconditionally appends a trailing newline at
      311–312 — overriding any decision `format_yaml`
      makes at lines 479–481. To avoid the override:

  **Recommended approach (option b in Context):** keep
  `attach_comments` internal — leave its line-joining
  on `"\n"` and its trailing-newline behavior
  unchanged. In `format_yaml`, after the call to
  `attach_comments` at line 487 returns:

  - If `options.line_ending != LineEnding::Lf`, replace
    every `"\n"` in the result with the chosen line
    ending. (No CR-handling complications because
    `attach_comments` produces only LF.)
  - If `options.insert_final_newline == false`, strip a
    single trailing line terminator (`"\n"`, `"\r\n"`,
    or `"\r"` depending on `line_ending`) if present.
    `attach_comments` always leaves one, so stripping
    one is sufficient.
  - Remove the now-redundant
    `if !result.ends_with('\n') { result.push('\n'); }`
    at lines 479–481 — `attach_comments` already
    guarantees one, and the new post-processing step
    enforces the final policy. Leaving it in place is
    harmless but dead.

  **Alternative approach (option a):** thread
  `line_ending` and `insert_final_newline` into
  `attach_comments`'s signature and condition both the
  `join` and the trailing-newline append at lines 310
  and 311–312. Then `format_yaml` does not need a
  post-processing step. The developer may choose this
  approach if they prefer keeping the policy local to
  one function — both produce identical output. State
  the choice in the handoff so the reviewer can verify
  the design.
- [ ] Update the three `YamlFormatOptions` construction
      sites in `server.rs` to apply the `.editorconfig`
      overlay. These are:
  - `formatting()` at `server.rs:1110`
  - `range_formatting()` at `server.rs:1210`
  - `code_action()` at `server.rs:1006`

  `on_type_formatting()` does NOT construct
  `YamlFormatOptions` (it calls `format_on_type` with
  only `tab_size`) and does NOT need changes. Skipping
  it is intentional — on-type formatting computes
  indentation only and is unaffected by
  `max_line_length`, `end_of_line`, and
  `insert_final_newline`.

  At each of the three sites, before constructing
  `YamlFormatOptions`:
  - If `format_respect_editorconfig` is `false`
    (handled in Task 4 but already gate-defensive here),
    skip `.editorconfig` lookup
  - Otherwise, call
    `editor_config::resolve(&uri)` to fetch the resolved
    `.editorconfig` settings for the document
  - Apply precedence per field:
    - `print_width`: `Settings.format_print_width`
      `Some(v)` → use `v`; else `EditorConfigSettings
      .max_line_length` `Some(v)` → use `v`; else default
      80
    - `line_ending`: only set by `.editorconfig`'s
      `end_of_line` (LSP has no equivalent setting);
      `Some(v)` → use `v`; else `Lf`
    - `insert_final_newline`: only set by
      `.editorconfig`; `Some(v)` → use `v`; else `true`
  - Keep all other field handling unchanged
- [ ] At the `code_action()` construction site
      (`server.rs:1006`), the struct literal exhaustively
      initializes all 9 existing fields. The two new
      fields (`line_ending`, `insert_final_newline`) must
      be added to this literal in addition to the overlay
      logic above — otherwise the code does not compile.
      The downstream code-action callsites in
      `yaml11_octal.rs:61` and `yaml11_bool.rs:60` use
      `..options.clone()` struct-update syntax and
      automatically inherit the two new fields from the
      cloned `options`; they require no changes
- [ ] Update the fixture-harness `apply_setting`
      functions per the Settings Sync table in the root
      `CLAUDE.md` (which mandates this whenever
      `YamlFormatOptions` gains a field). Specifically:
  - In `rlsp-yaml/tests/formatter_fixtures.rs`,
    extend the `apply_setting` match to recognize
    `line_ending` (string: `lf`/`crlf`/`cr`) and
    `insert_final_newline` (boolean) keys and set the
    corresponding fields on the `YamlFormatOptions`
    under test
  - In `rlsp-yaml/tests/code_action_fixtures.rs`, apply
    the same extension to its `apply_setting`
    equivalent so code-action fixtures can also drive
    the two new fields
  - In `rlsp-yaml/tests/fixtures/CLAUDE.md`, extend the
    documented `format-options:` key table to list the
    two new keys alongside the existing ones
    (`print_width`, `tab_width`, `single_quote`, etc.)
  Without these updates, future fixtures that specify
  `line_ending:` or `insert_final_newline:` in their
  `settings:` block will silently use the defaults
  (existing harness comment: "Unknown settings keys are
  silently ignored"), producing tests that look correct
  but exercise the wrong configuration
- [ ] Register a third FileSystemWatcher in the
      `initialized` handler (`server.rs:688–710`):
  - Glob pattern: `**/.editorconfig`
  - Watch kind: `WatchKind::all()`
- [ ] In `did_change_watched_files` (`server.rs:716`),
      when any received change has a URI matching
      `.editorconfig` (file name ends with
      `.editorconfig`), call
      `editor_config::invalidate_all()`. The next format
      request re-resolves
- [ ] Code-action edits inherit the `.editorconfig`
      overlay automatically because their source
      `options` is the `code_action()` handler's
      `YamlFormatOptions` (now updated above), and
      `yaml11_octal.rs:61` / `yaml11_bool.rs:60` use
      `..options.clone()`. No code change in those two
      files. The integration tests in this task must
      include at least one case proving code-action
      output respects `.editorconfig` — consistent with
      the precedent set by the 2026-04-27 code-action-
      respect-user-format-config plan
- [ ] Integration tests in `rlsp-yaml/tests/` (a new file
      `editorconfig_integration.rs` or extension of an
      existing one — developer chooses based on file
      sizes):
  - `max_line_length = 100` in `.editorconfig` overrides
    the default 80 print width when no explicit
    `formatPrintWidth` is set
  - Explicit `formatPrintWidth = 60` overrides
    `.editorconfig`'s `max_line_length = 100`
  - `end_of_line = crlf` produces CRLF output
  - `end_of_line = lf` (default) produces LF output
  - `insert_final_newline = false` omits the trailing
    newline
  - `insert_final_newline = true` (default) appends a
    trailing newline
  - Walk-up resolution: `.editorconfig` two directories
    above the YAML file is found and applied
  - `root = true` in the inner file terminates the walk
- [ ] **Cause line not applicable** (feature
      integration). Include in the handoff: an
      implementation summary describing the precedence
      logic and the watcher invalidation flow
- [ ] **Advisors:**
  - **test-engineer** required. **Input gate:** consult
    for a test list (cross-component integration with
    file I/O, watchers, and formatter output — high
    uncertainty about coverage shape). **Output gate:**
    TE sign-off before submitting to the reviewer
  - **security-engineer** not required (no new trust
    boundary in this task — Task 1 already covered
    untrusted-input parsing concerns)
- [ ] `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` all pass with zero warnings

### Task 4: Opt-out setting, documentation, and follow-up cleanup

Add the `formatRespectEditorconfig` setting (default
`true`) so users with conflicting `.editorconfig` setups
can opt out, write user-facing documentation, and clean
up the stale follow-up item.

- [ ] Add `format_respect_editorconfig: Option<bool>` to
      the `Settings` struct in `server.rs`, deserialized
      from the JSON key `formatRespectEditorconfig`
- [ ] At each of the three `YamlFormatOptions`
      construction sites in `server.rs` (`formatting()`
      at line 1110, `range_formatting()` at line 1210,
      `code_action()` at line 1006), gate the
      `editor_config::resolve()` call on
      `format_respect_editorconfig.unwrap_or(true)` —
      when `false`, skip the lookup and fall through to
      defaults. No additional gating in
      `yaml11_octal.rs` or `yaml11_bool.rs` — they
      inherit via `..options.clone()`
- [ ] Add `rlsp-yaml.formatRespectEditorconfig` to VS Code
      `package.json` with type `boolean`, default `true`,
      description: "When false, ignore `.editorconfig`
      files for YAML formatting. Defaults to true."
- [ ] Add `formatRespectEditorconfig: boolean` to the
      VS Code `ServerSettings` interface and read it via
      `cfg.get('formatRespectEditorconfig', true)` in
      `getConfig()`
- [ ] `docs/configuration.md`:
  - Add a `formatRespectEditorconfig` entry in the
    workspace settings reference, adjacent to the other
    `format*` settings. The entry states the default and
    the behavior when `false`
  - Add a new ".editorconfig support" section in or
    adjacent to the Formatting section covering:
    - Which settings are honored (`max_line_length`,
      `end_of_line`, `insert_final_newline`)
    - Which settings are explicitly NOT honored, with
      one-line reasons: `indent_size`/`indent_style`
      (taken from the LSP request's `tab_size`),
      `trim_trailing_whitespace` (always trimmed),
      `charset` (UTF-8 only)
    - `indent_style = tab` is silently ignored (YAML
      forbids tabs)
    - Precedence: explicit LSP setting > `.editorconfig`
      > defaults
    - Live reload via file watcher
    - How to opt out via `formatRespectEditorconfig`
  - Update the existing "Interop with external
    formatters" section: remove the bullet stating
    `.editorconfig` is not read. Replace with a one-line
    note pointing to the new ".editorconfig support"
    section
- [ ] `rlsp-yaml/README.md`: add a one-sentence mention
      in the existing formatter section pointing to the
      ".editorconfig support" section of
      `docs/configuration.md`
- [ ] `rlsp-yaml/docs/feature-log.md`: add a single entry
      with date and a one-line user-facing description
      of the new behavior
- [ ] Verify cross-references: the configuration.md
      section, the README mention, and the feature-log
      entry use the same name (`formatRespectEditorconfig`)
      and the same default (`true`). No stale references
      to "we do not read .editorconfig" remain in any doc
      file
- [ ] Remove the ".editorconfig support for the
      formatter" item from
      `/workspace/.ai/memory/project_followup_plans.md` —
      this plan fully delivers the follow-up. The memory
      file lives outside the repository; this is a
      lead-side edit, not part of the task commit, but
      remind the lead in the handoff that this cleanup
      is pending
- [ ] **Advisors:** no advisor consultation required
      (small Rust setting addition, small VS Code config
      addition, documentation, and memory cleanup — no
      behavior change beyond the gating expression, which
      is one method call)
- [ ] `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` all pass with zero warnings
- [ ] `pnpm run lint`, `pnpm run format`, `pnpm run build`,
      and `pnpm run test` in
      `rlsp-yaml/integrations/vscode` all pass

## Decisions

- **Crate choice:** `ec4rs` (current, well-maintained
  Rust `.editorconfig` parser). Rejected
  `editorconfig-rs` (less active) and rolling our own
  (the spec is non-trivial; glob matching with brace
  expansion alone is a meaningful subsystem).
- **Cache keyed by directory, not by file:**
  `.editorconfig` resolution depends only on the
  directory hierarchy and the glob-matching rules within
  files. Caching at directory granularity gives the same
  results with fewer entries; invalidation is wholesale
  on any `.editorconfig` change.
- **Wholesale cache invalidation on any `.editorconfig`
  change:** simpler than tracking which directories
  depend on which `.editorconfig` files; the cache is
  cheap to repopulate (filesystem walk + small file
  parses), and `.editorconfig` files change rarely
  during normal editing. Rejected per-directory
  invalidation as premature optimization (YAGNI).
- **Default values flow through `Option<T>`:** the
  server-side precedence is `Settings field Some(v) >
  EditorConfig field Some(v) > hardcoded default`.
  Mirrors the existing `Option`-based handling and keeps
  the precedence rule expressible as a chain of
  `or_else`/`unwrap_or` calls.
- **VS Code defaults fix is part of this plan, not a
  follow-up:** without it, `.editorconfig`'s
  `max_line_length` cannot affect VS Code users — the
  feature would be inert for the primary client. Fixing
  it now keeps the plan's deliverable user-visible.
  Rejected deferring this to a separate plan: the
  precedence rule is the user-visible behavior, and a
  silent inversion of that rule for VS Code users would
  be a Cause-vs-symptom violation.
- **`trim_trailing_whitespace` and `charset` are
  documented as unhonored, not implemented:** the
  formatter always trims and is always UTF-8. Honoring
  these would require fundamental formatter changes
  (preserving trailing whitespace, output encoding
  conversion) without a clear user need. YAGNI.
- **`indent_style = tab` is silently ignored, not
  diagnosed:** matches the existing `insertSpaces: false`
  behavior and avoids diagnostic noise on every YAML
  file in projects whose `.editorconfig` is meant for
  other file types.
- **Four tasks, not three or five:** Task 1
  (infrastructure) is isolated for focused review;
  Task 2 (VS Code) is a self-contained prerequisite
  that can land independently; Task 3 (integration +
  watcher) is one vertical slice because the watcher is
  trivially small once the cache exists; Task 4
  (opt-out + docs) is the user-facing wrap-up. Combining
  Task 1 and Task 3 would produce a single oversized
  commit; splitting Task 3 into integration and watcher
  would produce a non-functional intermediate state
  (integration without watcher means stale cache after
  edits).
- **Code actions also honor `.editorconfig`:** consistent
  with the 2026-04-27 code-action-respect-user-format-
  config plan, which established that code-action text
  edits use the user's formatter settings. `.editorconfig`
  is part of those settings under the new precedence
  rule, so code actions follow.

## Non-Goals

- Honoring `indent_size` or `indent_style` from
  `.editorconfig`. The editor's `tab_size` in each LSP
  format request is the source of truth.
- Honoring `trim_trailing_whitespace`. The formatter
  always trims as a side effect of its line-joining
  logic.
- Honoring `charset`. The formatter is UTF-8 only;
  documented as such.
- Honoring `tab_width` as a distinct setting from
  `indent_size`. `.editorconfig` exposes both; we honor
  neither.
- Reading `.prettierrc`, `.dprint.json`, or any other
  formatter's config files. Out of scope per the
  2026-05-20 disable-switch plan's documented
  non-promise (which this plan partially relaxes only
  for `.editorconfig`).
- Emitting diagnostics for malformed `.editorconfig` files
  or unsupported keys. Logged at `debug` only.
- Workspace-folder–scoped `.editorconfig` discovery.
  Discovery walks up from the file's directory; if no
  ancestor has `.editorconfig`, no settings are applied.
- Per-handler granularity for `formatRespectEditorconfig`
  — the opt-out covers all three formatting handlers
  uniformly.
- Surfacing the active `.editorconfig` path in hover,
  status bar, or any other UI element.
