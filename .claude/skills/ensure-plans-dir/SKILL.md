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

**All three steps below are mandatory — execute every step,
every time.** Do not skip step 2 or 3 because the directory
already exists or the format guide appears current. The
template may have changed since the last run, and skipping
the overwrite causes plans to follow a stale format. This
has caused real drift in production sessions.

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

2. **Write the format guide (always — not conditional)** —
   read the canonical template from
   `.claude/skills/ensure-plans-dir/plan-format.md` and
   write it to `<plansDirectory>/CLAUDE.md` using Write —
   this creates the directory if needed. Do not modify the
   template content. **Always overwrite, even if the file
   already exists** — the template may have changed since
   the last run, and only an unconditional write guarantees
   the deployed guide matches the current blueprint.

3. **Read `<plansDirectory>/CLAUDE.md`** — load the format
   guide into context. Plans must follow this format so
   future sessions can parse them without guessing at
   conventions.
