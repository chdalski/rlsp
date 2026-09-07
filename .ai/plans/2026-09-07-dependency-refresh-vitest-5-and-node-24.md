**Repository:** root
**Status:** InProgress
**Created:** 2026-09-07

# Land the pending dependency refresh, migrate to Vitest 5, move CI to Node 24

## Goal

The user refreshed dependencies out-of-band (`cargo update` for the Rust
workspace and the Zed extension, `pnpm update` for the VS Code extension) and
wants the remaining gaps closed: the refreshed lockfiles landed, the packages
`pnpm outdated` still reports addressed, the open GitHub issues reviewed, and
the four open Dependabot security alerts resolved. The issues review is already
done and produced no work — the queue is empty (see Context) — so this plan
carries the other three. Landing the refreshed `pnpm-lock.yaml` is what actually closes
those alerts, and the same task must tighten the lockfile regression guard that
is supposed to catch a future regression into the advisory range — it currently
would not. Vitest 5 is then the only remaining upgradeable package, and CI moves
to Node 24 so the runtime is a supported LTS with headroom over Vitest 5's
floor.

## Context

- **The pending working-tree changes are already verified green.** Four files
  are modified but uncommitted: root `Cargo.lock`, `rlsp-yaml/integrations/zed/Cargo.lock`,
  and the VS Code extension's `package.json` + `pnpm-lock.yaml`. Every build and
  test command listed in the root `CLAUDE.md` and the extension `CLAUDE.md` was
  run against this tree on 2026-09-07 and passed:

  | Gate | Result |
  |------|--------|
  | `pnpm run typecheck` (`tsc --noEmit`) | clean |
  | `pnpm run lint` (`eslint src/`) | clean |
  | `pnpm run format` (`prettier --check`) | clean |
  | `pnpm run test` (`vitest run`) | 49 passed, 5 files |
  | `pnpm run test:integration` (under `xvfb-run -a`) | 19 passed, 1 pending |
  | `pnpm run build` (esbuild) | `out/main.js`, 968.7 kb |
  | `pnpm run audit` | 1 low, the allowlisted `CVE-2026-24001` |
  | `cargo fmt --all --check` | clean |
  | `cargo clippy --all-targets` | clean |
  | `cargo build` | clean |
  | `cargo test --workspace` | 6300 passed, 0 failed |
  | Zed `cargo check` / `cargo clippy` (`--target wasm32-wasip2`) | clean |
  | `cargo test -p rlsp-yaml --test claude_code_stdio_smoke` | 2 passed |
  | `claude plugin validate --strict` (plugin + marketplace) | both passed |

  The single pending integration test is the pre-existing conditional skip
  "activate() resolves and starts the client when the server binary is present",
  which is skipped because no built server binary is on the path — it is not a
  regression and not something this plan changes. These are baseline figures for
  the current toolchain and dependency set; Task 4 changes the JavaScript test
  runner, so the 49-test figure is the number to compare against after that
  upgrade. The Zed wasm gates matter here specifically because
  `rlsp-yaml/integrations/zed/Cargo.lock` is one of the four refreshed files.
  The `package.json` diff is three dependency
  range bumps (`@types/node` `^26.4.0`→`^26.4.1`, `@types/vscode` `^1.134.0`→`^1.136.0`,
  `eslint` `^10.9.1`→`^10.10.0`) plus a cosmetic reformat of the
  `pnpm.auditConfig.ignoreCves` array from one line to multi-line.

- **Those four files exist only in the working tree and are not recoverable if
  discarded.** They are the product of the user's out-of-band `cargo update` and
  `pnpm update` runs, with no commit or stash behind them. Any destructive git
  operation on the working tree before Task 1 commits them loses that dependency
  resolution work. They must be WIP-committed early and must appear in Task 1's
  commit; re-running the update commands is not an equivalent recovery, because
  registry state moves and would produce a different resolution than the one
  verified green here. Task 2 and Task 4 modify `package.json` and `pnpm-lock.yaml` again
  on top of this baseline.

