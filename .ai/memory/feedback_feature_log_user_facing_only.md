---
name: feature-log.md is user-facing docs only
description: Do not add feature-log entries for internal refactors, retrofits, or implementation-only changes in rlsp-yaml
type: feedback
originSessionId: 4a7f02e4-d091-41a4-8fa9-b3aedb4e51d0
---
In the `rlsp-yaml` project, `rlsp-yaml/docs/feature-log.md`
was created for USER-FACING feature decisions — things a
user of the language server would notice. It is published
documentation (lives under `docs/`), not an internal
changelog.

Do NOT add feature-log entries for:

- Internal refactors (e.g., "retrofit X to AST+formatter")
- Code-action rewrites that don't change user-visible
  behavior (title strings, quickfix output, what fires)
- Test infrastructure changes
- Audit / boundary-check additions
- Memory or plan-file updates

Internal changes belong in git history (commit messages)
and in the plan file. The feature-log is reserved for
things a user reading the docs would want to see — new
LSP features, behavior changes that affect their editor
experience, settings that unlock new UX.

**Why:** A feature log full of internal retrofits dilutes
the signal for users who want to know what's actually new
that they can use.

**How to apply:** When writing a plan for an internal
refactor, do NOT include a sub-task to update
`feature-log.md`. When reviewing a plan, flag any such
sub-task for removal. If the change IS user-facing (new
setting, new diagnostic users will see, new code-action
target style), a feature-log entry is appropriate.

Existing internal entries added in earlier sessions (e.g.
"AST-Based X Code Action" entries for string_to_block_scalar,
block_to_flow, flow_map_to_block, validate_flow_style,
corpus_invariants harness) should be removed in a future
cleanup pass. Tracked in `project_followup_plans.md`.
