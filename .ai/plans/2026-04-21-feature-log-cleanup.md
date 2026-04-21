**Repository:** root
**Status:** Completed (2026-04-21)
**Created:** 2026-04-21

## Goal

Remove internal-refactor entries from
`rlsp-yaml/docs/feature-log.md` per the file's own scope
note. The feature log is user-facing documentation —
entries describe changes a user of the language server
would notice. Five entries added during the AST-first
retrofit program (2026-04-18 onward) describe implementation
rewrites and test infrastructure, not user-observable
features, and violate the file's stated scope.

## Context

- `rlsp-yaml/docs/feature-log.md` carries a hidden
  HTML-comment scope note at the top of the file that
  excludes: (a) internal refactors (e.g., "retrofit X to
  AST+formatter"); (b) implementation rewrites that don't
  change user-visible behavior; (c) test infrastructure,
  audits, boundary-check additions; (d) memory or plan-file
  updates.
- The root `CLAUDE.md` reinforces this convention:
  "`docs/feature-log.md` is user-facing feature decisions
  only — internal refactors and implementation rewrites do
  NOT go there (commit history + plan files carry that
  record)."
- The five entries to remove are, by current title:
  1. "AST-Based String-to-Block-Scalar Code Action"
  2. "AST-Based Block-to-Flow Code Action"
  3. "AST-Based Flow-to-Block Quick Fixes"
  4. "AST-Based Flow Style Validator"
  5. "Corpus Invariant Harness"
- The first four are rewrites from text-surgery to AST+
  formatter — pure implementation changes with the same
  quickfix titles, diagnostic codes, severities, and
  message text as before. The fifth is a test harness
  (`rlsp-yaml/tests/corpus_invariants.rs`) — test
  infrastructure, explicitly on the scope-note exclusion
  list.
- Two of the four retrofit entries (Flow-to-Block,
  Flow Style Validator) contain user-observable bug-fix
  notes embedded in the descriptions. This plan removes
  the entries in full; see Decisions.
- Follow-up queue pointer:
  `.ai/memory/project_followup_plans.md` carries a bullet
  titled "Clean internal-refactor entries out of
  `rlsp-yaml/docs/feature-log.md`". That bullet is removed
  as part of this plan so the memory file does not carry a
  stale open-item pointer after the cleanup lands.

## Steps

- [x] Remove the 5 internal-refactor entries from
      `rlsp-yaml/docs/feature-log.md`
- [x] Remove the corresponding bullet from
      `.ai/memory/project_followup_plans.md`
- [x] Verify the remaining file structure is intact and
      the build is green

## Tasks

### Task 1: Remove internal-refactor entries and queue pointer

Delete the five internal-refactor entries from
`rlsp-yaml/docs/feature-log.md` and the matching pointer
in the follow-up queue.

- [x] Delete the entry titled "AST-Based String-to-Block-
      Scalar Code Action" in full (`###` heading,
      `**Description:**`, `**Complexity:**`, `**Comment:**`,
      `**Tier:**`, and the blank line separating it from
      the next entry)
- [x] Delete the entry titled "AST-Based Block-to-Flow
      Code Action" in full
- [x] Delete the entry titled "AST-Based Flow-to-Block
      Quick Fixes" in full
- [x] Delete the entry titled "AST-Based Flow Style
      Validator" in full
- [x] Delete the entry titled "Corpus Invariant Harness"
      in full, including its inline link to
      `../tests/corpus/WORKLIST.md`
- [x] Delete the bullet starting
      "**Clean internal-refactor entries out of
      `rlsp-yaml/docs/feature-log.md`**" from
      `.ai/memory/project_followup_plans.md`
- [x] Grep the repository for other references to the
      removed entry titles (`grep -rn "AST-Based
      String-to-Block-Scalar\|AST-Based Block-to-Flow\|
      AST-Based Flow-to-Block\|AST-Based Flow Style\|
      Corpus Invariant Harness"` in `rlsp-yaml/` and
      repository root); confirm the only remaining hits
      are in plan files under `.ai/plans/` (historical
      record, expected) and in this plan
- [x] Run `cargo test -p rlsp-yaml` and
      `cargo clippy --all-targets`

**Commit:** `c8d1580`

Acceptance criteria:
- `rlsp-yaml/docs/feature-log.md` contains zero `###`
  headings matching any of the five deleted titles
- `rlsp-yaml/docs/feature-log.md` contains zero entries
  whose body uses "retrofit" or "text-surgery" to describe
  an implementation change — remaining uses of those words
  are acceptable only in other contexts (user-visible
  rationale, historical explanation of a kept entry)
- `rlsp-yaml/docs/feature-log.md` contains zero links to
  `../tests/corpus/WORKLIST.md`
- `.ai/memory/project_followup_plans.md` contains zero
  bullets whose first bold phrase is "Clean internal-
  refactor entries out of `rlsp-yaml/docs/feature-log.md`"
- The file's scope note (HTML comment at the top),
  tier legend, and all non-removed entries are unchanged
- `cargo test -p rlsp-yaml` passes
- `cargo clippy --all-targets` reports zero warnings
- Git diff of `rlsp-yaml/docs/feature-log.md` shows only
  deletions — no additions, no reformatting of retained
  entries

## Decisions

- **Removal only, no reframing.** The Flow-to-Block and
  Flow Style Validator entries contain user-observable
  bug-fix notes embedded in otherwise-internal
  descriptions (e.g., "`${{ secrets.GITHUB_TOKEN }}` no
  longer produces false-positive `flowMap`/`flowSeq`
  diagnostics"). This plan does not reframe them as new
  user-facing entries. The follow-up queue item scopes
  this work as cleanup, not re-authoring. If the project
  later wants user-facing entries for those bug fixes,
  they can be added in separate commits after this plan
  lands.
- **Queue-pointer removal in the same commit.** Leaving
  the "clean feature-log" bullet in
  `project_followup_plans.md` after the cleanup lands
  would create stale state. Bundling the removal into this
  task keeps the memory file consistent with the
  repository state without requiring a separate follow-up.
- **No separator cleanup needed.** The file uses three
  `---` section separators (after the tier legend, after
  the JSON Schema `contentEncoding` entry, and after the
  `$vocabulary` entry). None of them border on a removed
  entry in a way that would leave a stray double-separator
  or a separator with no entries on one side. The removals
  land cleanly inside a single section.

## Non-Goals

- Reframing or rewriting any removed entry as a new
  user-facing entry
- Auditing or editing other docs (README.md,
  `docs/configuration.md`, per-crate READMEs) for
  internal-refactor leakage
- Changing the scope note, tier legend, or any other
  convention in `feature-log.md`
- Editing or removing the plan files under `.ai/plans/`
  that describe the retrofit work — plan files are the
  authoritative record of internal changes and must stay
  intact as committed
