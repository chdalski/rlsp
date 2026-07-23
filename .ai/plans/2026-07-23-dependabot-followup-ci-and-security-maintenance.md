**Repository:** root
**Status:** InProgress
**Created:** 2026-07-23

# Dependabot Follow-Up: Toolchain Pin, Action Bumps, and npm Advisory Patches

## Goal

Dependabot PR #50 proposed bumping `dtolnay/rust-toolchain`
from `1.97.0` to `1.100.0` — a version of Rust that does not
exist, which would break every Rust CI job. Stop that class
of bad PR permanently, then land the maintenance work the
same review surfaced: bump the Rust toolchain pin to the
newest release that actually exists (`1.97.1`), take the
`actions/setup-node` v6 → v7 major bump, make CI run the
same pnpm version the project declares, and clear the
outstanding npm security advisories in the VS Code
extension's dependency tree.

Five independently committable changes, in dependency
order. Each is verified by a concrete command whose output
is reported in the handoff — not by "CI looks green."

## Context

### What PR #50 actually proposed, and why it is wrong

PR #50 (`build(deps): bump the github-actions group across 1
directory with 2 updates`) is a **grouped** Dependabot PR
carrying two changes that cannot be merged separately:

1. `dtolnay/rust-toolchain` `1.97.0` → `1.100.0` — **invalid**
2. `actions/setup-node` `v6` → `v7` — valid

**Root cause of the invalid bump:** `dtolnay/rust-toolchain`
publishes toolchain versions as *git branches*, not tags, and
pre-creates branches ahead of the actual Rust releases. The
repository currently carries branches `1.97.0`, `1.97.1`,
`1.98.0`, `1.99.0`, and `1.100.0` — all pointing at commits
dated `2026-07-16T16:35:23Z`. Dependabot reads branch names
as version strings and picks the highest-sorting one. It has
no knowledge of whether that Rust release shipped.

Verified against the official Rust distribution channel on
2026-07-23:

| Version | `https://static.rust-lang.org/dist/channel-rust-<v>.toml` |
|---------|------------------------------------------------------------|
| 1.97.0  | 200 |
| 1.97.1  | 200 |
| 1.98.0  | 404 |
| 1.99.0  | 404 |
| 1.100.0 | 404 |

`rustup toolchain install 1.100.0` would therefore fail, and
every job using the action would go red. `rustup check`
independently confirms the newest stable is
`1.97.1 (8bab26f4f 2026-07-14)`, published 2026-07-16.

This will recur on every Dependabot run until the dependency
is ignored in configuration.

### How the Rust toolchain is pinned in this repo

The root `rust-toolchain.toml` (`channel = "1.97.0"`,
`components = ["clippy", "rustfmt"]`, no `targets`) pins
**both** local dev and CI — rustup honors the file over
`dtolnay/rust-toolchain`'s `rustup default`, so CI runs on
the pinned toolchain rather than floating on stable.

**Critical gotcha:** two rustup installs at different
versions are entirely separate, including their installed
cross-compile targets. `dtolnay/rust-toolchain@<X>` installs
each job's targets onto toolchain `X`; if the pin selects a
*different* toolchain, that one lacks the targets and
cross-compile jobs fail with
`error[E0463]: can't find crate for core`. This is why the
workflow action refs must be bumped in lockstep with the
`channel` value, in a single commit — and why the local
`wasm32-wasip2` target must be re-added after installing the
new toolchain locally.

There are **8** action refs to keep in lockstep:
`ci.yml` (2), `coverage.yml` (1), `release-plz.yml` (3),
`vscode-extension.yml` (1), `zed-release.yml` (1).

### `actions/setup-node` v6 → v7 — verified impact

Read directly from the `v7` branch of `actions/setup-node`
(`README.md` and `action.yml`) on 2026-07-23:

- The only v7 change upstream categorizes as breaking is an
  internal migration to ESM, with no change to action inputs
  or behavior. (Inputs are byte-identical v6→v7; two cache
  **outputs** were added additively — `cache-primary-key`,
  `cache-matched-key` — neither used here.)
- The one consumer-visible behavior change is removal of the
  dummy `NODE_AUTH_TOKEN` fallback (upstream files it under
  Bug fixes, not Breaking changes). It affects only steps
  that set `registry-url`. **No `setup-node` step in this
  repository sets `registry-url`**, so it is a no-op at all
  three call sites. Verified at source: `src/main.ts` guards
  the entire auth path behind a non-empty `registry-url`.
- The `node24` runtime (`runs: using: 'node24'`, requiring
  runner ≥ v2.327.1) was introduced in **v5**, not v7 — this
  bump imposes no new runner requirement.
