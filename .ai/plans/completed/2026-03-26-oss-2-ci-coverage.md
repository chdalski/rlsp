**Repository:** root
**Status:** Completed (2026-03-26)
**Created:** 2026-03-26

## Goal

Set up GitHub Actions CI workflows for testing, linting,
and code coverage with Codecov integration, plus Dependabot
for automated dependency updates.

## Context

- No CI/CD exists currently — no `.github/` directory
- Build/test: `cargo fmt --check`, `cargo clippy`, `cargo test`
- Clippy is configured at pedantic+nursery with several
  deny-level lints
- ~30 integration tests in `rlsp-yaml/tests/lsp_lifecycle.rs`
- Coverage tool: `cargo-llvm-cov` (modern, accurate,
  works with llvm-based instrumentation)
- Codecov for coverage reporting and PR comments
- Repository: https://github.com/cdalski/rlsp

## Steps

- [x] Create CI workflow (test, lint, format check)
- [x] Create coverage workflow with Codecov upload
- [x] Add codecov.yml configuration
- [x] Create dependabot.yml
- [ ] Add CI and coverage badges to root README (deferred to Plan 5)

## Tasks

### Task 1: CI workflow

Create `.github/workflows/ci.yml`:

- **Trigger:** push to `main`, pull requests to `main`
- **Jobs:**
  - `check` — `cargo fmt --check` + `cargo clippy -- -D warnings`
  - `test` — `cargo test` on stable Rust
  - Run on `ubuntu-latest`
- Use `actions/checkout@v4`, `dtolnay/rust-toolchain@stable`
- Cache cargo registry/target with `Swatinem/rust-cache@v2`

- [ ] CI workflow file
- [ ] Verify workflow syntax is valid YAML

### Task 2: Coverage workflow + Codecov

Create `.github/workflows/coverage.yml`:

- **Trigger:** push to `main`, pull requests to `main`
- **Job:**
  - Install `cargo-llvm-cov`
  - Run `cargo llvm-cov --workspace --lcov --output-path lcov.info`
  - Upload to Codecov with `codecov/codecov-action@v4`
- Requires `CODECOV_TOKEN` secret (mention in PR/docs)

Create `codecov.yml` at repo root:
```yaml
coverage:
  status:
    project:
      default:
        target: auto
        threshold: 1%
    patch:
      default:
        target: 80%
comment:
  layout: "diff, flags, files"
  require_changes: true
```

- [ ] Coverage workflow file
- [ ] codecov.yml at repo root
- [ ] Note about CODECOV_TOKEN secret setup

### Task 3: Dependabot

Create `.github/dependabot.yml`:
```yaml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
    groups:
      rust-dependencies:
        patterns: ["*"]
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    groups:
      github-actions:
        patterns: ["*"]
```

- [ ] dependabot.yml

## Decisions

- **cargo-llvm-cov over tarpaulin** — more accurate
  instrumentation-based coverage, better maintained, works
  with async code and the `tokio` runtime used by the tests
- **Grouped Dependabot updates** — group all Cargo deps
  into one PR and all GitHub Actions into one PR to reduce
  PR noise
- **Separate CI and coverage workflows** — CI runs fast
  (check + test), coverage is slower and uploads to
  external service. Separating them means CI gives fast
  feedback on PRs.
