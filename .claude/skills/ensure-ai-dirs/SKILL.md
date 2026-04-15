---
name: ensure-ai-dirs
description: >
  Ensure the configured plans and memory directories exist,
  and sync the plan format guide. Run once before planning
  begins.
---

# /ensure-ai-dirs

Prepare the `.ai/` directories before writing any plan
files or storing memories. The lead writes plans to the
configured plans directory and consults its `CLAUDE.md`
for the required format — without the directory, the
CLAUDE.md pointer, and the format guide, the planning
flow breaks. The memory directory must exist for Claude
Code's auto-memory system to persist memories across
sessions.

**All steps below are mandatory — execute every step,
every time.** Do not skip step 2 because the directory
already exists or the format guide appears current. Do not
skip step 3 because the memory directory already exists.

## Steps

1. **Read settings** — read `.claude/settings.json` and
   extract both `plansDirectory` and `autoMemoryDirectory`.

   If either key is **absent**: the blueprint expects both
   directories configured. Silently defaulting would leave
   the configuration missing for future sessions and other
   agents. Instead:

   a. Read `.claude/settings.local.json` if it exists
      (it may contain other local overrides that must be
      preserved).
   b. Add the missing key(s) to the parsed object (or
      create a new object if the file does not exist):
      - `"plansDirectory": ".ai/plans/"` if absent
      - `"autoMemoryDirectory": ".ai/memory/"` if absent
   c. Write the result back to `.claude/settings.local.json`.
   d. Report which keys were not configured and have been
      set in `settings.local.json` — the lead must relay
      this to the user so they can move the settings to
      `settings.json` if they want them version-controlled.

   Using `settings.local.json` (not `settings.json`)
   avoids modifying the checked-in blueprint configuration.
   Claude Code merges both files at startup, so the
   settings take effect immediately.

2. **Sync the plans directory files** — sync two files
   from `.claude/skills/ensure-ai-dirs/` to
   `<plansDirectory>`. Always read both source and target
   and compare them:

   a. **Plan format guide** — read the canonical template
      from `.claude/skills/ensure-ai-dirs/plan-format.md`.
      Read `<plansDirectory>/plan-format.md` if it exists.
      If the file does not exist or its content differs
      from the template, write the template to
      `<plansDirectory>/plan-format.md` using Write.

   b. **Plans CLAUDE.md** — read the template from
      `.claude/skills/ensure-ai-dirs/claude-md-template.md`.
      Read `<plansDirectory>/CLAUDE.md` if it exists. If
      the file does not exist or its content differs from
      the template, write the template to
      `<plansDirectory>/CLAUDE.md` using Write.

   c. Report whether updates were written or the files
      were already identical.

   This step is unconditional — execute it every time,
   even if the files appear current. The CLAUDE.md is
   intentionally slim — it points agents to plan-format.md
   rather than embedding the full format guide, so agents
   reading plans do not load the format guide into their
   context unnecessarily. Only the agent writing plans
   reads plan-format.md on demand.

3. **Ensure the memory directory exists** — create
   `<autoMemoryDirectory>` if it does not exist. No format
   guide is needed — Claude Code manages memory files
   directly. Report whether the directory was created or
   already existed.
