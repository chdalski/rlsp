**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-18

## Goal

Fix the destructive `flow_map_to_block` and
`flow_seq_to_block` quick-fix code actions in
`rlsp-yaml/src/editing/code_actions.rs`. Applying the
"Convert flow mapping to block style" or "Convert flow
sequence to block style" quick fix currently produces
broken YAML on several legitimate inputs (full-line
replace destroys surrounding content; single-line scope
cannot handle multi-line flow collections;
key-reconstruction fragility when the prefix doesn't
end in `:`).

This is a stub plan filed as the lead pre-execution
step required by
`.ai/plans/2026-04-18-corpus-invariants-scaffold.md`
— specifically Task 3's skip-list entries must cite a
concrete plan file path for failures caused by these
code actions, and this stub provides that path.

## Context

The destructive behavior was traced during
investigation of the originating GHA-expression bug:
for an input like `          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}`,
applying the quick fix wiped the `GITHUB_TOKEN:` key
and produced a line of 27 spaces followed by
`{ secrets.GITHUB_TOKEN }`.

Once Move 1
(`.ai/plans/2026-04-18-one-parser-one-ast.md`) lands,
the false-positive `flowMap` diagnostic on `${{ … }}`
will stop firing, so the code action will no longer be
triggered on that input. However, the latent defects
inside `flow_map_to_block` / `flow_seq_to_block`
(full-line replace, single-line scope,
key-reconstruction fragility) remain and will still
produce broken output on legitimate flow collections
— e.g. multi-line flow mappings in corpus files.

This stub will be expanded into a proper plan with full
Context, Tasks, Decisions, and References once Move 1
completes and we have a clear view of what's left in
the code-action bug surface.

### Relationship to other plans

- Prerequisite: `.ai/plans/2026-04-18-one-parser-one-ast.md`
  (Move 1). Must land first so the false-positive
  diagnostic class is eliminated and we can see the
  code action's remaining defects in isolation.
- Depends on: `.ai/plans/2026-04-18-corpus-invariants-scaffold.md`
  (corpus-invariant harness). I3 (code-action
  round-trip) exercises the code action across the
  corpus; this plan removes the resulting skip-list
  entries as it lands.

## Steps

- [ ] File proper plan content (Goal, Context, Tasks,
      Decisions, References) once Move 1 completes.
      At minimum, the expanded plan must cover:
  - Root-cause analysis of the full-line replace,
    single-line scope, and key-reconstruction
    fragility defects
  - A chosen fix approach (tighten replace range +
    guard rails vs. full AST-subtree re-serialize via
    formatter)
  - Test coverage for each defect class
  - Removal of the corresponding skip-list entries
    in the corpus-invariants harness

## Tasks

Tasks will be filled in when the plan is expanded.

## Decisions

- **Stub exists solely to provide a referable plan
  path.** The corpus-invariants plan's skip-list
  discipline forbids ad-hoc TODO markers; every entry
  must cite a filed plan. This stub satisfies that
  discipline while the actual fix work waits behind
  Move 1.
