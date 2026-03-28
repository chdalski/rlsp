# GitHub Sanity Check

Audit GitHub Actions workflow files for common issues.
This check is **audit-only** — collect findings and return
them to the dispatcher. Do not fix anything.

## Checks

### 1. Action Version Currency

For each workflow file in `.github/workflows/`:

1. Read the file and extract every `uses:` line.
2. For each action reference (e.g., `actions/checkout@v4`,
   `softprops/action-gh-release@v2`), determine the latest
   stable major version by querying the action's repository:
   ```
   gh api repos/<owner>/<repo>/releases/latest
   ```
3. Compare the version in use against the latest release.
4. Record a finding for each outdated action.

### 2. Workflow Permissions

For each workflow file in `.github/workflows/`:

1. Check for a top-level `permissions:` block.
2. For workflows without a top-level block, check each job
   for a per-job `permissions:` block.
3. A workflow is non-compliant if it has neither a
   top-level block nor per-job blocks on every job.
4. For non-compliant jobs, examine the job's steps to
   suggest least-privilege permissions:
   - Steps that check out code → `contents: read`
   - Steps that create releases or push → `contents: write`
   - Steps that comment on PRs → `pull-requests: write`
   - Steps that use OIDC (e.g., crates.io publish) →
     `id-token: write`
5. Record a finding for each non-compliant workflow or job.

### 3. Node.js Deprecation

While examining actions in check 1, also flag any action
version known to use a deprecated Node.js runtime:

- Node.js 16 — deprecated; actions still on `node16`
  runner cause CI warnings on every run
- Node.js 20 — deprecated as of 2026; actions still on
  `node20` runner will soon cause CI warnings

An action version is affected if the GitHub releases page
or deprecation warnings indicate it runs on a deprecated
runtime. This is often the root cause of "Node.js XX is
deprecated" warnings in CI logs.

## Finding Format

Return each finding in this structure:

```
- Severity: high | medium | low
  File: <path to workflow file>
  Location: line <N> (or "top-level" / "job: <job-id>")
  Finding: <what is wrong>
  Recommendation: <what to do>
```

**Severity guidance:**

- **high** — missing permissions (security exposure)
- **medium** — outdated action with a known deprecation
  warning or breaking change in a newer version
- **low** — outdated action with no known issues, or a
  Node.js deprecation warning that does not yet break CI