- **The open GitHub issues queue was checked and is empty.** The user's request
  covered three things: the packages `pnpm outdated` still reports, the open
  GitHub issues, and the Dependabot security alerts. `gh issue list --state open`
  returned zero issues on 2026-09-07, so that third of the request needs no task
  — it was investigated and came back empty, not dropped. Open *pull requests*
  are a separate matter and are addressed under Non-Goals.

- **The four Dependabot alerts are all `fast-uri`, all high, all dev-scope.**
  Alerts #52, #53, #56, #57 — `GHSA-5jgf-p345-68v8` / `CVE-2026-75931`,
  `GHSA-fph4-wmhf-6fwf` / `CVE-2026-75899`, `GHSA-f65p-4m7j-42xc` / `CVE-2026-75975`,
  `GHSA-jqff-g426-hqxp` / `CVE-2026-76172`. They cover host confusion and SSRF
  via URI normalization. `fast-uri` is reached only through
  `@vscode/vsce → secretlint → ajv` (and `table`), so it is build/publish
  tooling, not extension runtime code.

  **Each advisory spans three vulnerable ranges, not one.** An earlier revision
  of this plan said only "every one is first patched in 3.1.6", which is
  incomplete and was the basis for a control that would have missed the 4.x
  line. Verified against `gh api repos/:owner/:repo/dependabot/alerts` on
  2026-09-07:

  | Advisory | 2.x vulnerable | 3.x vulnerable | 4.x vulnerable |
  |----------|----------------|----------------|----------------|
  | `GHSA-5jgf-p345-68v8` | `>= 2.4.2, < 2.4.5` | `>= 3.1.3, < 3.1.6` | `>= 4.0.1, < 4.1.3` |
  | `GHSA-f65p-4m7j-42xc` | `>= 2.3.1, < 2.4.5` | `>= 3.0.0, < 3.1.6` | `>= 4.0.0, < 4.1.3` |
  | `GHSA-fph4-wmhf-6fwf` | `>= 2.4.1, < 2.4.5` | `>= 3.1.2, < 3.1.6` | `>= 4.0.0, < 4.1.3` |
  | `GHSA-jqff-g426-hqxp` | `>= 2.3.1, < 2.4.5` | `>= 3.0.0, < 3.1.6` | `>= 4.0.0, < 4.1.3` |

  The patched floors are therefore `2.4.5`, `3.1.6`, and `4.1.3` — one per major
  line. The committed lockfile resolves `3.1.5`; the pending lockfile resolves
  `3.1.7`. Pushing the pending lockfile to `main` is what closes all four.

- **The lockfile regression guard's `fast-uri` floor is stale and currently
  unsound.** `src/overrides.test.ts` asserts every resolved `fast-uri` version
  is `>= 3.1.5`. That floor predates these four advisories, whose vulnerable
  ranges are `>= 3.0.0, < 3.1.6` and `>= 3.1.2, < 3.1.6`. A future dependency
  bump that resolved `fast-uri` back to exactly `3.1.5` would satisfy the guard
  while being vulnerable to all four — precisely the regression the guard exists
  to catch. The file's header comment also names only the two older `fast-uri`
  advisories (`GHSA-v2hh-gcrm-f6hx` / `GHSA-4c8g-83qw-93j6`). The guard reads the
  lockfile as text and asserts on resolved versions rather than on the presence
  of an override string; the `brace-expansion` assertions in the same file are
  unaffected by this work.

