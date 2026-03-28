---
paths:
  - ".github/workflows/**"
---

# GitHub Workflows

These rules activate when creating or modifying GitHub
Actions workflow files. They address two recurring failure
modes seen in this repository: stale action versions and
missing permissions.

## Action Version Currency

When creating a new workflow or modifying an existing one,
always use the latest stable version of each action used.
When touching an existing workflow for any reason — even a
one-line change — check whether the actions it uses have
newer versions available and update them.

Pin actions to major version tags (e.g., `@v6`), not SHA
commits. Major version tags are readable, convey intent,
and pick up patch and minor updates automatically within
the stable API contract.

**Why:** Stale actions accumulate deprecation warnings and
eventually break when the underlying runtime (e.g.,
Node.js) is removed. Updating at touch-time keeps the
codebase current incrementally instead of accumulating a
large upgrade batch.

## Explicit Permissions

Every workflow must declare explicit `permissions` blocks
following the principle of least privilege. Use either a
top-level `permissions:` block that applies to all jobs,
or per-job `permissions:` blocks when jobs have different
needs. Never rely on repository or organization defaults.

Grant only the permissions a job actually uses. Common
examples:

```yaml
permissions:
  contents: read        # read the repository
  contents: write       # create releases, push commits
  pull-requests: write  # comment on PRs
  id-token: write       # OIDC token exchange (e.g., crates.io publish)
```

**Why:** Repository and organization defaults can change.
Explicit permissions document exactly what each workflow
needs and protect against future default changes that
would silently grant or revoke access. Code scanning
flags workflows without explicit permissions as a security
alert (alerts #1–4 in this repository were all
missing-permissions findings).