- v6's automatic caching applies only when `packageManager`
  or `devEngines.packageManager` names **npm**. This project
  declares `pnpm@10.33.2` and passes `cache: 'pnpm'`
  explicitly, so no caching behavior changes.

Three refs use it: `coverage.yml:45`,
`vscode-extension.yml:112`, `vscode-extension.yml:165`.

Every other action in the repository is already at its
latest major (`checkout@v7`, `upload-artifact@v7`,
`download-artifact@v8`, `pnpm/action-setup@v6`,
`Swatinem/rust-cache@v2`, `taiki-e/install-action@v2`,
`codecov/codecov-action@v7`,
`MarcoIeni/release-plz-action@v0.5`).

### CI runs pnpm 9 while the project declares pnpm 10.33.2

`rlsp-yaml/integrations/vscode/package.json` declares
`"packageManager": "pnpm@10.33.2"`, and the devcontainer
installs `pnpmVersion: latest` (currently 10.33.2). But all
three `pnpm/action-setup` steps pass `version: 9`, so CI
runs pnpm 9.

Reading `src/install-pnpm/run.ts` on the action's `v6`
branch explains why and constrains the fix:

- `package_json_file` (default `package.json`) is resolved
  relative to `GITHUB_WORKSPACE` — the **repository root**.
  There is no root `package.json`, so the `ENOENT` is
  swallowed and the `version` input wins.
- If `version` is set **and** a `packageManager` field is
  found that differs, the action throws
  `Multiple versions of pnpm specified`. So adding
  `package_json_file` while keeping `version: 9` would break
  CI — `version` must be **removed**, not overridden.
