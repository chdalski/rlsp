**Repository:** root
**Status:** Completed (2026-05-29)
**Created:** 2026-05-29

## Goal

Close the three remaining open GitHub Dependabot alerts
(#18 `uuid`, #19 `qs`, #20 `tmp`) against
`rlsp-yaml/integrations/vscode/pnpm-lock.yaml` by moving
each affected transitive dependency to a non-vulnerable
version. The fix must use the lightest available
mechanism: re-resolve the lockfile where the parent
package's existing semver range already permits the
patched version, and add a `pnpm.overrides` entry only
where the chain pins a vulnerable version exactly. The
change must not modify any application code or shipped
extension behavior.

## Context

### Open alerts

From `gh api repos/chdalski/rlsp/dependabot/alerts`:

- **Alert #20 — `tmp@0.2.5`, high severity**
  (GHSA-ph9p-34f9-6g65). Path traversal via unsanitized
  prefix/postfix that enables directory escape.
  Vulnerable range `< 0.2.6`; first patched version
  `0.2.6`.
- **Alert #19 — `qs@6.15.1`, medium severity**
  (GHSA-q8mj-m7cp-5q26). Remotely triggerable DoS:
  `qs.stringify` crashes with `TypeError` on
  `null`/`undefined` entries in comma-format arrays when
  `encodeValuesOnly` is set. Vulnerable range
  `>= 6.11.1, <= 6.15.1`; first patched version `6.15.2`.
- **Alert #18 — `uuid@8.3.2`, medium severity**
  (GHSA-w5hq-g745-h8pq). Missing buffer bounds check in
  v3/v5/v6 when `buf` is provided. Vulnerable range
  `< 11.1.1`; first patched version `11.1.1`.

### Dependency chains (from `pnpm-lock.yaml`)

- `tmp@0.2.5` ← `@vscode/vsce@3.9.1` (devDependency).
- `qs@6.15.1` ← `typed-rest-client@1.8.11` ←
  `@vscode/vsce@3.9.1` (also reached via
  `azure-devops-node-api@12.5.0` → `@vscode/vsce`).
- `uuid@8.3.2` ← `@azure/msal-node@5.1.2` ←
  `@azure/identity@4.13.1` ← `@vscode/vsce@3.9.1`.

### Upstream parent investigation

All three packages were investigated against current
upstream registry metadata before choosing a fix
mechanism:

- **`tmp`**: `@vscode/vsce@3.9.1` (latest) declares
  `tmp: ^0.2.3`. The patched version `0.2.6` is inside
  that caret range, so re-resolving the lockfile picks
  it up. No override is required.
- **`uuid`**: `@azure/msal-node@5.2.2` (latest in the
  `^5.1.0` range that `@azure/identity@4.13.1` declares)
  has dependencies `@azure/msal-common: 16.6.2`,
  `jsonwebtoken: ^9.0.0` only — `uuid` is **not** in its
  dependency tree. Re-resolving the lockfile to advance
  `@azure/msal-node` from `5.1.2` to `5.2.2` removes the
  `uuid@8.3.2` resolution entirely. No override is
  required.
- **`qs`**: `typed-rest-client@1.8.11` (currently
  resolved) pins `qs: 6.15.1` exactly, and the latest
  `typed-rest-client@3.0.0` still pins `qs: 6.15.1`
  exactly. No re-resolution path moves `qs` to `6.15.2`.
  An override is required.

### Why this matters for the fix mechanism

The lighter mechanism (re-resolve only) is preferred when
the upstream chain already permits the patched version,
because it tracks future upstream patch movement
automatically. An override is a forced pin that bypasses
the upstream resolver — useful but heavier-handed, and
correct only when the chain itself is the obstacle. Two
of three alerts here fit the lighter case; one needs the
override.

### Scope

All three packages are devDependencies pulled in by
`@vscode/vsce` (the VS Code Extension Manager, used to
package the `.vsix`). The published extension bundle is
built by `esbuild src/main.ts --external:vscode` and the
package script is `vsce package --no-dependencies`, so
no transitive vsce dep enters the shipped artifact. The
fix has zero runtime impact on extension users; the
blast radius is the local build and CI test pipeline.

### Project precedent

A prior plan
(`.ai/plans/archive/2026-05-13-vscode-dependabot-overrides.md`,
commit `83c17a1f`) established the override pattern by
pinning `fast-uri` and `serialize-javascript` through
`pnpm.overrides`. That plan's verification checklist
(lint, build, vitest, vsce integration tests under
`xvfb-run -a`) is the project-accepted shape for
dependabot remediation in this directory.

### References

- pnpm overrides documentation:
  <https://pnpm.io/package_json#pnpmoverrides>
- pnpm update CLI: <https://pnpm.io/cli/update>
- GHSA-ph9p-34f9-6g65 (tmp path traversal):
  <https://github.com/advisories/GHSA-ph9p-34f9-6g65>
- GHSA-q8mj-m7cp-5q26 (qs DoS):
  <https://github.com/advisories/GHSA-q8mj-m7cp-5q26>
- GHSA-w5hq-g745-h8pq (uuid bounds check):
  <https://github.com/advisories/GHSA-w5hq-g745-h8pq>

## Steps

- [x] Add `qs: ^6.15.2` to the existing `pnpm.overrides`
  block in `rlsp-yaml/integrations/vscode/package.json`
- [x] Run `pnpm update @vscode/vsce @azure/msal-node` (or
  equivalent re-resolve invocation) and follow with
  `pnpm install` to rewrite `pnpm-lock.yaml`
- [x] Verify resolved versions in the lockfile satisfy
  every advisory's `first_patched_version`
- [x] Run lint, format check, build, unit tests, and
  integration tests
- [x] Confirm all three Dependabot alerts will close
  once the change lands on `main`

## Tasks

### Task 1: Patch `tmp`, `qs`, `uuid` resolutions in the vscode pnpm-lock.yaml

**Commit:** `5bd686a4bf8f065aa1253f7dfc1ce055fbad043f`

Apply the smallest change that moves all three packages
to non-vulnerable versions: re-resolve the lockfile to
pull `tmp@>=0.2.6` (in-range under
`@vscode/vsce`'s `^0.2.3`) and to advance
`@azure/msal-node` from `5.1.2` to `5.2.2` (within
`@azure/identity`'s `^5.1.0`), which drops the
`uuid@8.3.2` resolution entirely; and add a single
`pnpm.overrides` entry for `qs` (`^6.15.2`) because
`typed-rest-client` pins `qs@6.15.1` exactly even in
its latest release. The build, unit-test, and
integration-test suites must continue to pass to
confirm that the resolver moves are compatible with
`@vscode/vsce@3.9.1` end-to-end.

Files involved:

- `rlsp-yaml/integrations/vscode/package.json` — add
  one entry to `pnpm.overrides`
- `rlsp-yaml/integrations/vscode/pnpm-lock.yaml` —
  regenerated by `pnpm install`

Sub-tasks (all must be true to pass):

- [x] `pnpm.overrides` in `package.json` contains
  `"qs": "^6.15.2"` alongside the existing `lodash`,
  `fast-uri`, and `serialize-javascript` entries
- [x] `pnpm install` completes without error and updates
  `pnpm-lock.yaml`
- [x] The top-level `overrides:` block in
  `pnpm-lock.yaml` contains the new `qs` entry
- [x] `grep "^  tmp@" pnpm-lock.yaml` returns only
  versions `>= 0.2.6` (resolved to `0.2.7`)
- [x] `grep "^  qs@" pnpm-lock.yaml` returns only
  versions `>= 6.15.2` (resolved to `6.15.2`)
- [x] `grep "^  uuid@" pnpm-lock.yaml` returns no `8.3.2`
  resolution and no resolution in the vulnerable range
  `< 11.1.1` — `uuid` is entirely absent from the
  resolved lockfile (msal-node `5.2.2` dropped it)
- [x] `grep "^  @azure/msal-node@" pnpm-lock.yaml`
  returns version `5.2.2`
- [x] `pnpm run lint` exits zero
- [x] `pnpm run format` exits zero
- [x] `pnpm run build` exits zero
- [x] `pnpm run test` exits zero — 38/38 vitest passing
- [ ] `pnpm run test:integration` exits zero — **not
  met; pre-existing failure verified at baseline SHA
  `97758574` (7 TS2345 errors in `src/config.test.ts`,
  unrelated to this change). User approved landing this
  task with the pre-existing failure unaddressed; a
  follow-up plan repairs `src/config.test.ts`.**
- [x] Security-engineer sign-off: input gate (risk
  assessment of override-vs-re-resolve approach) and
  output gate (per-advisory lockfile verification —
  tmp@0.2.7 ≥ 0.2.6, qs@6.15.2 ≥ 6.15.2, uuid absent
  from chain) both received

## Decisions

- **Re-resolve where possible, override where needed:**
  chose to use `pnpm update` for `tmp` and `uuid`
  because the parent packages already permit the
  patched versions, and to use `pnpm.overrides` only
  for `qs` because `typed-rest-client` pins
  `qs@6.15.1` exactly even in its latest release. This
  keeps the override surface as small as possible while
  still closing every alert; future patch movement in
  vsce and `@azure/msal-node` will be picked up by the
  resolver automatically.
- **No `@vscode/vsce@next` switch:** `@vscode/vsce@3.9.2-3`
  exists on the `next` dist-tag but declares the same
  `typed-rest-client: ^1.8.4` and `tmp: ^0.2.3` ranges
  as 3.9.1, so switching channels would not change any
  of the three vulnerable resolutions. Stay on the
  `latest` tag.
- **Caret range for `qs`:** `^6.15.2` matches the
  `first_patched_version` of advisory #19 and allows
  future patch and minor releases within the 6.x line,
  consistent with the existing `^` ranges throughout
  `package.json` and with the established override
  pattern from commit `83c17a1f`.
- **Verification by lockfile inspection:** the
  acceptance criterion is "resolved versions in the
  lockfile meet the patched-version threshold," not
  "GitHub closes the alert." Alert auto-closure happens
  after merge to the default branch and is outside the
  developer's local control; the lockfile state is what
  determines whether the alert will close.
- **Single task:** all three alerts are remediated by one
  atomic edit-and-reinstall — one override entry plus
  one re-resolve. Splitting into per-package tasks would
  produce three consecutive lockfile rewrites with no
  intermediate value, mirroring the single-task choice
  made in the 2026-05-13 precedent.
- **Unrelated failing dependabot runs:** the
  `serde_json` cargo update run from 2026-05-26 failed
  with a transient GitHub-side
  `Failed to download archive .../dependabot-action/...`
  network error. It is not actionable from this repo
  and is excluded from scope; the next scheduled run
  will retry automatically.

## Non-Goals

- Bumping `@vscode/vsce`, `@azure/identity`,
  `typed-rest-client`, or any other devDependency
  declared in `package.json` to a different range. This
  plan stays within the existing declared ranges;
  parent-range bumps are out of scope.
- Investigating or remediating any Dependabot alert
  outside the three named (`#18`, `#19`, `#20`).
- Touching the Zed extension, the Rust crates, or any
  workspace file outside
  `rlsp-yaml/integrations/vscode/`.
