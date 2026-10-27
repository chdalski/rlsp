**Repository:** root
**Status:** NotStarted
**Created:** 2026-10-27

# Fix the Open Dependabot Alerts in the VS Code Extension

## Goal

Two advisories in the VS Code extension's dev dependencies are
open as Dependabot alerts and make the "VS Code Extension" workflow
fail at `pnpm run audit`, which skips building and publishing the
extension: `@azure/msal-node` GHSA-xcx6-jx8p-x3h5 (high, fixed in
7.1.0, reached only through `@vscode/vsce`) and
`serialize-javascript` GHSA-m5mm-p8h7-m29v (low, fixed in 7.1.3,
reached through `@vscode/test-cli` → `mocha`). Remove both from the
dependency tree so the audit and the workflow pass again.

## Context

- `rlsp-yaml/integrations/vscode/package.json`: `@vscode/vsce` is a
  devDependency used only by the `package` script
  (`vsce package --no-dependencies`); `pnpm.overrides` pins
  `serialize-javascript` to `^7.1.2`; `pnpm.auditConfig.ignoreCves`
  allowlists CVE-2026-24001.
- `.github/workflows/vscode-extension.yml` runs
  `pnpx @vscode/vsce package …` and `pnpx @vscode/vsce publish …`.
- The `serialize-javascript` override is what lifts `mocha`'s
  declared `^6.0.2` to major 7.
- `rlsp-yaml/integrations/vscode/src/overrides.test.ts` checks the
  lockfile's `overrides:` block for exact equality with the expected
  overrides and holds a table of each overridden package's patched
  floor and its advisory.
- Advisories: https://github.com/advisories/GHSA-xcx6-jx8p-x3h5,
  https://github.com/advisories/GHSA-m5mm-p8h7-m29v.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [ ] Task 1: the extension's dependency tree is free of both
      advisories
- [ ] Push to `main` and confirm the full CI matrix, the VS Code
      Extension workflow included, is green

## Tasks

### Task 1: Remove both advisories from the extension's dependency tree

Drop the `@vscode/vsce` devDependency, run the local `package`
script through `pnpx @vscode/vsce` like CI, raise the
`serialize-javascript` override to the patched release, and update
the override guard test with it.

- [ ] `package.json` has no `@vscode/vsce` devDependency, its
      `package` script runs `pnpx @vscode/vsce package
      --no-dependencies`, and the `serialize-javascript` override
      is `^7.1.3`
- [ ] `pnpm-lock.yaml` contains no `@vscode/vsce`, `@azure/identity`,
      or `@azure/msal-node` entry, and every `serialize-javascript`
      it resolves is 7.1.3 or higher
- [ ] `pnpm run audit` exits 0, reporting no advisory at low or
      above apart from the allowlisted CVE-2026-24001
- [ ] `overrides.test.ts` expects the `^7.1.3` override, records
      GHSA-m5mm-p8h7-m29v with floor 7.1.3 next to the earlier
      `serialize-javascript` advisory, and treats 7.1.2 as
      vulnerable and 7.1.3 as patched
- [ ] Comments in `overrides.test.ts` match the new graph and floor:
      none cites `@vscode/vsce` as an installed dependency, and the
      `serialize-javascript` floor comment names 7.1.3 and
      GHSA-m5mm-p8h7-m29v as the binding advisory
- [ ] `pnpm run package` produces a `.vsix`
- [ ] `pnpm run build`, `test`, `lint`, `format`, `typecheck`, and
      `xvfb-run -a pnpm run test:integration` pass

## Non-Goals

- Pinning the vsce version used by CI or the local script
- Other dependency updates

## Decisions

- **Remove `@vscode/vsce` instead of forcing `@azure/msal-node`:**
  CI never uses the installed vsce, so dropping it removes the
  vulnerable chain without overriding what `@azure/identity`
  declares; no released `@azure/identity` accepts `@azure/msal-node`
  7.x. Declined: a `^7.1.0` override for `@azure/msal-node`
  (untested against `@azure/identity`), and allowlisting the
  advisory in the audit.
- **Local packaging runs unpinned `pnpx @vscode/vsce`, like CI:** the
  local VSIX is built by the same vsce as the shipped one; a pin in a
  script string is not updated by Dependabot and would drift from
  CI.
- **Override floor `^7.1.3`:** every override floor equals its
  advisory's patched release.
