**Repository:** root
**Status:** Completed (2026-04-16)
**Created:** 2026-04-16

## Goal

In Lossless loader mode (the default), the loader clones
every anchored node's full subtree into `anchor_map` even
though Lossless `resolve_alias` never reads the map. On
anchor-heavy documents — Kubernetes manifests, CI
configuration, multi-environment YAML — this is a
recursive clone per anchored collection, a measurable
memory and CPU cost paid for nothing. Change
`register_anchor` to accept `&Node<Span>` and clone only
when `mode == Resolved`, and change the three call sites
in `parse_node` to pass a borrow instead of a clone. The
caller-side `node.clone()` is what makes the overhead
O(subtree_size) per anchor; eliminating it is the real
win.

## Context

- `rlsp-yaml-parser/src/loader.rs:656–678` defines
  `register_anchor(name: String, node: Node<Span>)`. Its
  final statement,
  `self.anchor_map.insert(name, node);`, unconditionally
  stores the node. Three callers pass an eager
  `node.clone()`:
  - `:429–431` — Scalar path
  - `:528–530` — Mapping path (root of a whole subtree
    for anchored block mappings)
  - `:623–625` — Sequence path (root of a whole subtree
    for anchored block sequences)
- `resolve_alias` (loader.rs `:680–698`):
  - Lossless mode returns
    `Node::Alias { name, loc, leading_comments,
    trailing_comment }` without touching `anchor_map`.
  - Resolved mode reads `self.anchor_map.get(name)` and
    then invokes `expand_node` recursively.
- `register_anchor`'s uniqueness tracking (for the
  `max_anchors` limit) uses
  `self.anchor_map.contains_key(&name)`. Skipping the
  insert entirely would break uniqueness — the second
  definition of the same anchor would re-increment
  `anchor_count`. We keep the `contains_key` semantics by
  inserting a placeholder `empty_scalar()` in Lossless
  mode. `empty_scalar()` (loader.rs `:830–844`) is
  already a `const fn` returning a Node with
  `String::new()` and `Vec::new()` — no heap allocation,
  no meaningful cost.
- No other code paths read `anchor_map` in Lossless mode:
  - `reset_for_document` only clears it.
  - `expand_node` is only called from `resolve_alias`'s
    Resolved branch.
  - No other callers.
- Block_heavy and the other current benchmark fixtures
  contain zero anchors, so this change will not move
  those numbers. The motivation is correctness (Lossless
  mode should not carry the cost of a feature it never
  uses) and realistic Kubernetes/CI workloads where
  anchors are common.
- Unit test `register_anchor_increments_count` at
  `rlsp-yaml-parser/src/loader.rs:884–913` calls
  `register_anchor` with the old signature; it must be
  updated to pass `&node`.
- Related memory:
  `.ai/memory/potential-performance-optimizations.md`
  (candidate L1).

## Steps

- [x] Change `register_anchor`'s `node` parameter from
      `Node<Span>` to `&Node<Span>` in `loader.rs:656`
- [x] In the Resolved branch of `register_anchor`, clone
      the borrowed node when inserting into `anchor_map`
- [x] In the Lossless branch, insert `empty_scalar()` as
      a placeholder so `anchor_map.contains_key` still
      detects re-definitions correctly
- [x] Update the three callers (`:429–431`, `:528–530`,
      `:623–625`) to pass `&node` instead of
      `node.clone()`
- [x] Update unit test
      `register_anchor_increments_count` at
      `loader.rs:884–913` to use the new signature
- [x] Update
      `rlsp-yaml-parser/docs/architecture.md` line 332
      (the `anchor_map` field description in
      `LoadState`) to reflect mode-dependent storage:
      Resolved holds full node clones, Lossless holds
      `empty_scalar()` placeholders for uniqueness
      tracking only
- [x] Update
      `rlsp-yaml-parser/docs/architecture.md` lines
      360–364 (the "Registration" paragraph) so the
      described behavior matches mode-dependent
      registration: Lossless stores a zero-cost
      `empty_scalar()` placeholder, Resolved stores the
      full node clone for expansion
