# Codecov Sanity Check

Audit the Codecov configuration and coverage visibility
for common issues. This check is **audit-only** — collect
findings and return them to the dispatcher. Do not fix
anything.

## Checks

### 1. Config Validation

Read `codecov.yml` (or `codecov.yaml`) at the repository
root and check for:

1. **Invalid YAML structure** — attempt to parse the file.
   A syntax error means Codecov silently ignores the config
   and falls back to defaults, so any customizations the
   team intended are not applied.
2. **Deprecated settings** — look for the legacy `notify:`
   top-level key, which was replaced by `comment:` and
   `coverage.status:`. Deprecated keys are silently ignored
   in newer Codecov versions, so the team's notification
   preferences have no effect.
3. **Missing `coverage:` section** — a config file with no
   `coverage:` key sets no coverage targets. This is likely
   an incomplete setup or a leftover from a removed
   integration — contributors see no coverage gates on
   their PRs.
4. **Overly permissive thresholds** — if
   `coverage.status.project.default.target` or
   `coverage.status.patch.default.target` is set to `0` or
   a similarly low value, the coverage gate is effectively
   disabled and will never block a PR regardless of
   coverage regression.

### 2. CI Integration

If `.github/workflows/` exists, scan all workflow files for
a coverage upload step. Look for any of these patterns:

- `codecov/codecov-action@` (the official GitHub Action)
- `codecovcli upload` or `codecovcli do-upload` (the CLI)
- `bash <(curl -s https://codecov.io/bash)` (the legacy
  bash uploader)

A `codecov.yml` with no upload step in CI means Codecov is
configured but never receives data — the coverage dashboard
will be empty or stale, and contributors may assume coverage
is being tracked when it is not.

### 3. Coverage Tool Detection

Check whether the project has a coverage-generating tool
configured for its detected language. Codecov only receives
data if a tool generates a coverage report first — the
upload step sends whatever artifact exists, and if no tool
generates one, the upload silently succeeds with no data.

Language-specific indicators:

- **Python** — `pytest-cov` in `requirements*.txt`,
  `pyproject.toml` (`[project.optional-dependencies]` or
  `[tool.poetry.dependencies]`), or `setup.cfg`;
  `--cov` flag in pytest config (`pyproject.toml`
  `[tool.pytest.ini_options]`, `pytest.ini`, `setup.cfg`)
- **Node.js** — `nyc`, `c8`, or `@vitest/coverage-*` in
  `package.json` `devDependencies`; `--coverage` flag in
  test scripts
- **Go** — `-cover` or `-coverprofile` flags in CI
  workflow files or `Makefile`
- **Rust** — `cargo-tarpaulin` or `cargo-llvm-cov` in CI
  workflow files or as a Cargo dependency

Record a finding if Codecov config exists but no coverage
tool is detected for any of the project's languages.

### 4. Upload Token

Check whether the CI upload step references a Codecov
token (typically `${{ secrets.CODECOV_TOKEN }}` or an
environment variable named `CODECOV_TOKEN`).

- **Public repositories** can use tokenless uploads — a
  missing token is informational, not an error.
- **Private repositories** require a `CODECOV_TOKEN` for
  uploads. Without it, uploads fail silently — Codecov
  returns a success status but discards the data, so the
  coverage dashboard goes stale with no CI error to flag
  the problem.

If the upload step exists but has no token reference, record
a finding. If the repository's visibility cannot be
determined, note the ambiguity and recommend verifying
manually.

### 5. Coverage Gap Detection

Identify areas of the codebase with low or missing test
coverage. Execute these three levels in order, using the
first level that produces per-file coverage data (levels 2
and 3 are alternatives — run level 3 only if level 2
found no artifacts).

#### Level 1 — Structural Gap Detection (always run)

Scan source directories for files and directories with no
corresponding test file. This catches the most common
coverage blind spots — entire modules with no tests at
all — without requiring any coverage tooling.

Use language conventions for test file naming:

- Python: `foo.py` → `test_foo.py` or `foo_test.py`
- TypeScript/JavaScript: `foo.ts` → `foo.test.ts` or
  `foo.spec.ts`
- Go: `foo.go` → `foo_test.go` (same directory)
- Rust: check for `#[cfg(test)]` module in the source
  file, or a corresponding file under `tests/`

Exclude non-source files (configs, migrations, generated
code, `__init__.py`, `mod.rs`) from the analysis — these
rarely need dedicated tests.

Record a finding for each source file or directory with
no test counterpart. Group findings by directory to avoid
excessive noise.

#### Level 2 — Local Coverage Artifacts (if available)

Look for existing coverage reports from a previous local
or CI run. These are typically gitignored but may exist
in the working tree:

| Language | Artifacts |
|---|---|
| Python | `.coverage`, `coverage.xml`, `htmlcov/index.html` |
| Node.js | `coverage/lcov.info`, `coverage/clover.xml` |
| Go | `cover.out`, `cover.html` |
| Rust | `tarpaulin-report.xml`, `lcov.info` |
| Generic | `cobertura.xml` |

If any artifact is found, parse it to extract per-file
coverage percentages. Flag files below the project's
configured target (from the `codecov.yml` `coverage:`
section), or below 50% if no target is configured — 50%
is a conservative floor that avoids excessive noise while
still flagging clearly undertested files.

If coverage artifacts are found, skip Level 3 — local
data is more current than what Codecov has cached.

#### Level 3 — Codecov API (fallback)

If no local coverage artifacts were found in Level 2,
query the Codecov API for the latest coverage data. This
provides per-file coverage from the most recent CI upload
without requiring local test execution.

1. Determine `{owner}` and `{repo}` from `git remote -v`
   (parse the first `origin` remote URL).
2. Determine the service — use `github` unless the remote
   URL indicates GitLab or Bitbucket.
3. Fetch coverage data:
   ```
   GET https://api.codecov.io/api/v2/{service}/{owner}/repos/{repo}/totals/
   ```
   This endpoint returns overall coverage and a per-file
   breakdown. It works **without authentication for public
   repositories**.
4. If the request fails with 401 or 403, check for a
   `CODECOV_TOKEN` environment variable and retry with
   an `Authorization: bearer <token>` header. If no token
   is available, record a finding noting that the repo
   appears private and a `CODECOV_TOKEN` is needed for
   online coverage lookup.
5. If the request fails with 404, the repo has no coverage
   data on Codecov — record this as informational and rely
   on Level 1 findings only.
6. If data is returned, report overall coverage and flag
   files below the project's configured target (or below
   50% if no target is set).

## Finding Format

Return each finding in this structure:

```
- Severity: high | medium | low
  File: <path to config, workflow, or source file>
  Location: <YAML key path, line number, or "repo-level">
  Finding: <what is wrong>
  Recommendation: <what to do>
```

**Severity guidance:**

- **high** — Codecov configured but no CI upload step
  (dead configuration), or private repo with no
  `CODECOV_TOKEN` (silent upload failure)
- **medium** — missing `coverage:` section in config,
  deprecated settings, no coverage tool detected for the
  project's language
- **low** — structural coverage gaps (source files with
  no test counterpart), overly permissive thresholds,
  low coverage on individual files from artifacts or API
