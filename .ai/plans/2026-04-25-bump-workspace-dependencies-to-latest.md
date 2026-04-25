**Repository:** root
**Status:** InProgress
**Created:** 2026-04-25

## Goal

Bump every dependency across the rlsp workspace — the
three Rust crates (`rlsp-fmt`, `rlsp-yaml-parser`,
`rlsp-yaml`) and the VS Code extension at
`rlsp-yaml/integrations/vscode/` — to its absolute latest
published version, including major-version bumps. Bump
the two non-dependency version fields in the VS Code
extension manifest as well: `engines.vscode` (aligned to
the new `@types/vscode` major) and `packageManager`
(aligned to the latest pnpm 10.x). Verify the entire
workspace continues to build, lint, format, and test
cleanly across both ecosystems with zero warnings — every
existing test must continue to pass, with no allowlists
or skips.

## Context

Constraints set during clarification:
- **Aggressiveness:** every dep bumped to absolute
  latest, including incompatible (major) bumps in both
  ecosystems. Lowering scope to "minor/patch only" or
  "manifest caret only" is not authorized; if a specific
  major bump turns out to require unbounded refactoring,
  the developer must pause and consult rather than
  silently downgrade the bump.
- **Slicing:** one task slice per crate / extension —
  four slices total, ordered for foundational-first
  isolation.
- **Non-dep fields:** both `engines.vscode` and
  `packageManager` are in scope.
- **Test scope per slice:** `cargo build` /
  `cargo test` / `cargo clippy --all-targets` for Rust
  crates; `pnpm run lint`, `pnpm run format`,
  `pnpm run build`, `pnpm run test`, and headless
  `xvfb-run -a pnpm run test:integration` for the VS Code
  extension.

Workspace conventions that constrain bump fallout (root
`CLAUDE.md` and `rlsp-yaml/integrations/vscode/CLAUDE.md`):
- Workspace lints enforce `warnings = "deny"` plus
  `clippy::pedantic` and `clippy::nursery` at warn, with
  selected lints (`unwrap_used`, `expect_used`, `panic`,
  `allow_attributes`, etc.) at deny. New warnings or new
  lint failures introduced by a bump must be fixed in
  source — `#[expect(lint, reason = "...")]` is the only
  permitted local override.
- Workspace path dependencies must include a `version`
  field — `cargo publish` rejects path-only deps. Do not
  drop the `version` from `rlsp-fmt`/`rlsp-yaml-parser`
  workspace path entries when bumping.
- TS strictness: `tsconfig.json` extends
  `@tsconfig/strictest`; ESLint uses `strictTypeChecked` +
  `stylisticTypeChecked` from `typescript-eslint`. Bumps
  to TS, ESLint, or `typescript-eslint` may surface new
  lint errors that must be fixed.
- pnpm store policy (per
  `rlsp-yaml/integrations/vscode/CLAUDE.md`): default
  store, no `store-dir` override in `.npmrc`. Do not
  introduce `.npmrc` settings while bumping.

State of the working tree at plan time:
- All previous plans in `.ai/plans/` are marked
  `Completed`. No in-flight work to coordinate with.
- `pnpm outdated` (run from
  `rlsp-yaml/integrations/vscode/`) reports the only
  npm-side major bump as `vite 6.4.2 → 8.0.10`; all
  other bumps are minor or patch. Vitest 4.x supports
  Vite 7+ — the bump should be tractable.
- `cargo update --dry-run` reports compatible bumps for
  19 transitive crates only; direct-manifest major
  bumps require `cargo upgrade --incompatible` (from
  `cargo-edit`), which is **not currently installed**.
- Local pnpm is `10.33.0`; the `packageManager` field in
  `package.json` is pinned at `pnpm@9.15.5`.
- Local cargo is `1.94.1`. xvfb-run is available at
  `/usr/bin/xvfb-run`, satisfying the integration-test
  requirement on Linux.

Direct dependencies that will be touched per crate (so
the developer does not need to re-discover them):
- `rlsp-fmt/Cargo.toml`: **no** `[dependencies]` or
  `[dev-dependencies]` sections. Verification only —
  see Task 2.
- `rlsp-yaml-parser/Cargo.toml`: deps `memchr "2"`,
  `thiserror "2"`; dev-deps `rstest "0.26"`,
  `criterion "0.8"`.