- [x] Run `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` — all pass with zero warnings
- [x] Run `cargo test -p rlsp-yaml-parser --test
      conformance` and confirm 726 passed, 0 failed (351
      stream + 375 loader cases) — anchor resolution must
      be unchanged

## Tasks

### Task 1: Skip anchor-subtree clone in Lossless mode (commit: `5606ccc`)

Change `register_anchor` to borrow its node argument so
callers no longer need to eagerly clone, and insert a
zero-cost `empty_scalar()` sentinel in Lossless mode to
preserve `anchor_map.contains_key`-based uniqueness
tracking. In Resolved mode, clone inside `register_anchor`
so the full subtree remains available for expansion.

- [x] `register_anchor` signature now takes `node:
      &Node<Span>` (not `Node<Span>`)
- [x] Body inserts `node.clone()` into `anchor_map` only
      when `self.options.mode == LoadMode::Resolved`
- [x] Body inserts `empty_scalar()` into `anchor_map` in
      the Lossless branch so `contains_key` and the
      `max_anchors` check work unchanged
- [x] `expanded_nodes` counter logic in
      `register_anchor` is unchanged (still increments
      only in Resolved mode, still checks
      `max_expanded_nodes`)
- [x] All three callers in `parse_node` pass `&node`
      instead of `node.clone()`; the return statement
      that consumes `node` is preserved
- [x] `register_anchor_increments_count` test at
      `loader.rs:884–913` updated to pass `&node`
- [x] `rlsp-yaml-parser/docs/architecture.md` line 332
      (the `anchor_map` field description in
      `LoadState`) rewritten to describe mode-dependent
      contents: Resolved holds full node clones,
      Lossless holds `empty_scalar()` placeholders
- [x] `rlsp-yaml-parser/docs/architecture.md` lines
      360–364 ("Registration" paragraph) rewritten to
      describe mode-dependent behavior: Lossless stores
      a zero-cost `empty_scalar()` placeholder (enabling
      `contains_key`-based uniqueness tracking without
      the subtree clone); Resolved stores a clone of the
      node for later expansion
- [x] `cargo fmt` produces zero diff
- [x] `cargo clippy --all-targets` produces zero warnings
- [x] `cargo test -p rlsp-yaml-parser` — all tests pass,
      including the anchor smoke tests in
      `tests/smoke/anchors_and_aliases.rs`
- [x] `cargo test -p rlsp-yaml-parser --test conformance`
      — 726 passed, 0 failed (351 stream + 375 loader
      cases)
- [x] `cargo test` (full workspace) — all tests pass
- [x] Bench binary builds:
      `CARGO_PROFILE_BENCH_DEBUG=true cargo bench -p
      rlsp-yaml-parser --bench throughput --no-run`
      exits 0

## Decisions

- **Placeholder choice: `empty_scalar()`.** It is already
  defined in `loader.rs`, already used for the
  empty-document case, and has zero heap cost
  (`String::new()` and `Vec::new()` are sentinel). An
  alternative would be adding a second tracking structure
  (e.g., `HashSet<String>`) just for Lossless, but that
  expands state and duplicates the uniqueness logic.
- **Borrow in signature, not ownership.** Taking
  `&Node<Span>` lets callers keep ownership of the node
  for the subsequent `Ok(node)` return. Taking
  `Node<Span>` would force callers to clone anyway. The
  borrow cleanly localizes the clone decision inside
  `register_anchor`.
- **Count/limit semantics are preserved.** Both
  `max_anchors` (via `contains_key`) and
  `max_expanded_nodes` (Resolved-only counter) continue
  to behave identically; only the stored value in
  `anchor_map` changes, and only in Lossless mode.
- **No change to `resolve_alias` or `expand_node`.**
  Lossless `resolve_alias` never reads `anchor_map`;
  Resolved still reads real nodes. Anchor expansion
  behavior is unchanged.
- **Measurement:** block_heavy and other
  currently-benchmarked fixtures have zero anchors, so
  this change will not visibly move those numbers. The
  correctness/memory win is real for anchor-heavy
  workloads (Kubernetes manifests, CI config) that the
  test suite already exercises via
  `tests/smoke/anchors_and_aliases.rs`.
