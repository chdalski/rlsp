**Repository:** root
**Status:** Completed (2026-11-05)
**Created:** 2026-11-05

# VS Code Extension: Override glob and Allowlist the minimatch Advisory

## Goal

The "VS Code Extension" workflow fails in `pnpm run audit` on two moderate advisories. The glob one
(GHSA-3g3m-2fjr-6pw4) is fixed by overriding glob onto major 14. The minimatch one
(GHSA-g3hv-75wg-m249) has a fix only in minimatch 11, which drops `Minimatch.prototype.makeRe`
that the bundled `vscode-languageclient` calls, so it is allowlisted with its reason and revisit
trigger, and the project's audit rule says when such an exception is allowed.

## Context

- Extension paths are under `rlsp-yaml/integrations/vscode/`; the commands are those of the root
  `CLAUDE.md`, VS Code Extension section.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: glob resolves only on its patched major
- [x] Task 2: The minimatch advisory is allowlisted under a stated rule
- [x] `.ai/backlog.md` has a line for the event that ends the minimatch exception, pointing to the
      `Audit dependencies` step's comment
- [x] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: glob resolves only on its patched major

An override lifts glob from 10.5.0 (through mocha) and 13.0.6 (through `@vscode/test-cli` and c8's
test-exclude) onto major 14, and the guard covers the new pin. glob 14 brings its own minimatch 11;
every other consumer keeps minimatch 9 or 10, so the brace-expansion overrides stay.

- [x] The overrides in `package.json` are exactly `serialize-javascript: ^7.1.3`,
      `source-map-js: ^1.2.2`, `brace-expansion@2: ^2.1.7`, `brace-expansion@5: ^5.0.12`, and
      `glob: ^14.0.3`, and `pnpm-lock.yaml` is regenerated from them
- [x] The lockfile resolves glob only on major 14 at 14.0.3 or later, and the minimatch that glob
      depends on at 11.0.2 or later; the handoff lists every package the lockfile adds or drops and
      shows each added package's version and integrity as the registry reports them
- [x] `pnpm run audit` reports no glob advisory
- [x] The guard's `patchedFloors` and `lockfileRules` cover glob (floor 14.0.3, must resolve, major
      14), its `patchedFloors` row names the advisory and why the floor is 14.0.3 rather than the
      advisory's 14.0.2, and the exact overrides test includes the glob pin
- [x] The cases of `isPatchedVersion`, `allPatched`, and `lockfileViolations` cover for glob the
      versions the lockfile resolved before this task, 14.0.2, 14.0.3, a later patch, and a later
      major, and the cross-package traps cover glob against the other packages' floors
- [x] The comments and test titles in `src/overrides.test.ts` describe the guard after this task
- [x] `pnpm run build`, `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, `pnpm run format`,
      and `xvfb-run -a pnpm run test:integration` pass

### Task 2: The minimatch advisory is allowlisted under a stated rule

minimatch is patched only in major 11, which drops `Minimatch.prototype.makeRe`. The runtime
dependency `vscode-languageclient` 11.0.0, its latest release, calls `makeRe` in
`lib/common/fileOperations.js` and `lib/common/utils/globPattern.js`, which esbuild bundles into the
VSIX; the client hands these only globs that a server registers for file operations, pull
diagnostics, or notebooks, and rlsp-yaml registers none. `.github/workflows/vscode-extension.yml`
documents each allowlisted CVE in the comment on its `Audit dependencies` step;
`dtolnay/rust-toolchain` there is pinned to a toolchain version and moves with
`rust-toolchain.toml`, not as a major tag.

- [x] `pnpm.auditConfig.ignoreCves` is exactly `CVE-2026-24001` and `CVE-2026-118764`, and
      `pnpm run audit` passes; the handoff shows that the registry's audit response names
      CVE-2026-118764 for GHSA-g3hv-75wg-m249
- [x] A reader of the `Audit dependencies` step's comment finds why CVE-2026-118764 is allowlisted
      and that the exception ends with a `vscode-languageclient` release that runs with minimatch
      11.0.1 or later, or once rlsp-yaml registers file operations, pull diagnostics with patterns,
      or notebooks
- [x] Every action in `vscode-extension.yml` other than `dtolnay/rust-toolchain` is on its latest
      major tag, and the handoff names the source checked for each action
- [x] In the root `CLAUDE.md`, the `pnpm run audit` line no longer limits exceptions to dev-only
      lows and points to Conventions; a Conventions entry, naming no CVE, allows a CVE in the
      extension's `ignoreCves` only when its vulnerable code is unreachable from runtime or user
      input and either the code is dev-only or its patched release is in a major that a dependent
      cannot run with and that has no release that can, and says that each entry's reason and
      revisit trigger go in the comment on the `Audit dependencies` step of
      `.github/workflows/vscode-extension.yml`
- [x] `pnpm run build`, `pnpm run test`, `pnpm run typecheck`, `pnpm run lint`, and
      `pnpm run format` pass

## Non-Goals

- The Node version comments in `vscode-extension.yml` (on `.ai/backlog.md`)

## Decisions

- **`glob: ^14.0.3`, not `^14.0.2`:** glob 14.0.2 declares `minimatch ^11.0.0`, which admits the
  vulnerable minimatch 11.0.0 that the allowlist would hide; 14.0.3 declares `^11.0.2`. Declined:
  `^14.0.2`.
- **The allowlist reason stands in the audit step's comment of `vscode-extension.yml`,** next to
  CVE-2026-24001, so all exceptions sit in one place. Declined: the header of
  `src/overrides.test.ts`.
- **The audit rule stands in Conventions:** `/project-init` regenerates the command block, not
  Conventions. Declined: rewording the command line only.
- **The guard covers glob only:** minimatch gets no row, since majors 9 and 10 have no patched
  release, and `ignoreCves` gets no test, since it would only repeat the configuration. Declined:
  a test of `ignoreCves`.
- **No minimatch override:** it would break the bundled client. Declined: overriding minimatch
  anyway, a `pnpm patch` of the client, and allowlisting both CVEs.
- **The revisit trigger also gets a backlog line,** since no CI run reports the client release that
  ends the exception. Declined: the workflow comment alone.
- **`lockfileRules` stays a tuple array:** the runtime coverage test already catches a missing glob
  row. Declined: a preparatory task that turns it into a `Record`.
- **Gate demands of Task 2:** the Conventions rule requires unreachability in both of its branches,
  in the wording the user approved; the step comment states that rlsp-yaml registers only fixed
  `didChangeWatchedFiles` globs, which do not reach minimatch, and adds two revisit triggers, a
  minimatch 10.x release carrying the fix and any pattern from settings, workspace, or user input
  flowing into a registration.
