# Manual Verification

These procedures are **not automatable in an agent sandbox** — each requires
an interactive Claude Code session with a real terminal, which agents in
this project's pipeline do not have. They must be run and confirmed by a
human. Each task's acceptance criteria mark its user-verified step;
nothing in this repository claims one passed until a user has actually run
it.

These are scratch verification procedures, task-scoped as noted in each
section below. `README.md` documents the install paths themselves
(`--plugin-dir` and marketplace); the Task 3 section here confirms the
marketplace path actually installs and loads the plugin end-to-end.

## Task 1 — PATH binary

Scoped to the PATH-resolved binary from Task 1.

### Prerequisites

- Claude Code CLI installed (`claude --version`).
- The `rlsp-yaml` binary built and on `PATH`:

  ```sh
  cargo build --release --package rlsp-yaml
  export PATH="$PWD/target/release:$PATH"
  which rlsp-yaml   # should resolve to the binary just built
  ```

### Steps

1. From the repository root, start Claude Code with the plugin loaded
   locally:

   ```sh
   claude --plugin-dir rlsp-yaml/integrations/claude-code
   ```

2. Run `/plugin` and open the plugin's detail view (or check the Errors
   tab). **Expected:** the `rlsp-yaml` LSP server is listed with no error
   entries (in particular, no `Executable not found in $PATH`).

3. Open or create a YAML file with a syntax error, e.g.:

   ```yaml
   key: [bad
   ```

   **Expected:** an `rlsp-yaml` diagnostic for the syntax error surfaces
   into Claude's context shortly after the edit (visible in the
   conversation as LSP-sourced diagnostic context, not just a tool-call
   result).

4. Open or create a YAML file using an anchor and alias, e.g.:

   ```yaml
   defaults: &defaults
     retries: 3
   production:
     <<: *defaults
   ```

   Ask Claude to hover over `*defaults` and to go to its definition.
   **Expected:** hover returns information about the anchor, and
   go-to-definition navigates to `&defaults`.

### Result

- [ ] Confirmed by: _(name / date)_
- [ ] Step 2 passed — no Errors-tab entries
- [ ] Step 3 passed — syntax-error diagnostic reached Claude's context
- [ ] Step 4 passed — hover and go-to-definition both resolved the anchor

## Task 2 — Auto-provisioning

Scoped to the `SessionStart` hook (`scripts/provision.sh`) and the
data-dir binary from Task 2, on a machine with **no `rlsp-yaml` on `PATH`**
and an **empty (or nonexistent) persistent data dir**. Confirms the plugin
is usable with neither Rust nor a prebuilt binary already present, and
records the session-start timing gap the plan calls out (Context: no
documented hot-restart, so the LSP may only become active on the *next*
session if the first download is slow).

### Prerequisites

- Claude Code CLI installed (`claude --version`).
- Confirm `rlsp-yaml` is **not** resolvable on `PATH`:

  ```sh
  command -v rlsp-yaml   # expect: nothing / non-zero exit
  ```

- Locate and clear this plugin's persistent data dir so provisioning starts
  from empty. It resolves to `~/.claude/plugins/data/<id>/` (relocatable
  via `CLAUDE_CONFIG_DIR`), where `<id>` is the plugin identifier used at
  install/load time (e.g. `rlsp-yaml` for a local `--plugin-dir` load):

  ```sh
  rm -rf ~/.claude/plugins/data/rlsp-yaml   # adjust <id> if it differs
  ```

- A supported host: Linux (`x86_64`/`aarch64`/`riscv64`) or macOS
  (`x86_64`/`aarch64`). On an unsupported host, skip to step 4 below —
  provisioning is expected to print install guidance instead of a binary.

### Steps

1. From the repository root, start Claude Code with the plugin loaded
   locally:

   ```sh
   claude --plugin-dir rlsp-yaml/integrations/claude-code
   ```

