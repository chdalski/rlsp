**Repository:** root
**Status:** Completed (2026-03-27)
**Created:** 2026-03-26

## Goal

Create community files (CONTRIBUTING, Code of Conduct,
Security policy) and GitHub issue/PR templates that
reflect the project's AI-written, issues-only contribution
model.

## Context

- The project accepts issues (bugs + feature requests),
  not external PRs or patches
- All code is AI-written — this is a defining project
  characteristic
- License: MIT, copyright Christoph Dalski
- Repository: https://github.com/cdalski/rlsp
- Code of Conduct: Contributor Covenant v2.1

## Steps

- [x] Create CONTRIBUTING.md (f16903c)
- [x] Create CODE_OF_CONDUCT.md (df467c3)
- [x] Create SECURITY.md (df467c3)
- [x] Create bug report issue template (f16903c)
- [x] Create feature request issue template (f16903c)
- [x] Create issue template chooser config (f16903c)
- [x] Create PR template (f16903c)
- [x] Document recommended GitHub labels (f16903c, in CONTRIBUTING.md)

## Tasks

### Task 1: Community files

**CONTRIBUTING.md** — explain the AI-written model:
- State that all code is AI-authored
- Contributions are welcome as GitHub issues
- Bug reports: describe the problem, steps to reproduce,
  expected vs actual behavior
- Feature requests: describe the use case, not the
  implementation
- PRs from external contributors are not accepted
- Link to issue templates

**CODE_OF_CONDUCT.md** — Contributor Covenant v2.1
(standard text, adapted with contact email/method)

**SECURITY.md** — vulnerability reporting:
- Do NOT file security issues as public GitHub issues
- Report via GitHub's private security advisory feature
  (Settings > Security > Advisories)
- Expected response timeline
- Scope: rlsp-yaml binary, dependencies

- [x] CONTRIBUTING.md (f16903c)
- [x] CODE_OF_CONDUCT.md (df467c3)
- [x] SECURITY.md (df467c3)

### Task 2: GitHub issue and PR templates

**Bug report template** (`.github/ISSUE_TEMPLATE/bug_report.yml`):
- YAML-based form template (not markdown)
- Fields: description, steps to reproduce, expected
  behavior, actual behavior, rlsp-yaml version, editor,
  OS, YAML sample (optional)
- Label: `bug`

**Feature request template** (`.github/ISSUE_TEMPLATE/feature_request.yml`):
- Fields: description of the use case, proposed behavior,
  alternatives considered (optional), YAML sample (optional)
- Label: `enhancement`

**Issue template chooser** (`.github/ISSUE_TEMPLATE/config.yml`):
- List bug report and feature request as options
- Add external link for general questions/discussions

**PR template** (`.github/PULL_REQUEST_TEMPLATE.md`):
- Short message explaining that this project does not
  accept external PRs
- Redirect to filing an issue instead
- Keep it friendly — people may not read CONTRIBUTING.md
  before submitting

- [x] Bug report issue template (f16903c)
- [x] Feature request issue template (f16903c)
- [x] Issue template chooser config (f16903c)
- [x] PR template (f16903c)

### Task 3: Recommended GitHub labels

Document a set of labels the maintainer should create in
the GitHub repository settings. Include in CONTRIBUTING.md
or as a separate note:

| Label | Color | Description |
|-------|-------|-------------|
| `bug` | `#d73a4a` | Something isn't working |
| `enhancement` | `#a2eeef` | New feature or request |
| `question` | `#d876e3` | Further information needed |
| `accepted` | `#0e8a16` | Issue accepted for implementation |
| `wontfix` | `#ffffff` | Will not be addressed |
| `duplicate` | `#cfd3d7` | Duplicate of another issue |
| `good first issue` | `#7057ff` | Good for newcomers to file |

- [x] Label table in CONTRIBUTING.md (f16903c)

## Decisions

- **YAML form templates over markdown** — form templates
  provide structured fields that ensure reporters include
  necessary information. Less freeform than markdown
  templates.
- **PR template as redirect** — rather than disabling PRs
  entirely (which GitHub doesn't support natively), use
  a template that explains the issues-only model. This is
  friendlier than a bot closing PRs.
- **Contributor Covenant v2.1** — most widely adopted CoC
  in open source. Covers issue tracker and community
  interactions, which is the relevant scope for an
  issues-only project.
