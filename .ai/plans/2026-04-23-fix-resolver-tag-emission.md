**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-23

# Fix resolver-injected tag emission in formatter and corpus invariant

## Goal

Fix two bugs introduced by the default-Core schema change
(`073f128`): the formatter emits `!!null`/`!!str` for empty
scalars that only carry resolver-injected tags, producing
non-idempotent output that can fail to re-parse; and the I6
corpus invariant rejects resolver-injected tags that have
`tag_loc: None`. Both bugs cause test failures on `main`
(4 formatter conformance + 4 corpus invariant).

## Context

- **Incident report:**
  `.ai/reports/2026-04-23-resolver-tag-emission-in-formatter.md`
  documents the full analysis.
- **Formatter tag filtering:** `formatter.rs` already has
  `is_core_schema_tag()` (line 707-709) that suppresses
  core schema tags (`tag:yaml.org,2002:*`) for non-empty
  scalars and collections. The **empty-scalar branch**
  (lines 536-555) is the exception — it converts core
  schema tags to short form (`!!null`, `!!str`) and emits
  them. Before the default change, empty scalars had
  `tag: None` so this branch never fired for
  resolver-injected tags.
- **Failing formatter tests:** `formatter_conformance` paths
  070 (6KGN: anchor for empty node — non-idempotent), 079
  (6XDY: two document start markers — re-parse fails), 213
  (JEF9), 225 (K858).
- **I6 invariant:** `corpus_invariants.rs` line 547 asserts
  `tag.is_some() == tag_loc.is_some()`. Resolver-injected
  tags have `tag: Some(...)` with `tag_loc: None` by design
  (no source position for resolved tags). The invariant
  predates schema resolution.
- **Failing I6 tests:** All 4 corpus files (docker-compose,
  github-actions-matrix, kubernetes-deployment,
  release-plz-workflow).
- **`tag_loc` as discriminator:** `tag_loc: Some(...)` means
  the user wrote the tag in the source (explicit tag);
  `tag_loc: None` with `tag: Some(...)` means
  resolver-injected. This is the correct signal for both
  fixes.

## Steps

- [ ] Task 1 — fix formatter empty-scalar tag emission and
      I6 corpus invariant

## Tasks

### Task 1: Fix formatter empty-scalar tag emission and I6 corpus invariant

Both fixes are small, tightly coupled (same root cause),
and belong in one commit.

- [ ] In `editing/formatter.rs` (lines 536-555): change the
      empty-scalar branch of the `tag_prefix` logic to
      suppress core schema tags for empty scalars, matching
      the existing behavior for non-empty scalars and
      collections. When `value.is_empty()` and
      `is_core_schema_tag(t)`, return `None` instead of
      emitting `!!<suffix>`. The resolver re-injects the tag
      on the next `load()`, so emitting it is redundant and
      breaks idempotency.
- [ ] Verify the formatter still emits explicit user tags on
      empty scalars (non-core-schema tags like `!custom` or
      `!<tag:example.com:shape>`). The `else` branch already
      handles this — confirm with a test case.
- [ ] In `corpus_invariants.rs` (line 547): update the I6
      assertion to allow resolver-injected tags without
      `tag_loc`. Resolver-injected tags start with
      `"tag:yaml.org,2002:"` — when the tag matches that
      prefix and `tag_loc` is `None`, the invariant holds.
      The assertion should only fail when a non-resolver tag
      has mismatched `tag`/`tag_loc` presence.
- [ ] Update the I6 invariant description string in the
      `INVARIANTS` array to reflect the narrowed assertion
      (e.g., "tag_loc invariant: explicit tags have tag_loc;
      resolver-injected core-schema tags may have
      tag_loc: None").
- [ ] Confirm whether the 4 I6 failures are currently in
      the `SKIP_LIST` constant in `corpus_invariants.rs`.
      If they are, remove those entries and update
      `WORKLIST.md` to match. If no skip-list entries exist
      (failures are live), no action needed.
- [ ] All 4 formatter conformance failures pass: paths 070
      (6KGN), 079 (6XDY), 213 (JEF9), 225 (K858).
- [ ] All 4 corpus invariant I6 checks pass.
- [ ] No regressions: `cargo test --workspace` passes with
      zero failures.
- [ ] Append a "Resolved" note to the incident report at
      `.ai/reports/2026-04-23-resolver-tag-emission-in-formatter.md`
      with the fix commit SHA so future readers know the
      issue is closed.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets`
      run clean.

## Decisions

- **Suppress, don't rewrite.** The formatter should not emit
  resolver-injected tags at all — suppressing them is
  correct because the resolver re-injects them on the next
  load. Emitting them adds redundant information that can
  break round-trip behavior.
- **One task, one commit.** Both fixes share the same root
  cause (resolver-injected tags lack `tag_loc`) and are
  small enough to review together. Splitting into two
  commits adds overhead without benefit.
- **`tag_loc` is the discriminator.** Rather than checking
  the tag prefix string, the formatter could also use
  `tag_loc.is_none()` to detect resolver-injected tags.
  However, `is_core_schema_tag()` already exists in the
  formatter and is the established pattern — use it for
  consistency. The I6 invariant uses the prefix check for
  the same reason.
