# Manual Verification — Task 1 (PATH binary)

This procedure is **not automatable in an agent sandbox** — it requires an
interactive Claude Code session with a real terminal, which agents in this
project's pipeline do not have. It must be run and confirmed by a human.
Task 1's acceptance criteria mark this step **user-verified**; nothing in
this repository claims it passed until a user has actually run it.

This is a scratch verification procedure for Task 1, scoped to the
PATH-resolved binary. It does not describe end-user installation — that is
Task 3's `README.md`, which covers the provisioned binary from Task 2 and
the marketplace install path.

## Prerequisites

- Claude Code CLI installed (`claude --version`).
- The `rlsp-yaml` binary built and on `PATH`:

  ```sh
  cargo build --release --package rlsp-yaml
  export PATH="$PWD/target/release:$PATH"
  which rlsp-yaml   # should resolve to the binary just built
  ```

## Steps

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

## Result

- [ ] Confirmed by: _(name / date)_
- [ ] Step 2 passed — no Errors-tab entries
- [ ] Step 3 passed — syntax-error diagnostic reached Claude's context
- [ ] Step 4 passed — hover and go-to-definition both resolved the anchor