- With `version` omitted, the action self-updates the
  bootstrap pnpm to the `packageManager` version. The
  action's own source comment records why this matters:
  without it, `pnpm store path` reports a different
  `STORE_VERSION` than the real install writes to, "breaking
  `cache: true` and actions/setup-node's `cache: pnpm` on
  cold caches (issue #233)". `coverage.yml` uses
  `cache: 'pnpm'`, so this is a correctness fix, not only
  hygiene.
- The `packageManager` spec permits an appended
  `+<integrity>` hash; the action strips it
  (`.split('+')[0]`) before self-update.

`--frozen-lockfile` is **not** needed and is out of scope:
verified on 2026-07-23 that with a deliberately out-of-sync
lockfile, `CI=true pnpm install` fails with
`ERR_PNPM_OUTDATED_LOCKFILE` ("in CI environments this
setting is true by default"). Bare `pnpm install` in CI is
already frozen.

### Outstanding npm advisories

One **open** GitHub Dependabot alert plus two advisories
Dependabot has not flagged, all confirmed by `pnpm audit` in
`rlsp-yaml/integrations/vscode` on 2026-07-23:

| Package | Advisory | Sev | Vulnerable | Patched | Installed |
|---------|----------|-----|-----------|---------|-----------|
| `brace-expansion` | GHSA-3jxr-9vmj-r5cp | high | `>= 2.0.0, < 2.1.2` | `>= 2.1.2` | 2.1.0 |
| `fast-uri` | GHSA-v2hh-gcrm-f6hx | high | `>= 3.0.0, <= 3.1.3` | `>= 3.1.4` | 3.1.2 |
| `fast-uri` | GHSA-4c8g-83qw-93j6 | high | `>= 3.0.0, < 3.1.3` | `>= 3.1.3` | 3.1.2 |

- `brace-expansion` is **Dependabot alert #37, state `open`**
  — the only open alert on the repository. Reached by two
  paths, verified with `pnpm why`:
  `vscode-languageclient@9.0.1 → minimatch@5.1.9` (a
  **runtime** dependency) and
  `@vscode/test-cli` / `mocha` / `glob@10.5.0 → minimatch@9.0.9`
  (dev). `minimatch@5.1.9` declares `brace-expansion: ^2.0.1`
  and `minimatch@9.0.9` declares `^2.0.2`, so `2.1.2`
  satisfies both without a major bump.
- Both `fast-uri` advisories are **absent from the Dependabot
  alert list** — the two fast-uri alerts on record (#15, #16)
  are different GHSAs, both `fixed`. The existing override
  `"fast-uri": "^3.1.2"`, added for those earlier advisories,
  now permits vulnerable versions. Path:
  `@vscode/vsce → @secretlint/node → @secretlint/config-loader → ajv → fast-uri`
  (dev).
- The `package.json` `pnpm.overrides` block is the
  established mechanism for transitive security pins in this
  project (currently: lodash, fast-uri, serialize-javascript,
  qs, undici, form-data, markdown-it, js-yaml, and the
  version-scoped `brace-expansion@5`).

**Prior scoping note:** the completed plan
`archive/2026-07-08-vscode-brace-expansion-redos-patch.md`
deliberately left the `brace-expansion` 2.x line untouched
and listed it as a Non-Goal, because at that time only the
5.x line was vulnerable and a blanket override would have
major-bumped the runtime path. GHSA-3jxr-9vmj-r5cp is a
**new** advisory that lands on the 2.x line itself. Patching
2.x now does not contradict that decision — the facts
changed. The 2.x fix stays inside the 2.x major
(`2.1.0 → 2.1.2`), so the concern that plan was protecting
against (a cross-major bump of the runtime path) still does
not arise.

**Residual, deliberately excluded:** `diff@7.0.0`
(GHSA-73rr-hh4g-fpgx, low) via `@vscode/test-cli → mocha`.
Its only patched line is `>= 8.0.3`, outside mocha's declared
`diff: ^7.0.0` range. Dev-only, already `auto_dismissed` as
Dependabot alert #4, and excluded by the same reasoning in
the 2026-07-08 plan. See Non-Goals.

### Pre-verified fix for the overrides

The override pair was applied to a throwaway copy of
`package.json` + `pnpm-lock.yaml` in a scratch directory on
2026-07-23 and regenerated with `pnpm install --lockfile-only`:

- `"brace-expansion@2": "^2.1.2"` resolves 2.1.0 → 2.1.2 at
  both lockfile sites.
- `"fast-uri": "^3.1.4"` resolves 3.1.2 → 3.1.4.
- Total lockfile churn: **23 changed lines**, no unrelated
  dependency movement.
- `pnpm audit` afterwards: **1 low** (`diff@7.0.0` only),
  down from 3 high + 1 low.

This is prior evidence that the approach resolves cleanly. It
is **not** a substitute for the developer performing and
reporting the real run — the scratch copy is discarded and
the numbers above must be independently reproduced.

### CI coverage of these changes

- `ci.yml` and `coverage.yml` run on every push to `main`
  and every PR to `main` — they exercise the toolchain pin,
  `setup-node`, and `pnpm/action-setup` changes directly.
- `vscode-extension.yml` runs on push to `main` filtered to
  `rlsp-yaml/integrations/vscode/**`. Task 5 changes files
  under that path, so its commit triggers the 5-target
  cross-compile matrix, which exercises the toolchain and
  Node/pnpm changes from Tasks 2–4 on macOS and Windows too.
- `release-plz.yml` and `zed-release.yml` are release/dispatch
  gated and will **not** run on these commits. Their toolchain
  refs are textually identical to the ones `ci.yml` exercises,
  so a failure to install `1.97.1` would surface in `ci.yml`
  first. The Zed extension's `wasm32-wasip2` check is covered
  by running it locally in Task 2.

### References

- PR #50: https://github.com/chdalski/rlsp/pull/50
- Dependabot alerts: https://github.com/chdalski/rlsp/security/dependabot
- Dependabot configuration options:
  https://docs.github.com/code-security/dependabot/working-with-dependabot/dependabot-options-reference
- `actions/setup-node` v7 release notes:
  https://github.com/actions/setup-node/releases/tag/v7.0.0
- pnpm `packageManager` / Corepack semantics:
  https://nodejs.org/api/corepack.html
- Rust release channel index:
  https://static.rust-lang.org/dist/

## Decisions

- **Ignore `dtolnay/rust-toolchain` in Dependabot rather than
  restricting update types.** The action's version branches
  do not correspond to released Rust versions, so no
  `update-type` filter is safe — `1.98.0` is as fictional as
  `1.100.0`. The pin is a deliberate, manually-managed
  project decision.
- **`ignore`, not group `exclude-patterns`.** An
  `exclude-patterns` entry would only split the dependency
  into its own PR; `ignore` suppresses it entirely, including
  from grouped PRs.
- **Accepted risk, rated Low (security-engineer, Task 1).** A
  bare `ignore` with only `dependency-name` suppresses
  Dependabot's *security*-update auto-PRs for this dependency,
  not just scheduled version-update PRs — GitHub's
  "Controlling which dependencies are updated by Dependabot"
  states `ignore` applies when Dependabot "opens pull requests
  for version updates and security updates." Rated Low because
  a GHSA-driven bump would resolve against the same
  fictional-branch namespace that produced PR #50, so that
  channel was never trustworthy for this action. Detection is
  retained via Dependabot Alerts, which `dependabot.yml` does
  not govern and which are verified enabled on this repository
  (`gh api repos/chdalski/rlsp/dependabot/alerts` returns 30
  alerts, 1 open). Remediation is the manual lockstep bump
  already documented in the config comment.
- **Bump to 1.97.1, not to the highest branch name.** 1.97.1
  is the newest Rust release that exists in the official
  distribution channel.
- **MSRV is unchanged.** `rust-version = "1.97"` in all four
  `Cargo.toml` files remains correct for 1.97.1. No
  `Cargo.toml` is edited by this plan.
- **Remove the `version` input from `pnpm/action-setup`
  rather than setting it to `10.33.2`.** Two hardcoded
  version strings in workflows plus the `packageManager`
  field is three sources of truth that drift independently;
  pointing `package_json_file` at the extension manifest
  leaves exactly one. Keeping `version` alongside
  `package_json_file` is not an option — the action throws.
- **`publish-extension` is pinned, not manifest-driven
  (user-confirmed during Task 4).** That job has no
  `actions/checkout` and runs `permissions: {}` because it
  holds `VSCE_PAT` and only publishes the pre-built VSIX.
  `package_json_file` cannot resolve without the repo tree,
  so removing its `version` would break the job. The two
  alternatives were: (a) pin the step to an explicit version,
  or (b) add a checkout + `contents: read` to the
  secret-bearing job. The user chose (a) — the publish job
  neither builds nor installs from the lockfile, so its pnpm
  version does not affect the published artifact, and adding
  a source checkout to the token-bearing job trades a real
  (if small) security cost for cosmetic consistency. The step
  is aligned to `version: 10` with a comment explaining the
  exception. This narrows the original "all 3 steps"
  acceptance to the 2 checkout-having steps; the plan's
  reproducibility goal is unaffected because only build/test
  jobs consume the lockfile.
- **Five commits, one per concern** (user direction). Each is
  independently revertable.
- **PR #50 is closed unmerged, not merged or rebased** (user
  direction). It is a grouped PR and cannot be partially
  merged; its valid half is reproduced by Task 3.
- **`diff@7.0.0` stays unpatched** (user direction),
  consistent with the same exclusion in the completed
  2026-07-08 brace-expansion plan.
- **`--frozen-lockfile` is not added** — verified already
  default-true under CI.
- **Not a user-facing feature change** — no
  `docs/feature-log.md` entry. Plan file and commit history
  carry the record.
- **Single plan rather than five** (user-confirmed at
  approval, after the plan reviewer raised the separable-
  concerns question). Tasks 4 and 5 are
  directly coupled (the pnpm version in CI must match the
  version that regenerates the lockfile), all five arose from
  one Dependabot review, and Task 5's commit is what triggers
  the cross-compile matrix that validates Tasks 2–4. Splitting
  would land Task 5's lockfile regeneration against a CI that
  still runs a different pnpm major.

## Non-Goals

- **The `diff@7.0.0` advisory** (GHSA-73rr-hh4g-fpgx). It
  remains the single residual `pnpm audit` finding after this
  plan and must not be "fixed" by overriding mocha's declared
  range.
- **Any `version = "..."` edit in any `Cargo.toml`** —
  release-plz owns version progression.
- **Bumping `rust-version` / MSRV** in any crate.
- **Bumping any other GitHub Action.** All others are already
  at their latest major.
- **Upgrading direct npm dependencies** (eslint,
  typescript-eslint, `@vscode/vsce`, mocha, or any other).
  The advisory fix is the `pnpm.overrides` entries alone.
- **Adding `--frozen-lockfile`** to CI install steps.
- **Any Rust source-code change.** If the 1.97.1 clean build
  surfaces new clippy findings, that is a blocker to report,
  not silent scope expansion — see Task 2.
- **Editing completed plan files** in `.ai/plans/` or
  `.ai/plans/archive/`. They are immutable history.
- **`docs/feature-log.md` entries** in any crate.

## Steps

- [x] Add a Dependabot `ignore` rule for
      `dtolnay/rust-toolchain` (Task 1)
- [x] Bump the Rust toolchain pin from 1.97.0 to 1.97.1
      across `rust-toolchain.toml`, all 8 workflow action
      refs, and the toolchain memory note (Task 2)
- [x] Bump `actions/setup-node` from v6 to v7 in all 3 refs
      (Task 3)
- [x] Make CI resolve pnpm from the extension's
      `packageManager` field instead of a hardcoded
      `version: 9` (Task 4)
- [ ] Patch the `brace-expansion` and `fast-uri` advisories
      via `pnpm.overrides` and regenerate the lockfile
      (Task 5)
- [ ] Lead: close PR #50 unmerged once Tasks 1–5 are on
      `main`

## Tasks

### Task 1 — Ignore `dtolnay/rust-toolchain` in Dependabot

Stop Dependabot from ever again proposing a
`dtolnay/rust-toolchain` version that does not correspond to
a released Rust toolchain. The pin is manually managed in
lockstep with `rust-toolchain.toml`, so automated bumps of
this one action are never wanted.

- [x] In `.github/dependabot.yml`, add an `ignore` list to
      the `github-actions` update entry containing a single
      `dependency-name: "dtolnay/rust-toolchain"` element.
      Leave the existing `groups`/`patterns` block unchanged.
- [x] Add a comment above the `ignore` list stating (a) that
      the action publishes version branches ahead of the
      actual Rust releases, so the highest branch name is not
      necessarily a released toolchain, and (b) that the pin
      is bumped manually together with `rust-toolchain.toml`
      and all 8 workflow refs.
- [x] Confirm the file is valid YAML and the `ignore` key
      sits at the same nesting level as `schedule` and
      `groups` within the `github-actions` entry — quote the
      resulting entry verbatim in the handoff.
- [x] Confirm the `cargo` update entry is byte-for-byte
      unchanged.

**Acceptance:** `.github/dependabot.yml` parses as valid
YAML; the `github-actions` entry contains
`ignore: [{dependency-name: "dtolnay/rust-toolchain"}]` with
the explanatory comment; the `cargo` entry and the
`github-actions` `groups` block are unchanged. The handoff
quotes the full `github-actions` entry.

**Files:** `.github/dependabot.yml`

**Advisors:** security-engineer. Risk category: supply-chain
— this deliberately disables automated dependency updates,
including security updates, for a third-party action that
executes in CI. Input gate: a risk assessment covering what
exposure is accepted by ignoring this action and what
compensating control (if any) should accompany it. Output
gate: sign-off that the committed rule scopes exactly to
`dtolnay/rust-toolchain` and suppresses nothing else.
Test-engineer not required: no production or test code
changes, and the artifact is a declarative config file with
no test harness — verification is YAML validity and textual
inspection.

### Task 2 — Bump the Rust toolchain pin to 1.97.1

Move the project from Rust 1.97.0 to 1.97.1, the newest
release present in the official distribution channel. The
`channel` value in `rust-toolchain.toml` and all 8
`dtolnay/rust-toolchain@<version>` workflow refs must move
together in this one commit — a mismatch makes CI's
cross-compile jobs install targets onto a toolchain that
cargo does not select, which fails with
`error[E0463]: can't find crate for core`.

- [x] Run `rustup toolchain install 1.97.1`.
- [x] In `rust-toolchain.toml`, set `channel = "1.97.1"` and
      update all three `1.97.0` mentions in the header
      comment to `1.97.1` — line 2 carries two (a prose
      mention and an inline `@1.97.0` example) and line 5
      carries one. Leave `components` unchanged and do not
      add a `targets` key. The binding criterion is that no
      stale `1.97.0` text remains in the file.
- [x] Run `rustup target add wasm32-wasip2` — the newly
      installed toolchain is a separate rustup install and
      does not inherit the target list from 1.97.0.
- [x] Update all 8 `dtolnay/rust-toolchain@1.97.0` refs to
      `@1.97.1`: `.github/workflows/ci.yml` (lines 21, 36),
      `.github/workflows/coverage.yml` (line 21),
      `.github/workflows/release-plz.yml` (lines 19, 39,
      142), `.github/workflows/vscode-extension.yml`
      (line 79), `.github/workflows/zed-release.yml`
      (line 26). Confirm by grep that zero `@1.97.0` refs
      remain under `.github/`.
- [x] Update `.ai/memory/project_rust_toolchain_pin_ci_behavior.md`.
      The file contains exactly 5 literal `1.97.0`
      occurrences, at lines 9, 12, 15, 18, and 23. Change
      **4** of them to 1.97.1: line 9 (the
      `channel = "1.97.0"` example), lines 15 and 18 (both in
      the "Critical gotcha" paragraph), and line 23 (the
      `dtolnay/rust-toolchain@1.97.0` ref in the "Fix in use
      (full-pin)" paragraph). The "To bump the toolchain"
      paragraph needs **no** edit — it already uses a
      version-agnostic `@<ver>` placeholder. Do not change
      the note's structure or its description of the pinning
      strategy.
- [x] **Leave the 5th occurrence — line 12 — unchanged.** The
      text
      `(CI log: "1.97.0 ... overridden by rust-toolchain.toml")`
      is a verbatim quotation from a real 1.97.0-era CI run,
      not a statement about the currently pinned version.
      Rewriting it to 1.97.1 would fabricate a quote. State
      in the handoff that this occurrence was deliberately
      left at 1.97.0 and why. After editing, the file must
      contain exactly one remaining `1.97.0` occurrence.
- [x] Run `cargo clean`, then `cargo fmt --all -- --check`,
      then `cargo clippy --workspace --all-targets -- -D warnings`,
      then `cargo test --workspace`. The `cargo clean` is
      mandatory and must precede clippy — an incremental
      clippy cache does not re-lint unchanged code after a
      toolchain change and silently under-reports new lints.
- [x] Run
      `cargo check --manifest-path rlsp-yaml/integrations/zed/Cargo.toml --target wasm32-wasip2`
      and
      `cargo clippy --manifest-path rlsp-yaml/integrations/zed/Cargo.toml --all-targets --target wasm32-wasip2 -- -D warnings`.
      These cover `zed-release.yml`, which is dispatch-gated
      and will not run in CI on this commit.
- [x] Confirm `rustc --version` reports 1.97.1 from within
      the repository (the pin is active) and quote it in the
      handoff.
- [x] Confirm no `Cargo.toml` was modified — `rust-version`
      stays `"1.97"` in all four crates.

**Acceptance:** `rust-toolchain.toml` reads
`channel = "1.97.1"` with no stale `1.97.0` text; zero
`dtolnay/rust-toolchain@1.97.0` refs remain under
`.github/`; every descriptive version reference in
`.ai/memory/project_rust_toolchain_pin_ci_behavior.md` names
1.97.1, and the quoted CI log excerpt in that note still
reads `1.97.0`. All six commands
above pass, with `cargo clippy --workspace --all-targets -- -D warnings`
reporting **zero warnings** on a build that followed
`cargo clean`. The handoff reports each command and its
actual result, states that `cargo clean` was run before
clippy, and confirms no `Cargo.toml` changed.

**Blocker condition:** if the clean 1.97.1 clippy run reports
any finding, do **not** fix the code and do **not** suppress
the lint. Stop, and report the full finding list to the lead
— Rust source changes are outside this plan's scope and need
their own plan.

**Files:**
- `rust-toolchain.toml`
- `.github/workflows/ci.yml`
- `.github/workflows/coverage.yml`
- `.github/workflows/release-plz.yml`
- `.github/workflows/vscode-extension.yml`
- `.github/workflows/zed-release.yml`
- `.ai/memory/project_rust_toolchain_pin_ci_behavior.md`

**Advisors:** none. Low risk and low uncertainty: a patch
bump within an unchanged, already-established pinning
strategy, with no trust-boundary, secret, or input-handling
surface. No production or test code changes, and every
verification step is an existing project quality gate with
an objective pass/fail — there is no test design to
specify.

### Task 3 — Bump `actions/setup-node` from v6 to v7

Take the valid half of Dependabot PR #50. The v7 major
contains only an internal ESM migration plus removal of the
dummy `NODE_AUTH_TOKEN` fallback; the latter affects only
steps that set `registry-url`, and no step in this
repository does.

- [x] Update all 3 `actions/setup-node@v6` refs to `@v7`:
      `.github/workflows/coverage.yml` (line 45),
      `.github/workflows/vscode-extension.yml` (lines 112,
      165).
- [x] Confirm by grep that zero `actions/setup-node@v6` refs
      remain under `.github/`.
- [x] Confirm no `setup-node` step in the repository sets a
      `registry-url` input, and state this explicitly in the
      handoff — it is the condition under which v7's only
      breaking change applies.
- [x] Leave every `with:` block on those steps otherwise
      unchanged, including `node-version: '22'`,
      `cache: 'pnpm'`, and `cache-dependency-path` in
      `coverage.yml`.

**Acceptance:** zero `actions/setup-node@v6` refs remain
under `.github/`; exactly 3 `@v7` refs exist; no `with:` key
on any of the 3 steps was added, removed, or changed. The
handoff states the verified absence of `registry-url` on all
`setup-node` steps.

**Files:**
- `.github/workflows/coverage.yml`
- `.github/workflows/vscode-extension.yml`

**Advisors:** security-engineer. Risk category: secrets
handling — v7's breaking change concerns credential material
written into `.npmrc`, and two of the three updated steps sit
in `vscode-extension.yml`, whose `publish-extension` job
carries the `VSCE_PAT` marketplace-publishing secret. Input
gate: a risk assessment on whether the v7 auth-token change
alters credential handling anywhere in these workflows.
Output gate: sign-off on the committed diff. Test-engineer
not required: no production or test code changes; the change
is a version-string edit verified by grep and by the existing
CI jobs.

### Task 4 — Resolve CI's pnpm version from `packageManager`

CI currently runs pnpm 9 while the project declares
`pnpm@10.33.2`, because `pnpm/action-setup` looks for
`package.json` at the repository root — where none exists —
and falls back to the hardcoded `version: 9` input. Point
the action at the extension's manifest and drop the input so
`packageManager` is the single source of truth. Beyond
removing the drift, this makes the action self-update the
bootstrap pnpm so `pnpm store path` reports the store version
the real install actually writes to, which is what
`coverage.yml`'s `cache: 'pnpm'` depends on for cold caches.

**Scope correction (applied at execution).** The plan
originally directed making all 3 `pnpm/action-setup` steps
manifest-driven. During the security input gate, the
developer and advisor found that the third step —
`vscode-extension.yml`'s `publish-extension` job — has **no
`actions/checkout`** and runs with `permissions: {}` (it holds
`VSCE_PAT` and only publishes the pre-built VSIX). With no
checkout, `package_json_file` resolves against a
`GITHUB_WORKSPACE` that lacks the repo tree; the action
swallows the `ENOENT` and, with `version` removed, throws
`No pnpm version is specified` — breaking the publish job.
The user chose to pin that step rather than add a checkout to
the secret-bearing job (see Decisions). Only the two
checkout-having jobs become manifest-driven.

- [x] In `.github/workflows/coverage.yml` (the
      `pnpm/action-setup` step, `coverage-vscode` job, which
      checks out the repo), remove the `version: 9` input and
      add `package_json_file: rlsp-yaml/integrations/vscode/package.json`.
- [x] In `.github/workflows/vscode-extension.yml`
      `build-extension` step (checks out at line 77), make the
      same change. Keep `run_install: false`.
- [x] In `.github/workflows/vscode-extension.yml`
      `publish-extension` step (no checkout), do **not** remove
      `version`: change `version: 9` → `version: 10` (align the
      major to the declared `pnpm@10.33.2`), keep
      `run_install: false`, and add a comment explaining the
      step is pinned because the job has no checkout by design
      and its pnpm version does not affect the published VSIX.
      Do **not** add a checkout or change `permissions: {}`.
- [x] Confirm the `version` input is removed (not merely
      changed) on the two manifest-driven steps — leaving it
      alongside `package_json_file` makes the action throw
      `Multiple versions of pnpm specified`.
- [x] Confirm by grep: exactly 2 `pnpm/action-setup` steps
      under `.github/` carry `package_json_file` with no
      `version:`; exactly 1 retains `version: 10`; zero remain
      at `version: 9`.
- [x] Confirm the `package_json_file` value is written
      relative to the repository root, not to any job
      `working-directory` — the action resolves it against
      `GITHUB_WORKSPACE`.
- [x] Confirm `"packageManager": "pnpm@10.33.2"` is present
      and unmodified in
      `rlsp-yaml/integrations/vscode/package.json`.
- [x] Confirm the step ordering in `coverage.yml` is
      unchanged, with `pnpm/action-setup` still before
      `actions/setup-node` — `setup-node`'s `cache: 'pnpm'`
      needs pnpm on `PATH` to compute the store path.

**Acceptance:** the 2 checkout-having `pnpm/action-setup`
steps (`coverage.yml` coverage-vscode, `vscode-extension.yml`
build-extension) have no `version:` input and carry
`package_json_file: rlsp-yaml/integrations/vscode/package.json`;
the `publish-extension` step retains `version: 10` with an
explanatory comment and its `permissions: {}` is unchanged
(no checkout added); zero steps remain at `version: 9`;
`run_install: false` is retained on the two
`vscode-extension.yml` steps; `coverage.yml` step order is
unchanged; `package.json` is not modified by this task.

**Files:**
- `.github/workflows/coverage.yml`
- `.github/workflows/vscode-extension.yml`

**Advisors:** security-engineer. Risk category: supply chain
— this moves control of which package-manager binary executes
in CI from the workflow file into a package manifest, and one
affected workflow holds the `VSCE_PAT` publishing secret. The
`packageManager` field supports an optional `+<integrity>`
hash which `pnpm/action-setup` strips before self-update.
Input gate: a risk assessment on that authority shift, and
specifically on whether the `packageManager` value should
carry an integrity hash given the action's stripping
behavior. Output gate: sign-off on the committed diff.
Test-engineer not required: no production or test code
changes; verification is textual plus the existing CI jobs.

### Task 5 — Patch the `brace-expansion` and `fast-uri` advisories

Clear the repository's only open Dependabot alert
(`brace-expansion`, GHSA-3jxr-9vmj-r5cp, high, alert #37)
and two high-severity `fast-uri` advisories that Dependabot
has not flagged but `pnpm audit` reports, by adding a
version-scoped `brace-expansion` 2.x override and tightening
the existing `fast-uri` override, then regenerating the
lockfile. This runs last so the lockfile is regenerated by
the same pnpm version CI now uses.

- [ ] In `rlsp-yaml/integrations/vscode/package.json`, add
      `"brace-expansion@2": "^2.1.2"` to the existing
      `pnpm.overrides` block, alongside the existing
      `"brace-expansion@5": "^5.0.6"` entry. Do not replace
      or widen the `@5` entry.
- [ ] In the same block, change `"fast-uri": "^3.1.2"` to
      `"fast-uri": "^3.1.4"`.
- [ ] Run `pnpm install` in
      `rlsp-yaml/integrations/vscode` to regenerate
      `pnpm-lock.yaml`.
- [ ] Verify every `brace-expansion` 2.x occurrence in the
      lockfile is `2.1.2` and none remains at `2.1.0`.
- [ ] Verify every `brace-expansion` 5.x occurrence is
      unchanged at `5.0.7`.
- [ ] Verify every `fast-uri` occurrence is `3.1.4` and none
      remains at `3.1.2`.
- [ ] Run `pnpm audit` and report the full result. The only
      remaining finding must be `diff@7.0.0`
      (GHSA-73rr-hh4g-fpgx, low). Zero `brace-expansion` and
      zero `fast-uri` findings must remain.
- [ ] Inspect the `pnpm-lock.yaml` diff and confirm it
      contains only the `brace-expansion` 2.x and `fast-uri`
      version movements plus the two override lines — no
      other package changes version, and nothing is
      downgraded. Report the diff line count.
- [ ] Run the extension quality gates and confirm each
      passes: `pnpm run build`, `pnpm run lint`,
      `pnpm run format`, `pnpm run test`.
- [ ] Run `xvfb-run -a pnpm run test:integration` and confirm
      it passes — `brace-expansion` sits on the runtime
      `vscode-languageclient` glob path, so the VS Code
      integration tests are the gate that exercises it.
- [ ] Confirm no scratch or temporary files remain in the
      working tree before submitting for review.

**Acceptance:** `pnpm.overrides` contains both
`"brace-expansion@2": "^2.1.2"` and `"fast-uri": "^3.1.4"`,
with `"brace-expansion@5": "^5.0.6"` retained unchanged. In
the regenerated lockfile: zero `brace-expansion@2.1.0`, zero
`fast-uri@3.1.2`, `brace-expansion` 5.x still `5.0.7`.
`pnpm audit` reports exactly one finding — `diff@7.0.0`, low
— and zero findings for `brace-expansion` and `fast-uri`. All
five gates pass (`build`, `lint`, `format`, `test`,
`test:integration`). The handoff reports the `pnpm audit`
output verbatim, the resolved versions, the lockfile diff
line count, and the vitest and integration-test pass counts.

**Files:**
- `rlsp-yaml/integrations/vscode/package.json`
- `rlsp-yaml/integrations/vscode/pnpm-lock.yaml`

**Advisors:** security-engineer **and** test-engineer — four
independent gates.

- security-engineer input gate: a risk assessment on whether
  `^2.1.2` and `^3.1.4` fully cover the vulnerable ranges
  `>= 2.0.0, < 2.1.2` and `>= 3.0.0, <= 3.1.3`, and whether
  scoping the `brace-expansion` override to the `@2` major
  leaves any vulnerable resolution unpatched. Output gate:
  sign-off that both advisories are remediated in the
  regenerated lockfile with no residual exposure.
- test-engineer input gate: a test list. This is the first
  time a `pnpm.overrides` entry moves a package on the
  extension's **runtime** dependency path
  (`vscode-languageclient@9.0.1 → minimatch@5.1.9 → brace-expansion`),
  where `brace-expansion` drives glob expansion for document
  selectors and file watchers. The question is whether the
  existing vitest and VS Code integration suites actually
  exercise that path, and what to assert if they do not.
  Output gate: sign-off that the executed test set matches
  the test list.

## Verification

After all five tasks land on `main`:

1. GitHub Dependabot alert #37 (`brace-expansion`,
   GHSA-3jxr-9vmj-r5cp) reads state `fixed`, and the
   repository's Dependabot alert list contains zero alerts in
   state `open`. Dependabot re-scans the default branch on
   push rather than instantly, so check this after the
   post-merge scan completes; a still-`open` alert before
   that scan is latency, not failure. Check with
   `gh api repos/chdalski/rlsp/dependabot/alerts --jq '[.[] | select(.state=="open")] | length'`,
   which must return `0`.
2. `ci.yml` and `coverage.yml` pass on the pushes, exercising
   the 1.97.1 pin, `setup-node@v7`, and the `packageManager`-
   driven pnpm resolution.
3. `vscode-extension.yml` is triggered by Task 5's path and
   passes on all 5 build targets, exercising the toolchain
   and Node/pnpm changes on Linux, macOS, and Windows.
4. The next weekly Dependabot `github-actions` run produces
   no `dtolnay/rust-toolchain` PR.
5. Lead closes PR #50 unmerged with a comment recording that
   its `setup-node` half landed separately and its
   `dtolnay/rust-toolchain` half proposed a nonexistent Rust
   version.
