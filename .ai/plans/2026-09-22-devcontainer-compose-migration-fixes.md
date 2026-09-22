**Repository:** root
**Status:** NotStarted
**Created:** 2026-09-22

## Goal

The devcontainer was moved from a single Dockerfile config to Docker Compose.
That work is in the working tree but not committed yet. A check of the running
container found three problems. The Claude code intelligence plugins are not
installed in a new container. rust-analyzer does not start under the pinned
Rust toolchain. The README and `post-start.sh` state different defaults from
the ones `.env.defaults` actually sets. Fix these three problems and commit
them together with the migration, so that a new container has working Rust and
TypeScript code intelligence without manual steps, and its docs match this
repo's defaults.

## Context

- **Uncommitted migration (the base for this plan).** The working tree holds
  the user's Compose migration in 14 paths under `.devcontainer/`.
  - Modified: `.gitignore`, `Dockerfile`, `README.md`, `devcontainer.json`,
    `post-create.sh`, `post-start.sh`
  - Deleted, and already staged: `init-env`, `init-env.cmd`
  - New and untracked: `.env.credentials.example`, `.env.defaults`,
    `cargo/config.toml`, `docker-compose.audio.yml`, `docker-compose.yml`,
    `fish/config.fish`

  The lead compared it with `HEAD` and checked the running container on
  2026-09-22. Every setting from the old config has a working replacement:
  - `CLAUDE_AUTH=oauth` and `GIT_EMAIL_DOMAIN=chrisski.dev` reach the container
  - the `~/.claude` and `~/.claude.json` host binds are read-only; a write
    fails with "Read-only file system"
  - PulseAudio connects
  - the volumes use the `rlsp_devcontainer_<name>-<id>` names
  - fish history persists
  - Rust binaries are linked by mold 2.30
  - tokens from `.env.credentials` reach new fish shells and Claude's tool shell
  - the `.gitignore` rules work

  The migration's content is the user's work. It changes only where this plan
  says so.
- **Secrets.** `.devcontainer/.env.credentials` exists, holds real tokens and
  is gitignored. It is never read, printed or committed.
  `.claude/settings.local.json` is gitignored as well and is never committed.
- **Plugins are enabled but not installed.** The plugins are enabled only in
  the user settings `~/.claude/settings.json`, which `post-start.sh` copies from
  the host:
  - `rust-analyzer-lsp@claude-plugins-official`
  - `typescript-lsp@claude-plugins-official`
  - `rlsp-yaml@rlsp`

  The committed `.claude/settings.json` has no `enabledPlugins`. The plugin step
  in `post-start.sh` (`setup_plugins`) reads only that project file, and only
  its `@claude-plugins-official` entries, so today it installs nothing and logs
  nothing. `claude plugin list` in the container prints "No plugins installed."
  The old config volume was named after the folder, so plugins installed by
  hand once survived rebuilds. The new volume belongs to one checkout and
  starts empty. The marketplace lists both official plugins. They start the
  servers `rust-analyzer` and `typescript-language-server --stdio`, and
  `post-create.sh` already installs the second one globally with npm.
- **rust-analyzer fails under the pinned toolchain.** `rust-toolchain.toml`
  pins `1.97.1` with the components `clippy` and `rustfmt`, and it is also the
  CI pin, so it must not change here. The Rust devcontainer feature installs
  rust-analyzer for `stable` only. The rustup proxy therefore fails inside
  `/workspace` with `error: Unknown binary 'rust-analyzer' in official toolchain
  '1.97.1-x86_64-unknown-linux-gnu'`. A new container also has no `1.97.1`
  until the first `cargo` or `rustc` call downloads it. In this container the
  lead's check installed it on 2026-09-22. The `post-create.sh` comment
  "rust-analyzer and gopls come with the Rust and Go features" is wrong for this
  repo: no Go feature is configured, and the rust-analyzer the feature installs
  is not the one used in `/workspace`.
- **Defaults stated wrongly.** `.env.defaults` sets `CLAUDE_AUTH=oauth` and
  `GIT_EMAIL_DOMAIN=chrisski.dev`. These are the user's intended values: the old
  `init-env` also wrote `oauth`, and the old `post-start.sh` hardcoded
  `chrisski.dev`. The following still state `proxy` and `codecentric.de`, and
  none of them fails a build, so they are listed here:
  - `README.md`: the Configuration settings table
  - `README.md`, Authentication Modes: the intro sentence, the mode table that
    marks Proxy as the default, the heading "Proxy mode (default)", and the
    "Switching modes" example
  - `post-start.sh`: the header comment and the two fallbacks
    `${CLAUDE_AUTH:=proxy}` and `${GIT_EMAIL_DOMAIN:-codecentric.de}`

  The example `.env` below the settings table sets `CLAUDE_AUTH=oauth`. It is
  meant to show a personal override, but once `oauth` is the stated default it
  overrides nothing.

  The README's migration section also mentions a former `.devcontainer_audio/`
  template. That template never existed in this repo.
- **Checking the changes in the running container.** `post-start.sh` runs
  `main`, which first copies the host's `~/.claude.json` and `settings.json`
  over the container's copies (`init_claude_settings`) and then picks a random
  git identity (`init_git_identity`). Rerunning the whole script while the
  agent team is running would overwrite live session state and change the
  committer identity mid-plan. The checks therefore run the plugin step and the
  new rust-analyzer step without those two functions. Installing the two
  plugins and the rust-analyzer component into this container is expected: that
  is what the fix does.
