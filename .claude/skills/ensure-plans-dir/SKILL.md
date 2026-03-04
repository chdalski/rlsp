---
name: ensure-plans-dir
description: >
  Ensure the configured plans directory and its format
  guide exist before writing a plan. Run once before
  Phase 1 planning.
---

# /ensure-plans-dir

Prepare the plan directory before writing any plan files.
The Architect writes plans to the configured plans directory
and consults its `CLAUDE.md` for the required format —
without both, the planning flow breaks.

## Steps

1. **Find the plans directory** — read `.claude/settings.json`
   and extract `plansDirectory`. If the key is absent,
   default to `.ai/plans/`. This respects the project's
   configured location rather than assuming a fixed path.

2. **Write the format guide** — read the canonical template
   from `.claude/templates/plan-format.md` and write it to
   `<plansDirectory>/CLAUDE.md` using Write — this creates
   the directory if needed. Do not modify the template
   content. Always overwriting ensures the format guide is
   current and plan naming conventions are never stale.

3. **Read `<plansDirectory>/CLAUDE.md`** — load the format
   guide into context. Plans must follow this format so the
   lead and future sessions can parse them without guessing
   at conventions.