- `rlsp-yaml/Cargo.toml`: deps `rlsp-fmt` (path),
  `rlsp-yaml-parser` (path), `tower-lsp "0.20"`,
  `tokio "1"`, `serde_json "1"`, `serde "1"`,
  `regex "1"`, `once_cell "1"`, `ureq "3"`,
  `idna "1.0"`, `iri-string "0.7"`,
  `data-encoding "2.9"`; dev-deps `criterion "0.8"`,
  `futures "0.3"`, `rstest "0.26"`, `tiny_http "0.12"`,
  `tower "0.5"`.
- `rlsp-yaml/integrations/vscode/package.json`:
  dependencies `vscode-languageclient "^9.0.1"`;
  devDependencies `@tsconfig/strictest "^2.0.8"`,
  `@types/mocha "^10.0.10"`, `@types/node "^25.6.0"`,
  `@types/vscode "^1.115.0"`, `@vitest/coverage-v8
  "^4.1.4"`, `@vscode/test-cli "^0.0.12"`,
  `@vscode/test-electron "^2.5.2"`, `@vscode/vsce
  "^3.7.1"`, `esbuild "^0.28.0"`, `eslint "^10.0.0"`,
  `prettier "^3.8.2"`, `typescript "^6.0.0"`,
  `typescript-eslint "^8.58.1"`, `vite "^6.0.0"`,
  `vitest "^4.0.0"`. Plus `pnpm.overrides.lodash
  ">=4.18.0"` (preserve as-is — unrelated to bumps).

References: workspace conventions in `/workspace/CLAUDE.md`;
VS Code extension conventions in
`/workspace/rlsp-yaml/integrations/vscode/CLAUDE.md`;
release-plz tag format and trusted-publishing setup
(unaffected by this plan but informs why publishing must
not silently break).

## Steps

- [x] Bump every dep + non-dep field in the VS Code
  extension; verify lint/format/build/unit/integration
  tests
- [x] Verify `rlsp-fmt` has no manifest deps and the
  crate still builds and tests cleanly
- [x] Bump every dep in `rlsp-yaml-parser`; verify
  build/test/clippy/bench-compile
- [x] Bump every dep in `rlsp-yaml`; verify
  build/test/clippy/bench-compile and run a final
  workspace-wide verification

## Tasks

### Task 1: VS Code extension dependency bump

**Completed:** 2026-04-25 — commit `08e97b6cedae35065f532bf1ff126afd8e417d01`

Update every entry in
`rlsp-yaml/integrations/vscode/package.json` —
`dependencies` and `devDependencies` — to the absolute
latest published version, including the major bump of
`vite` (6 → 8) and any other majors that surface during
enumeration. Bump `engines.vscode` to align with the new
`@types/vscode` major. Bump `packageManager` to the
latest pnpm 10.x. Refresh `pnpm-lock.yaml`. Adjust source
in `src/`, the eslint config, the prettier config, the
vitest config, the vscode-test config, or the tsconfig
as needed to keep every existing check green.

- [x] Enumerate latest versions for every entry in
  `dependencies` and `devDependencies` using
  `pnpm outdated` plus `pnpm view <pkg> version` for any
  package not flagged outdated (some are at-latest but
  must be re-confirmed)
- [x] Edit `package.json`: set every dep's caret range
  to its latest version (e.g., `"vite": "^8.0.10"`)
- [x] Bump `engines.vscode` to `^<NEW_TYPES_VSCODE_MAJOR>.<MINOR>.0`
  matching the new `@types/vscode` (e.g., if types
  become `^1.116.0`, set engines to `^1.116.0`)
- [x] Bump `packageManager` to `pnpm@<latest-10.x>`
  (use `pnpm view pnpm version` for the exact value)
- [x] Run `pnpm install` to regenerate `pnpm-lock.yaml`
  against the new manifest
- [x] Run `pnpm run lint` from
  `rlsp-yaml/integrations/vscode/` — exit code 0, zero
  ESLint errors
- [x] Run `pnpm run format` from the same directory —
  prettier check passes (exit 0)
- [x] Run `pnpm run build` — esbuild bundles
  `out/main.js` without error
- [x] Run `pnpm run test` — all vitest unit tests pass,
  exit code 0
- [x] Run `xvfb-run -a pnpm run test:integration` — all
  vscode-test integration tests pass, exit code 0
- [x] Run `pnpm run package` — vsce produces a `.vsix`
  without errors. Warnings about missing
  `repository`/`license`/etc. that are unchanged from the
  baseline are acceptable; warnings introduced by the
  bumps are not — fix them.