- **VSIX packaging is broken on `main`, and the two CI failures had different
  causes.** Verified per-run rather than batch-categorized:

  | Run | `@types/vscode` | `engines.vscode` | Failing step |
  |-----|-----------------|------------------|--------------|
  | `03482b21` (2026-08-10) | `^1.125.0` | `^1.125.0` | none — success |
  | `762522a3` (2026-08-14) | `^1.125.0` | `^1.125.0` | **Audit dependencies** — the `fast-uri` advisory; fixed by Task 1 |
  | `6e65f730` (2026-09-07) | `^1.134.0` | `^1.125.0` | **Package VSIX** — types exceed engine |
  | `b0a93ef5` (Task 1) | `^1.136.0` | `^1.125.0` | **Package VSIX** — same cause |

  The packaging break was introduced by Dependabot PR #65, not by Task 1; Task 1
  raised the types further but did not cause the failure mode. The audit failure
  is already resolved.

  `engines.vscode` is mandatory — `@vscode/vsce/out/package.js` throws
  `Manifest missing field: engines` without it — and is not redundant with the
  types: it is the runtime compatibility promise the Marketplace uses to gate
  installs, while `@types/vscode` is the compile-time API ceiling. `vsce`
  enforces `types <= engines` so code cannot compile against an API the declared
  minimum runtime lacks. The check lives in
  `validateVSCodeTypesCompatibility` and runs *only* when `@types/vscode` is
  present in `devDependencies`, *only* during `vsce package`, and compares
  major.minor only — which is exactly why divergence reaches `main` unnoticed.

- **Vitest 5 is upgradeable; TypeScript 7 is not.** `vitest` and
  `@vitest/coverage-v8` are at `4.1.11` with `5.0.0` available, and its peers are
  already satisfied — `vite@8.2.2` (needs `^6.4.0 || ^7 || ^8`) and
  `@types/node@26` (needs `^22 || >=24`). `typescript` is at `6.0.3` with `7.0.2`
  available but stays put: `typescript-eslint@8.69.0` (current latest) declares
  the peer range `typescript: >=4.8.4 <6.1.0`, and lint is a required gate.
  `6.0.3` is the newest 6.x release. `.github/dependabot.yml` already carries an
  ignore rule for the `typescript` major, so nothing leaks through — no change
  is needed there.

- **Vitest 5 breaking changes that touch this project.** Minimum Node is
  `^22.12.0 || ^24 || >=26` and Vite `>=6.4`. Mocks are now cleared by default
  before each test. Unawaited async assertions now fail the test. The v8
  coverage provider switched to AST-based remapping as the only supported mode
  (`coverage.ignoreEmptyLines` and `coverage.experimentalAstAwareRemapping` are
  removed), so reported coverage numbers shift. Config lookup no longer searches
  ancestor directories. JSON/JUnit reporter output moved to `.vitest/`. Several
  subpath entry points were removed, and `@vitest/expect` / `@vitest/runner` are
  now inlined into `vitest`.

- **Existing test and coverage surface.** `vitest.config.mts` is 12 lines:
  `environment: 'node'`, `include: ['src/**/*.test.ts']`,
  `exclude: ['src/test/integration/**']`, and coverage with `provider: 'v8'`,
  `reporter: ['lcov', 'text']`, `reportsDirectory: './coverage'`. It uses no
  removed option and no negation globs. The five test files are
  `commands.test.ts`, `config.test.ts`, `overrides.test.ts`, `server.test.ts`,
  and `status.test.ts`; `commands.test.ts` and `config.test.ts` mock the `vscode`
  module and already call `vi.resetAllMocks()` in `beforeEach`, configuring
  implementations inside each test. The only `vitest` subpath import in the
  project is `vitest/config` in the config file, which remains supported.

- **Coverage is gated by Codecov, so the remapping change is observable.**
  `codecov.yml` sets `project.default` to `target: auto` with `threshold: 1%`
  and `patch.default` to `target: 80%`. The `coverage-vscode` job in
  `.github/workflows/coverage.yml` runs `pnpm run test:coverage` and uploads
  `rlsp-yaml/integrations/vscode/coverage/lcov.info` under the `vscode` flag with
  `fail_ci_if_error: true`.