- **Tooling.** `shellcheck` is not installed. `bash -n` is available, and
  `uvx --from shellcheck-py shellcheck` runs it without installing anything.
- **Not affected.** The Claude Code plugin versioning rule in the root
  `CLAUDE.md` covers only `rlsp-yaml/integrations/claude-code/` and does not
  apply here.

## Steps

- [x] Compare the Compose migration with `HEAD` and check the running container
- [x] Agree the fix direction with the user
- [x] Enable the official code intelligence plugins in the project settings
- [x] Make `post-start.sh` install rust-analyzer for the pinned toolchain
- [x] Change the stated defaults and the fallbacks to this repo's values
- [x] Commit the migration and the fixes together

## Tasks

### Task 1: Code intelligence works in a new container, docs match the real defaults, and the migration is committed

Enable the two official LSP plugins in the committed project settings, so the
existing plugin step installs them. Have every container start install
rust-analyzer for the pinned toolchain. Change the defaults the docs and
fallbacks state to `oauth` and `chrisski.dev`. The result lands as one commit
together with the uncommitted Compose migration.

- [x] `.claude/settings.json` enables `rust-analyzer-lsp@claude-plugins-official`
      and `typescript-lsp@claude-plugins-official`. Its existing `env` and
      `plansDirectory` values are unchanged, and `rlsp-yaml@rlsp` is not added
- [x] Running the plugin step of `post-start.sh` in this container installs
      both plugins. Afterwards `claude plugin list` shows both as installed. A
      second run reports both as already installed and installs nothing. The
      handoff includes the output of both runs
- [x] On every container start, `post-start.sh` makes sure the toolchain that
      is active in `/workspace` (the `rust-toolchain.toml` pin) is installed and
      has the rust-analyzer component. If this fails, for example offline, the
      script prints a warning and the start continues, the same way the plugin
      step handles failures. Afterwards `rust-analyzer --version` succeeds in
      `/workspace`, and the handoff includes that output
- [x] `rust-toolchain.toml` and the CI workflows are unchanged
- [x] Every statement of the `CLAUDE_AUTH` and `GIT_EMAIL_DOMAIN` defaults in
      `.devcontainer/README.md` and `.devcontainer/post-start.sh` says `oauth`
      and `chrisski.dev`, and the fallbacks in `post-start.sh` use those values.
      `codecentric.de` appears nowhere under `.devcontainer/`, and the README
      examples for switching auth modes switch to `proxy`
- [x] In the README, the example `.env` below the settings table sets
      `CLAUDE_AUTH=proxy`, so it shows a real override of the default. Its
      `DEVCONTAINER_CPUS` and `DEVCONTAINER_MEMORY` lines are unchanged
- [x] `post-create.sh`'s comments and the README's plugin and file
      descriptions say correctly where rust-analyzer comes from, and mention
      the new `post-start.sh` step. Neither says that rust-analyzer or gopls
      comes from a devcontainer feature
- [x] The README's migration section no longer mentions a `.devcontainer_audio/`
      template
- [x] `bash -n` passes for `post-start.sh` and `post-create.sh`, and
      `uvx --from shellcheck-py shellcheck` reports no findings on lines this
      task added or changed. The handoff includes both commands and their output
- [x] The commit contains exactly the 14 migration paths (including the
      deletions of `init-env` and `init-env.cmd`), `.claude/settings.json` and
      this plan. `.devcontainer/.env.credentials` and
      `.claude/settings.local.json` are not in it

## Decisions

- **Plugins are enabled in the project settings rather than by making
  `post-start.sh` read the user settings** (user choice). Code intelligence
  plugins are project entries in the blueprint's design (`/project-init` writes
  them to `.claude/settings.json`). The existing plugin step then works as
  intended. `rlsp-yaml@rlsp` stays a manual install: by design, the unattended
  step installs only plugins from the official marketplace.
- **rust-analyzer is installed by `post-start.sh`, not listed in
  `rust-toolchain.toml`** (user choice). This keeps the rust-analyzer download
  out of every CI job. Running on every start means a toolchain bump is picked
  up at the next container restart.
- **The docs use this repo's values** (user choice). `.devcontainer/README.md`
  is written for this repo, not kept as generic template text.
- **One combined commit** (user choice). The migration and its fixes land
  together. The reviewer reviews the whole diff. Findings in migration content
  that this task does not touch go to the lead as notes, not as rejections,
  because that content is the user's and the lead checked it on 2026-09-22.

## Non-Goals

- Installing `rlsp-yaml@rlsp` or other non-official plugins automatically, or
  making `post-start.sh` read the user settings
- Copying data from the old volumes (`claude-code-config-rlsp`,
  `claude-code-bashhistory-*`, `pnpm-store-*`) or deleting them. The user does
  that on the host
- Changing how tokens reach programs that do not run under fish, such as the
  VS Code Claude extension or bash-based tasks
- Changing `rust-toolchain.toml`, CI workflows or the Dockerfile's package list
- Removing template extras such as the `pyright` install, or testing resource
  limits through `.devcontainer/.env`