- [x] If any of the above checks fail because a bump
  introduced breaking API changes, edit the consuming
  source (`src/**/*.ts`, `eslint.config.mjs`,
  `vitest.config.ts`, `.vscode-test.mjs`, `tsconfig.json`)
  to match the new API. Each such edit is part of this
  task — do not defer to a follow-up plan.

Files (expected, may grow during execution):
- `rlsp-yaml/integrations/vscode/package.json`
- `rlsp-yaml/integrations/vscode/pnpm-lock.yaml`
- Any `src/**/*.ts`, `eslint.config.mjs`,
  `vitest.config.ts`, `.vscode-test.mjs`, or
  `tsconfig.json` that needs adjustment for breaking
  API changes.

Advisors: consult **security-engineer** at the input
gate (risk assessment) and again at the output gate
(sign-off) before submitting to the reviewer.
`vscode-languageclient` is the LSP transport between the
VS Code editor and the rlsp-yaml server binary — a trust
boundary. Major bumps to build/test tooling
(`esbuild`, `vite`, `@vscode/test-electron`,
`@vscode/vsce`) can change defaults that affect what
JavaScript code ships in the published `.vsix`. The
security-engineer evaluates whether any bump changes a
security-relevant default or API; surface-level "we
bumped versions" is not equivalent to a risk assessment.

### Task 2: rlsp-fmt verification

**Completed:** 2026-04-25 — commit `188a2078093a74f5ab9cfca651728a13f1d8c864`

The `rlsp-fmt` crate's `Cargo.toml` declares no
`[dependencies]` and no `[dev-dependencies]`. This task
records that fact, runs the standard verification suite,
and confirms the crate compiles and tests pass at the
current workspace lockfile state. The user explicitly
chose four slices including `rlsp-fmt`; this task is the
explicit verification slice for the dep-free crate. It
is expected to produce no source-file changes — the
plan-checkbox commit is the task's closure artifact.

- [x] Read `rlsp-fmt/Cargo.toml` and confirm there are
  no `[dependencies]` or `[dev-dependencies]` sections
- [x] Run `cargo build -p rlsp-fmt` — zero errors
- [x] Run `cargo test -p rlsp-fmt` — every test passes,
  exit code 0
- [x] Run `cargo clippy -p rlsp-fmt --all-targets` —
  zero warnings (workspace `warnings = "deny"` makes any
  warning a hard error)
- [x] Run `cargo fmt -p rlsp-fmt -- --check` — formatter
  check passes (exit 0)

Files: none expected. The verified file list submitted
to the reviewer for this task is empty for source code;
the plan-checkbox commit will carry only the plan file
update.

Advisors: none. Verification-only, no source changes,
no deps bumped.

### Task 3: rlsp-yaml-parser dependency bump

**Completed:** 2026-04-25 — commit `1743f663c12e8b4ea14c194c79abe475f77bbc56`

Outcome: every direct dep was already at its absolute latest
published version (`memchr 2.8.0`, `thiserror 2.0.18`,
`rstest 0.26.1`, `criterion 0.8.2`).
`cargo upgrade --dry-run --incompatible -p rlsp-yaml-parser`
reported `latest: rlsp-yaml-parser` (no proposed bumps). No
manifest or source changes were required; existing
verification commands all passed (2,588 tests, 0 clippy
warnings, 3 benches compile).

Update every dep in `rlsp-yaml-parser/Cargo.toml` —
`memchr`, `thiserror`, `rstest`, `criterion` — to the
absolute latest version, including incompatible majors
(e.g. `rstest 0.26 → 0.27`+, `criterion 0.8 → 0.9`+ if
released). Refresh `Cargo.lock`. Fix any compile, test,
clippy, or benchmark-compile breakage in
`rlsp-yaml-parser/src/`, `rlsp-yaml-parser/tests/`, and
`rlsp-yaml-parser/benches/` produced by the bumps.

- [x] If `cargo upgrade` is unavailable
  (`cargo upgrade --help` errors), install it via
  `cargo install cargo-edit`
- [x] Run `cargo upgrade --dry-run --incompatible -p rlsp-yaml-parser`
  and record the proposed bumps (none — already at latest)
- [x] Apply the bumps with
  `cargo upgrade --incompatible -p rlsp-yaml-parser`
  (no-op — nothing to apply)
- [x] Run `cargo update -p rlsp-yaml-parser` to refresh
  `Cargo.lock` (no-op — `Locking 0 packages`)
- [x] Run `cargo build -p rlsp-yaml-parser` — zero
  errors
- [x] Run `cargo test -p rlsp-yaml-parser` — every test
  passes, including yaml-test-suite event-stream
  conformance and loader conformance suites; exit code 0
  (2,588 passed)