- **The Node 22 pin has no rationale on record.** `node-version: '22'` appears
  four times — twice in `.github/workflows/coverage.yml` (the `coverage-vscode`
  and `vscode-static-checks` jobs) and twice in
  `.github/workflows/vscode-extension.yml`. It originated in commit `df2b68f6`
  (the first VSIX build workflow) and was copied into each workflow added since
  (`027c51e4`, `b6f8a5d9`, `82ed3663`); no commit message or comment explains it.
  There is no `.nvmrc` and no `engines.node` in `package.json` — only
  `engines.vscode: ^1.125.0`. Per the Node release schedule, v22 entered
  maintenance on 2025-10-21 (EOL 2027-04-30) and v24 is Active LTS until
  2026-10-20. A VS Code extension executes on the Node runtime bundled with VS
  Code, so the CI Node version governs build and test tooling only, not the
  shipped extension.

- **Project conventions that constrain this work.** Agents must not edit
  `version = "..."` fields in any `Cargo.toml` — release-plz owns version
  progression. Work lands directly on `main` (trunk-based); no feature branch or
  PR. Formatter/settings sync rules do not apply here, as no formatter setting
  changes.

- **References.** Vitest 5 release notes and migration guide
  (<https://vitest.dev/blog/vitest-5.html>, <https://vitest.dev/guide/migration>),
  the Node.js release schedule
  (<https://github.com/nodejs/Release/blob/main/schedule.json>), and the GitHub
  advisories named above.

## Steps

- [x] Clarify scope and the Node version decision with the user
- [x] Land the refreshed Rust and npm lockfiles and raise the `fast-uri` guard floor
- [x] Confirm the four Dependabot alerts close after the push
- [x] Realign `@types/vscode` with `engines.vscode` and guard the pair
- [ ] Move CI from Node 22 to Node 24 and record why
- [ ] Upgrade `vitest` and `@vitest/coverage-v8` to 5.0.0 and resolve the migration
- [ ] Measure and record the coverage delta caused by AST-based remapping
- [ ] Verify the full CI matrix is green after the final push

## Tasks

### Task 1: Land the refreshed lockfiles and make the `fast-uri` guard match the advisories

Commit the four already-verified dependency-refresh files and correct the
lockfile regression guard, whose `fast-uri` floor sits one patch below the
version that actually fixes the four open advisories. These ship together
because the refreshed lockfile is what resolves the alerts, and a guard that
still accepts `3.1.5` would let the same vulnerability return unnoticed.

- [x] The refreshed root `Cargo.lock`, Zed `Cargo.lock`, extension
      `package.json`, and `pnpm-lock.yaml` are committed with no unrelated
      changes included
- [x] No `version = "..."` field in any `Cargo.toml` is modified
- [x] The lockfile guard accepts a resolved `fast-uri` version only if it is
      patched on its own major line — `>= 3.1.6` on 3.x, `>= 4.1.3` on 4.x —
      and rejects every other major, failing closed. This is a single decision
      point, not a floor check plus a separate major check: the accepted floor
      cannot be changed without editing the floor itself
- [x] `isAtLeast` is directly unit-tested against literal version pairs
      spanning the new floor, including `('3.1.5','3.1.6') === false` (the exact
      regression this task closes), `('3.1.6','3.1.6') === true` (inclusive
      floor), and `('3.1.10','3.1.6') === true` (numeric, not lexicographic,
      comparison). These assertions are the standing proof that the floor
      rejects vulnerable versions — the lockfile-reading test cannot carry that
      claim, because once this task lands the real lockfile never contains a
      vulnerable version, so no floor value makes it fail
- [x] `parseVersion` is tested to throw on a non-release version string, the
      hard-failure behaviour its own comment documents
- [x] The assertion loop is proven non-vacuous: during implementation, raising
      the floor *above* the currently resolved version makes the lockfile test
      fail, and the handoff reports that observed fail-then-pass result. Raising
      is the correct direction — lowering a floor only makes it more permissive
      and can never produce a new failure. This check is not committed
- [x] The guard's explanatory comment names the four current advisories by
      GHSA identifier, states `3.1.6` as the patched version for the 3.x line,
      and records why the major is pinned
- [x] The handoff states the new total test count explicitly rather than
      implying the 49-test baseline still holds
- [x] `brace-expansion` and `serialize-javascript` assertions in the same file
      are unchanged and still pass
- [x] `pnpm run typecheck`, `lint`, `format`, `test`, and `audit` pass, and
      `cargo build`, `cargo clippy --all-targets`, and `cargo test` pass
- [x] The Zed extension checks and lints clean against its refreshed lockfile on
      the `wasm32-wasip2` target. The root workspace `members` list is
      `["rlsp-fmt", "rlsp-yaml", "rlsp-yaml-parser"]`, so the Zed crate is
      outside the workspace and the root `cargo` commands above never build it —
      it needs its own `--manifest-path rlsp-yaml/integrations/zed/Cargo.toml`
      invocations, per the "Zed Extension" section of the root `CLAUDE.md`
- [x] After the change is pushed to `main`, all four `fast-uri` Dependabot
      alerts (#52, #53, #56, #57) report as closed, and no new alert is open

### Task 2: Restore VSIX packaging by realigning `@types/vscode` with `engines.vscode`

`vsce package` refuses to build when `@types/vscode` exceeds `engines.vscode`,
and nothing in the fast gates notices — the check runs only inside packaging,
which executes late and only on `main`. Dependabot raised the types twice
without touching the engine, so all five platform builds now fail. Bring the
types back to the declared engine and add a guard so the pair cannot silently
diverge again.

- [x] `@types/vscode` and `engines.vscode` agree on major and minor, with
      `engines.vscode` unchanged at `^1.125.0` — the extension keeps working on
      VS Code 1.125 and newer
- [x] `vsce package` succeeds locally for at least one platform target,
      demonstrated rather than assumed
- [x] A guard test fails when the two fields disagree. It reads both values
      from `package.json` rather than hardcoding either, so it keeps working
      after a future deliberate bump
- [x] The guard is proven able to fail: temporarily diverging the two fields
      makes it fail, and restoring them makes it pass. That observed
      fail-then-pass result is reported; the diverged state is not committed
- [x] The guard states in a comment why the two fields are coupled and that
      `vsce` compares major.minor only, so a future reader does not treat the
      pairing as arbitrary
- [x] No Dependabot ignore rule is added for `@types/vscode` — a future bump
      should turn its PR red so the minimum-version decision is made
      deliberately, not suppressed
- [x] The existing 66 tests still pass and the new total is stated explicitly
- [x] `pnpm run typecheck`, `lint`, `format`, `test`, `audit`, and `build` pass
- [x] After the push, all five `Build VSIX` platform jobs in the VS Code
      Extension workflow are green, and the workflow itself succeeds

### Task 3: Move CI to Node 24 and record the reason

Replace all four `node-version: '22'` pins with Node 24 — the Active LTS line —
and leave a comment so the version stops being an undocumented copy-forward.
Node 22 is in maintenance, and Node 24 gives comfortable headroom over the
Vitest 5 floor that Task 4 introduces.

- [ ] Every workflow job that sets up Node for the extension runs Node 24; no
      `node-version: '22'` remains in `.github/workflows/`
- [ ] A comment at the pin records why this version was chosen and what would
      prompt changing it, so the next reader is not left guessing as with the
      previous pin
- [ ] `@types/node` is left at its current major — the runtime/types gap is a
      known, accepted difference
- [ ] All extension gates (`typecheck`, `lint`, `format`, `test`, `audit`) pass
      when run on Node 24
- [ ] After the push, the `coverage-vscode`, `vscode-static-checks`, and VS Code
      extension workflow jobs are green on the full CI matrix, Windows included
- [ ] The `publish-extension` pin is confirmed safe by direct evidence rather
      than by CI. That job is gated `if: needs.resolve-version.outputs.version
      != ''`, and `resolve-version` runs only on `workflow_dispatch`, so a push
      to `main` never executes it and the push-triggered run above cannot
      exercise this pin. It is the only Node-24 pin shipping without CI
      feedback; the handoff states that explicitly rather than implying the
      matrix covered it

### Task 4: Upgrade to Vitest 5 and record the coverage impact

Move `vitest` and `@vitest/coverage-v8` to 5.0.0 and resolve the behavioural
changes that reach this suite — default mock clearing, failure on unawaited
async assertions, and the switch to AST-based v8 coverage remapping. The
remapping changes reported coverage, and Codecov gates the project at a 1%
threshold, so the delta must be measured rather than assumed.

- [ ] `vitest` and `@vitest/coverage-v8` both resolve to 5.0.0 and remain
      version-aligned with each other
- [ ] All 49 existing tests pass, with no test disabled, skipped, or weakened to
      accommodate the upgrade
- [ ] The suite passes under the new default mock-clearing behaviour without
      relying on it — mock implementations each test depends on are established
      within that test or its `beforeEach`
- [ ] No assertion is left unawaited; the run reports no unawaited-assertion
      failures
- [ ] `vitest.config.mts` contains no option removed in Vitest 5, and the
      project imports no removed subpath entry point
- [ ] `pnpm run test:coverage` writes `coverage/lcov.info` at the path the
      `coverage-vscode` job uploads
- [ ] Total line coverage is measured before and after the upgrade, both figures
      are reported in the handoff, and the post-upgrade total is no more than 1
      percentage point below the pre-upgrade total
- [ ] `pnpm run typecheck`, `lint`, `format`, and `audit` pass
- [ ] After the push, the full CI matrix is green and the Codecov `vscode` flag
      reports without error

## Decisions

- **Node 24, not 22 or 26** — user's choice at clarification. v24 is Active LTS
  until 2026-10-20; v22 has been in maintenance since 2025-10-21. v26 does not
  reach LTS until 2026-10-28.
- **`@types/node` stays on its current major** — user declined aligning it down
  to `^24`. CI will typecheck against the Node 26 API surface while running Node
  24. Accepted knowingly; revisit if a type/runtime mismatch causes a real
  failure.
- **TypeScript stays at 6.0.3** — `typescript-eslint@8.69.0` declares
  `typescript: >=4.8.4 <6.1.0`, and lint is a required gate. Verified against the
  registry on 2026-09-07 rather than carried over from the earlier plan. The
  existing `dependabot.yml` ignore rule already covers this, so no change is
  needed.
- **The `fast-uri` guard floor moves to 3.1.6, not 3.1.7** — 3.1.6 is the first
  patched version for all four advisories. Pinning the floor to the currently
  resolved 3.1.7 would make the guard fail on an unrelated future downgrade that
  is not actually vulnerable.
- **The `fast-uri` guard uses a per-major floor, not a floor plus a major pin**
  — settled between the two advisors during Task 1. The underlying gap was
  raised by the security advisor at the input gate and verified directly
  against the GitHub advisory data on 2026-09-07. Every one of the four
  advisories lists
  three vulnerable ranges (`>= 2.3.1/2.4.1/2.4.2, < 2.4.5`;
  `>= 3.0.0/3.1.2/3.1.3, < 3.1.6`; `>= 4.0.0/4.0.1, < 4.1.3`), so a single floor
  is structurally the wrong control for this package: `isAtLeast('4.0.0',
  '3.1.6')` is true while `4.0.0` is vulnerable to all four.

  The first fix attempted was that floor plus a separate `major === 3`
  assertion. Both advisors then rejected it, for two reasons that compound:

  1. `fast-uri` carries no override, so its version drifts through ordinary
     transitive `pnpm update` churn. A pin on major 3 fires on a legitimately
     patched `4.1.3+` — a false alarm with no security meaning.
  2. Worse, the two assertions fail independently. Whoever hits that false
     alarm changes `.toBe(3)` to `.toBe(4)` and stops, because
     `isAtLeast(version, '3.1.6')` keeps passing for *any* 4.x — it was never
     major-aware. That silently reinstates the vulnerable-4.x hole the pin was
     added to close.

  The guard therefore uses a single `isPatchedFastUri` function with a
  per-major floor (`3.x → 3.1.6`, `4.x → 4.1.3`), collapsing this to one
  decision point: accepting a 4.x version requires `>= 4.1.3`, so no unrelated
  literal can silence a failure. Encoding `4.1.3` is not speculation — it is
  published advisory data, verified on 2026-09-07.

- **The guard fails closed on every major line except 3 and 4** — including
  2.x, whose `2.4.5` is genuinely patched. A transitive downgrade across a
  major line is unusual enough to deserve a human look regardless, and
  enumerating 2.x as vetted would reintroduce the per-patch bookkeeping this
  design exists to avoid.

- **The `overridesBlockOf` unguarded `indexOf` is a known latent gap, left
  as-is** — if pnpm ever stops emitting an `overrides:` block, `indexOf` returns
  `-1` and the slice yields near-empty text rather than failing loudly. It is
  pre-existing, outside this plan's Non-Goals boundary on overrides handling,
  and recorded here so it is carried forward knowingly rather than silently.

- **Lockfiles and the guard fix ship as one commit** — the guard's correctness
  is what makes the security posture of the refreshed lockfile durable; splitting
  them would leave a window where the guard silently under-enforces.
- **The `publish-extension` Node pin is accepted on evidence, not CI** — it
  cannot be reached by a push-triggered run (see Task 3). The residual risk is
  small and was checked directly on 2026-09-07: the job performs no build, lint,
  or test under its Node setup, using it only to run
  `pnpx @vscode/vsce publish` against an artifact built elsewhere;
  `@vscode/vsce@3.9.2` declares `engines: { node: '>= 20' }`, and its CLI was
  confirmed working under Node 24.14.1 locally. A failure would surface visibly
  at the next `workflow_dispatch` release rather than silently.

- **`engines.vscode` stays at `^1.125.0`; `@types/vscode` comes down to match**
  — user's choice. The source uses no API introduced after 1.125, so the newer
  types buy nothing today, and holding the engine keeps the extension
  installable for VS Code 1.125+ users. Raising the engine is a user-facing
  compatibility narrowing and should be a deliberate decision made when an API
  actually requires it.

- **No Dependabot ignore for `@types/vscode`; a guard test instead** — user's
  choice. An ignore rule hides the bump; a guard test turns a future Dependabot
  PR red in seconds and forces an explicit call on the minimum supported VS Code
  version. Dependabot cannot group the two in any case: `engines.vscode` is a
  manifest field, not a dependency, so no `dependabot.yml` grouping can keep
  them in lockstep.

- **Coverage delta is measured, not predicted** — AST-based remapping generally
  raises reported coverage by dropping non-executable lines from the
  denominator, but the direction is not assumed. If the measured drop exceeds
  the 1 percentage point bar, that is a blocker to raise, not a number to
  restate.

## Non-Goals

- **Upgrading TypeScript to 7.0.2** — blocked by the `typescript-eslint` peer
  range; unblocked only when typescript-eslint supports TS >= 7.1.
- **Closing Dependabot PR #62, landing PR #63, or acting on release-plz PR #44**
  — the user explicitly excluded all three from this plan.
- **Changing `pnpm.overrides` or the `auditConfig.ignoreCves` allowlist** — the
  current overrides posture was audited on 2026-08-07 and is out of scope; only
  the guard's `fast-uri` floor changes.
- **Adding an `.nvmrc` or an `engines.node` field** — the Node version is pinned
  in the workflows only; introducing a second source of truth is not part of this
  work.
- **Changing which workflows run which gates** — the CI gate coverage added on
  2026-08-10 stands as-is; only the Node version within those jobs changes.
