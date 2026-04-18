# Corpus Invariant Worklist

This file is the human-readable mirror of the `SKIP_LIST` constant in
`rlsp-yaml/tests/corpus_invariants.rs`. **The Rust constant is the source
of truth** — the test enforces it at every CI run. This file exists so
reviewers and follow-up-plan authors can scan the current failure set
without reading Rust source.

## Skip-list discipline

The skip-list is **shrink-only**. Entries are removed as follow-up plans
fix the root causes. New entries may only be added when a NEW corpus file
surfaces a known-fixable issue that already has a follow-up plan filed;
never to silence a surprise failure.

Adding a skip-list entry without a filed follow-up plan reference is
forbidden. When a surprise failure occurs, the developer reports it to the
lead (via SendMessage), waits for a plan to be filed, and only then adds
the entry — citing that plan's file path. This is the **Surprise Failure
Protocol**.

## Current failures

| File | Invariant | Plan |
|------|-----------|------|

---

*An empty list here means the harness is fully green — not that it is
unused. The discipline is cheaper to preserve than to re-establish.*
