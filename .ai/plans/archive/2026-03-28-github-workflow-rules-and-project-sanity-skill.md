**Repository:** root
**Status:** Completed (2026-03-28)
**Created:** 2026-03-28

## Goal

Create a rules file for GitHub Actions workflows and a
project-sanity skill that audits the repository for common
issues across detected technologies. The workflow rule
prevents recurring problems with outdated action versions
and missing permissions. The project-sanity skill provides
a reusable audit mechanism that dispatches to domain-specific
check files and reports findings without auto-fixing.

## Context

**Recurring problems this addresses:**
- Code scanning alert #4 (open): `release-plz.yml`
  `filter-binaries` job missing `permissions` block
- Actions run 23668614828 warnings: `softprops/action-gh-release@v2`
  uses deprecated Node.js 20, needs update before June 2026
- Prior alerts (#1-3, now fixed) were also missing-permissions
  issues in `ci.yml` and `coverage.yml`

**Design decisions:**
- The rules file (`.claude/rules/github-workflows.md`) uses
  `paths:` frontmatter targeting `.github/workflows/**` so
  it auto-activates when agents touch workflow files
- The project-sanity skill is a dispatcher: it detects what
  technologies/platforms exist in the repo, then calls
  domain-specific sanity check files for each
- Sanity check files audit and report findings — they do NOT
  fix anything. The skill collects all findings and presents
  them to the user, who decides what to act on
- `github-sanity.md` is the first domain check file,
  covering action version currency and permissions

**Key files involved:**
- `.claude/rules/github-workflows.md` — new rules file
- `.claude/skills/project-sanity/SKILL.md` — new skill entry
- `.claude/skills/project-sanity/github-sanity.md` — GitHub
  domain check file

**Existing patterns to follow:**
- Rules: `.claude/rules/documentation.md` (uses `paths:`
  frontmatter), `.claude/rules/lang-rust.md`
- Skills: `.claude/skills/project-init/SKILL.md` (dispatcher
  with detection tables and steps)

## Steps

- [x] Clarify requirements with user
- [x] Review existing code scanning alerts and action warnings
- [x] Analyze existing rules and skill patterns
- [x] Create GitHub workflows rules file (718d640)
- [x] Create project-sanity skill (SKILL.md dispatcher) (a4c6e19)
- [x] Create github-sanity.md domain check file (388f3e8)

## Tasks

### Task 1: Create GitHub workflows rules file

Write `.claude/rules/github-workflows.md` with `paths:`
frontmatter targeting `.github/workflows/**/*.yml`. The rule
must cover:

- **Action version currency** — when creating or modifying
  a workflow, use the latest stable version of each action.
  When touching an existing workflow, check whether its
  actions have newer versions available and update them.
- **Explicit permissions** — every workflow must have a
  top-level or per-job `permissions` block following
  least-privilege. Never rely on repository/org defaults.

Follow the style of existing rules files (concise, direct,
explain "why" not just "what"). Include the `paths:`
frontmatter so the rule auto-activates.

Files: `.claude/rules/github-workflows.md`

### Task 2: Create project-sanity skill

Write `.claude/skills/project-sanity/SKILL.md` — a
dispatcher skill that:

1. Scans the repository to detect what's present (e.g.,
   `.github/` directory, `Cargo.toml`, `package.json`, etc.)
2. For each detected technology/platform, reads and executes
   the corresponding sanity check file from
   `.claude/skills/project-sanity/`
3. Collects all findings from all check files
4. Presents the combined findings to the user and asks what
   should be done about each finding

The detection table maps directory/file presence to check
files:

| Indicator | Check File |
|---|---|
| `.github/` directory | `github-sanity.md` |

(Additional check files can be added later for Rust, Python,
etc. — YAGNI, only GitHub is needed now.)

The skill must be clear that check files audit only — they
report findings, they do not modify files.

Files: `.claude/skills/project-sanity/SKILL.md`

### Task 3: Create github-sanity.md domain check file

Write `.claude/skills/project-sanity/github-sanity.md` — the
GitHub-specific sanity check that audits:

1. **Action version currency** — for each `uses:` line in
   workflow files, check whether a newer version exists.
   Report outdated actions with current version and latest
   available version.
2. **Workflow permissions** — check that every workflow has
   explicit `permissions` blocks (top-level or per-job).
   Report any workflows or jobs missing permissions.
3. **Node.js deprecation warnings** — flag actions known to
   use deprecated Node.js versions.

The check file must output a structured findings list that
the dispatcher can collect. It does NOT fix anything — it
reports what it found.

Files: `.claude/skills/project-sanity/github-sanity.md`

## Decisions

- **Rules file uses `paths:` frontmatter** — auto-activates
  only when workflow files are touched, avoids noise in
  unrelated contexts
- **Skill is a dispatcher pattern** — extensible to new
  domains (Rust, Docker, etc.) without modifying the core
  skill
- **Sanity checks are audit-only** — the user decides what
  to fix, preventing unwanted automatic changes
- **Only github-sanity.md for now** — YAGNI, other domains
  can be added when needed
