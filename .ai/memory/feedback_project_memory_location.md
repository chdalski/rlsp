---
name: Project memory location
description: Store all memory for this project in the git-tracked .ai/memory/, not home-dir memory; /ensure-ai-dirs points auto-memory there via settings.local.json
type: feedback
originSessionId: 1ec2c7dc-e29d-4a65-8486-e9d5df8df19d
---
Store all memory for this project — feedback, follow-up items, design decisions, pending work — in `.ai/memory/` in the workspace, not in the home-dir memory (`~/.claude/projects/-workspace/memory/`).

**Why:** Project context belongs in the repo where it's visible to all sessions and part of project history. Claude Code honors only an absolute or `~/` `autoMemoryDirectory`; the old relative `.ai/memory/` value in `settings.json` was silently ignored, so feedback memories piled up in the home-dir directory until they were migrated here on 2026-09-15.

**How to apply:** `/ensure-ai-dirs` writes the absolute path of `.ai/memory/` as `autoMemoryDirectory` into `.claude/settings.local.json` (gitignored, per machine) on every run. If a session's memory directory is still the home-dir one — e.g. a fresh checkout before the skill has run — write project memories to `.ai/memory/` anyway and add them to its `MEMORY.md` index.
