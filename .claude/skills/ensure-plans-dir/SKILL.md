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

**Both steps below are mandatory — execute every step,
every time.** Do not skip step 2 because the directory
already exists or the format guide appears current.

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

2. **Sync the format guide** — always read both files and
   compare them:

   a. Read the canonical template from
      `.claude/skills/ensure-plans-dir/plan-format.md`.
   b. Read `<plansDirectory>/CLAUDE.md` if it exists.
   c. If the file does not exist or its content differs
      from the template, write the template to
      `<plansDirectory>/CLAUDE.md` using Write.
   d. Report whether an update was written or the files
      were already identical.

   This step is unconditional — execute it every time,
   even if the format guide appears current. A previous
   session may have written the guide from a stale
   template, and the only way to detect drift is to
   compare against the canonical source. Plans must follow
   this format so future sessions can parse them without
   guessing at conventions.