- [x] Run `cargo clippy -p rlsp-yaml-parser --all-targets` —
  zero warnings
- [x] Run `cargo fmt -p rlsp-yaml-parser -- --check` —
  exit 0
- [x] Run `cargo bench -p rlsp-yaml-parser --no-run` —
  every benchmark target compiles (3 benches)
- [x] If a major bump introduces breaking API changes
  (e.g., `rstest` macro signature changes, `criterion`
  group-config changes), update the consuming test or
  bench source as part of this task (N/A — no bumps)

Files (expected, may grow):
- `rlsp-yaml-parser/Cargo.toml`
- `Cargo.lock`
- Any `rlsp-yaml-parser/tests/**/*.rs` or
  `rlsp-yaml-parser/benches/**/*.rs` requiring
  adjustment.

Advisors: none. Direct deps are `memchr` (SIMD byte
search, no network/IO surface), `thiserror`
(error-derive macro, compile-time only), `rstest` and
`criterion` (test/bench infrastructure). None cross a
trust boundary; the parser handles untrusted YAML
input but the bumped deps do not participate in that
parsing path. Existing tests are the verification.

### Task 4: rlsp-yaml dependency bump

**Completed:** 2026-04-25 — commit `10ffcbf1477bca65b8c313c5fd9380de97b0642c`

Outcome: only two manifest bumps were available — `idna 1.0 → 1.1`
and `data-encoding 2.9 → 2.11`. All other direct deps and dev-deps
were already at their absolute latest (verbose `cargo upgrade
--dry-run --incompatible` reported `latest: 19 packages`). Workspace
path entries retained their pinned `version` strings. The
security-engineer input gate flagged a clarity concern around the
silent proxy-URL fallback in `build_agent`; resolved by adding an
inline comment documenting the intentional behavior. Output gate
sign-off granted. Workspace-wide verification clean: build, all
tests, clippy, fmt, and bench-compile across all three crates.

Update every dep in `rlsp-yaml/Cargo.toml` — both
`[dependencies]` (excluding the two workspace path
deps, which keep their current pinned `version` field)
and `[dev-dependencies]` — to the absolute latest
version, including incompatible majors. The path entries
for `rlsp-fmt` and `rlsp-yaml-parser` keep their
existing `version = "..."` strings (cargo publish
requires them); only the non-path crates are bumped.
Refresh `Cargo.lock`. Adjust source code in
`rlsp-yaml/src/`, `rlsp-yaml/tests/`, and
`rlsp-yaml/benches/` for any breaking API changes
(notably possible in `tower-lsp`, `tokio`, `ureq`,
`idna`, `iri-string`, and `regex`). Then run a final
workspace-wide verification to catch any cross-crate
fallout.

- [x] Run `cargo upgrade --dry-run --incompatible -p rlsp-yaml`
  and record the proposed bumps. The two workspace
  `path = "..."` entries (`rlsp-fmt`,
  `rlsp-yaml-parser`) must be excluded from the bump
  because their version is governed by the local crate
  release, not crates.io
- [x] Apply the bumps with
  `cargo upgrade --incompatible -p rlsp-yaml --exclude rlsp-fmt --exclude rlsp-yaml-parser`
  (or hand-edit `rlsp-yaml/Cargo.toml`)
- [x] Run `cargo update -p rlsp-yaml` to refresh
  `Cargo.lock`
- [x] Run `cargo build -p rlsp-yaml` — zero errors
- [x] Run `cargo test -p rlsp-yaml` — every test passes,
  exit code 0
- [x] Run `cargo clippy -p rlsp-yaml --all-targets` —
  zero warnings
- [x] Run `cargo fmt -p rlsp-yaml -- --check` — exit 0
- [x] Run `cargo bench -p rlsp-yaml --no-run` — every
  benchmark target compiles
- [x] Run the final workspace-wide verification:
  `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets` — every
  command exits 0 with zero warnings
- [x] For each major bump that changes a public API
  used by `rlsp-yaml/src/`, update the consuming source
  to match. Each such change is part of this task.

Files (expected, may grow):
- `rlsp-yaml/Cargo.toml`
- `Cargo.lock`
- Any `rlsp-yaml/src/**/*.rs`, `rlsp-yaml/tests/**/*.rs`,
  or `rlsp-yaml/benches/**/*.rs` requiring adjustment.

