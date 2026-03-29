---
name: project-sanity
description: >
  Audit the repository for common issues across detected
  technologies. Reports findings without fixing — the user
  decides what to act on.
---

# /project-sanity

Audit the repository for common issues by detecting which
technologies are present, running the corresponding sanity
checks, and presenting all findings to the user.

This skill is **audit-only** — it reports findings but does
not fix anything. The user decides what to act on after
seeing the results.

## Detection Table

Scan the repository root for these indicators and map each
to a check file:

| Indicator | Check File |
|---|---|
| `.github/` directory exists | `github-sanity.md` |
| `codecov.yml` or `codecov.yaml` exists at repo root | `codecov-sanity.md` |

## Steps

1. **Detect technologies** — scan the repository root for
   each indicator in the detection table. Record which
   indicators are present and which check files to run.

2. **Run check files** — for each detected check file, read
   `.claude/skills/project-sanity/<check-file>` and execute
   its instructions. Collect all findings it reports.

3. **Aggregate findings** — combine findings from all
   executed check files into a single list. Preserve the
   source check file for each finding so the user knows
   which technology each finding belongs to.

4. **Present findings** — report all findings to the user,
   grouped by technology. For each finding include what was
   found and why it matters. Then ask the user what should
   be done about each finding.