2. **Expected:** the `SessionStart` hook runs `provision.sh`, which
   downloads and verifies the pinned release and installs it to the data
   dir. Confirm from a second terminal (or after the session):

   ```sh
   ls -l ~/.claude/plugins/data/rlsp-yaml/rlsp-yaml   # adjust <id> if it differs
   ```

   **Expected:** the file exists and is executable.

3. Run `/plugin` and open the plugin's detail view (or check the Errors
   tab). **Expected:** the `rlsp-yaml` LSP server is listed with no error
   entries. **Record whether this is true in the same session step 1
   started, or only after restarting/resuming a session** — this is the
   timing gap noted in the plan's Context; either outcome is acceptable,
   but the plan needs the actual observed behavior recorded, not assumed.

4. Open or create a YAML file with a syntax error (same fixture as Task 1):

   ```yaml
   key: [bad
   ```

   **Expected:** an `rlsp-yaml` diagnostic for the syntax error surfaces
   into Claude's context, now served by the auto-provisioned data-dir
   binary rather than a `PATH` binary.

5. *(Only if your host is unsupported, per the Prerequisites note)*
   **Expected instead of steps 2–4:** no binary appears in the data dir;
   `provision.sh`'s guidance (naming the unsupported os/arch and pointing
   to the release page) reaches Claude's context at session start.

### Result

- [ ] Confirmed by: _(name / date)_
- [ ] Step 2 passed — binary present and executable in the data dir
- [ ] Step 3 passed — LSP active with no Errors-tab entries; timing
      recorded: activated in \_\_\_\_ (same session / next session)
- [ ] Step 4 passed — syntax-error diagnostic reached Claude's context via
      the auto-provisioned binary
- [ ] *(if applicable)* Step 5 passed — unsupported-platform guidance
      reached Claude's context, no binary was downloaded

## Task 3 — Marketplace install

Scoped to the repo-root `.claude-plugin/marketplace.json` and the
`/plugin marketplace add` + `/plugin install` path documented in
`rlsp-yaml/integrations/claude-code/README.md`. Confirms the plugin is
installable by a user who has never cloned this repo — the actual
end-user install path, as opposed to the `--plugin-dir`/local-checkout
paths verified in Tasks 1–2.

### Prerequisites

- Claude Code CLI installed (`claude --version`).
- No local `--plugin-dir` session already loading `rlsp-yaml` for this
  repo (so the marketplace-installed copy is unambiguously what's active).

### Steps

1. Start Claude Code without `--plugin-dir`:

   ```sh
   claude
   ```

2. Add this repository as a marketplace and install the plugin:

   ```
   /plugin marketplace add chdalski/rlsp
   /plugin install rlsp-yaml@rlsp
   ```

   **Expected:** both commands succeed with no errors; `/plugin` lists
   `rlsp-yaml` as installed from the `rlsp` marketplace.

3. Restart or resume the session so the newly installed plugin's hooks and
   LSP registration take effect (if not applied automatically).

4. Run `/plugin` and open the plugin's detail view (or check the Errors
   tab). **Expected:** the `rlsp-yaml` LSP server is listed with no error
   entries, and the `SessionStart` provisioning hook has run (per Task 2's
   verification, same expected outcome — PATH-first or auto-download).

5. Open or create a YAML file with a syntax error (same fixture as Tasks 1
   and 2):

   ```yaml
   key: [bad
   ```

   **Expected:** an `rlsp-yaml` diagnostic for the syntax error surfaces
   into Claude's context, confirming the marketplace-installed plugin
   works end-to-end, not just the local `--plugin-dir` copy.

### Result

- [ ] Confirmed by: _(name / date)_
- [ ] Step 2 passed — marketplace add and plugin install both succeeded
- [ ] Step 4 passed — LSP active with no Errors-tab entries
- [ ] Step 5 passed — syntax-error diagnostic reached Claude's context via
      the marketplace-installed plugin