Advisors: consult **security-engineer** at the input
gate (risk assessment) and again at the output gate
(sign-off) before submitting to the reviewer. This
crate's dependencies cross multiple trust boundaries:
- `tower-lsp` — LSP transport between editor and the
  language server binary; protocol-level changes affect
  parsing of incoming editor-controlled JSON-RPC.
- `ureq` (with `rustls` feature) — outbound HTTP for
  schema fetching from user-configurable URLs;
  TLS-stack defaults and redirect-handling changes are
  security-relevant.
- `idna` and `iri-string` — parse user-provided schema
  URLs and IRIs; host/punycode handling and IRI
  normalization changes are security-relevant.
- `regex` — invoked on user-supplied input in
  validators and code actions; changes to default
  size/time limits affect ReDoS exposure.
- `serde` / `serde_json` — deserializes editor-provided
  configuration and JSON-RPC payloads.

The security-engineer evaluates whether any major bump
changes a security-relevant default or API — that is
their gate, not a generic "bumps look fine" sign-off.

## Decisions

- **Major bumps are mandatory, not optional.** The user
  explicitly chose "all to latest, including majors" for
  both ecosystems. If a specific bump turns out to
  require unbounded refactoring, the developer pauses
  and consults rather than silently downgrading the
  bump. There is no pre-authorized fallback to "skip
  this dep" or "stay on the prior major."
- **Slice ordering — VS Code first, then Rust crates
  by dependency depth.** VS Code is an independent
  ecosystem with no Rust-side coupling at build time;
  starting there isolates one ecosystem's fallout from
  the other. Rust slices follow `rlsp-fmt` →
  `rlsp-yaml-parser` → `rlsp-yaml` in dependency order
  so that any cross-crate compile failures surface in
  the slice that introduced them.
- **`rlsp-fmt` slice is verification-only and may
  produce no source changes.** The user explicitly
  chose four slices including `rlsp-fmt`; the slice
  honors that scoping decision while accurately
  reflecting that the crate has no manifest deps to
  bump.
- **Workspace path deps in `rlsp-yaml` keep their
  pinned `version` field.** `cargo publish` rejects
  path-only deps (per workspace convention in root
  CLAUDE.md), so the `version = "0.1.7"` and
  `version = "0.5"` strings on the `rlsp-fmt` and
  `rlsp-yaml-parser` path entries stay as-is. The
  developer must `--exclude` these when running
  `cargo upgrade --incompatible` so the tool doesn't
  bump the version string against crates.io's latest
  for those package names.
- **`engines.vscode` follows the `@types/vscode`
  major.** End-user impact: VS Code installations older
  than the new minor will no longer be able to install
  the extension. The user explicitly authorized this
  field's bump.
- **`packageManager` follows latest pnpm 10.x.** Local
  pnpm is already 10.33.0, so the field is brought into
  alignment with the actual environment. CI and
  contributor environments using corepack pick up the
  bump automatically.
- **No allowlist or skip-list is introduced.** Every
  existing test must continue to pass after the bumps.
  The user did not authorize a "skip these tests"
  fallback. If a test starts failing under a new dep
  version, fix the test or the dep usage — do not
  allowlist the failure.
- **Test-engineer not consulted on any slice.** The
  unique verification path for dep bumps is "run all
  existing tests" — the test-engineer has no specific
  guidance to add beyond "execute the existing suites,"
  which is already directed in every slice. Adding the
  consult would dilute the advisor signal.
- **Security-engineer consulted on Tasks 1 and 4.**
  Task 1 (vscode-languageclient is the editor↔server
  trust boundary; build/test tooling determines what
  ships in the `.vsix`) and Task 4 (multiple
  network/URL/transport deps) cross trust boundaries
  in ways that major bumps can shift. Task 2
  (verification only) and Task 3 (dev-tooling and
  byte-search deps) do not.

## Non-Goals

- Bumping `rust-version = "1.87"` in any crate — left
  unchanged unless a dep's MSRV exceeds it, in which
  case the developer pauses and consults the user
  rather than bumping unilaterally.
- Refactoring or restructuring code beyond what is
  required to compile and test against new dep
  versions. No "while we're here" cleanup.
- Splitting, renaming, or reorganizing source files.
- Adding new tests beyond those needed to cover a
  breaking API adjustment forced by a bump.
- Updating Dependabot, release-plz, or git-cliff
  configuration (unaffected by this plan).
- Publishing new crate versions to crates.io or
  releasing a new VS Code extension `.vsix` to the
  marketplace.
- Modifying `pnpm.overrides.lodash >=4.18.0` —
  unrelated to bump scope; preserve as-is.
