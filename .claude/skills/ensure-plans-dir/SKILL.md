---
name: ensure-plans-dir
description: >
  Ensure the configured plans directory and its format
  guide exist before writing a plan. Run once before
  planning begins.
---

# /ensure-plans-dir

Prepare the plan directory before writing any plan files.
The lead writes plans to the configured plans directory
and consults its `CLAUDE.md` for the required format —
without both, the planning flow breaks.

## Steps

1. **Find the plans directory** — read `.claude/settings.json`
   and extract `plansDirectory`.

   If the key is **absent**: the blueprint's plan templates
   and format guide expect a configured directory. Silently
   defaulting would leave the configuration missing for
   future sessions and other agents. Instead:

   a. Read `.claude/settings.local.json` if it exists
      (it may contain other local overrides that must be
      preserved).
   b. Add `"plansDirectory": ".ai/plans/"` to the parsed
      object (or create a new object if the file does not
      exist).
   c. Write the result back to `.claude/settings.local.json`.
   d. Report that `plansDirectory` was not configured and
      has been set to `.ai/plans/` in `settings.local.json`
      — the lead must relay this to the user so they can
      move it to `settings.json` if they want the setting
      version-controlled.

   Using `settings.local.json` (not `settings.json`)
   avoids modifying the checked-in blueprint configuration.
   Claude Code merges both files at startup, so the setting
   takes effect immediately.

2. **Write the format guide** — read the canonical template
   from `.claude/skills/ensure-plans-dir/plan-format.md`
   and write it to `<plansDirectory>/CLAUDE.md` using
   Write — this creates the directory if needed. Do not
   modify the template content. Always overwriting ensures
   the format guide is current and plan naming conventions
   are never stale.

3. **Read `<plansDirectory>/CLAUDE.md`** — load the format
   guide into context. Plans must follow this format so
   future sessions can parse them without guessing at
   conventions.
