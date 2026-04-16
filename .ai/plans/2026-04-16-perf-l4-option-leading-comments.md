**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-16

## Goal

Change `leading_comments: Vec<String>` → `leading_comments:
Option<Vec<String>>` on all four `Node<Loc>` variants to
eliminate the per-node empty-Vec drop cost currently shown
in the block_heavy baremetal flamegraph as
`drop_in_place<Vec<String>>` at 2.9% of total CPU time. On
documents without leading comments near a node (the common
case for configuration-heavy YAML), the current `Vec`
default still requires a drop-time capacity check on every
node; `None` drops at zero cost. The public accessor
`node.leading_comments()` keeps its existing `-> &[String]`
signature, returning `&[]` for the `None` case so all
consumer-side read code is unchanged. This is a narrowly
scoped subset of the memory file's original "L4 — shrink
`Node` variant size" candidate: only `leading_comments` is
boxed behind `Option` because it is the only one of the
four rare fields with a measured non-zero drop cost when
unused. The broader `Option<Box<NodeMeta>>` refactor
remains a future candidate; its decision rests on whether
this scoped change actually moves throughput.

## Context

- `rlsp-yaml-parser/src/node.rs:37–100` defines four
  `Node<Loc>` variants (`Scalar`, `Mapping`, `Sequence`,
  `Alias`). Every variant carries
  `leading_comments: Vec<String>`. An empty `Vec::new()`
  costs zero heap bytes but the `Drop` impl still runs at
  node-drop time and is visible in the flamegraph.
- `rlsp-yaml-parser/src/node.rs:115–130` defines
  `Node::leading_comments(&self) -> &[String]`, a
  pattern-match accessor returning the inner vec. The
  new version returns
  `leading_comments.as_deref().unwrap_or(&[])`; the
  `&'static [T]` returned from the `&[]` fallback is
  lifetime-compatible with any borrow.
- `rlsp-yaml-parser/src/loader/comments.rs` defines
  `attach_leading_comments(&mut Node<Span>, Vec<String>)`
  which currently early-returns on empty and otherwise
  writes `*leading_comments = comments;`. New body writes
  `*leading_comments = Some(comments);` (guard is kept,
  so an empty input still leaves the field as `None`).
- Profile source:
  `.ai/reports/flame-block_heavy-load.svg` (post-L6,
  2026-04-16). Frame
  `core::ptr::drop_in_place<alloc::vec::Vec<alloc::string::String>>`
  records ~2.9% self-time. This is the frame the change
  targets.
- **Niche optimization expected.** `Vec<T>` holds a
  `NonNull<T>` pointer internally, so `Option<Vec<T>>`
  should stay at 24 bytes via niche tagging (rustc
  reuses the always-non-null pointer bit-pattern for
  `None`). If niche applies, `sizeof(Node)` does not
  grow; if rustc declines niche for some layout reason,
  `Option<Vec<T>>` grows to 32 bytes, partially offsetting
  the drop savings.
- **Construction sites:** 27 total across 7 files. Most
  are tests:
  - `rlsp-yaml-parser/src/loader.rs` (7 sites) — 4
    production (Scalar, Mapping, Sequence, empty_scalar,
    resolve_alias Alias branch) + 3 test helpers
  - `rlsp-yaml-parser/src/loader/reloc.rs` (9 sites) — 1
    production + 8 test
  - `rlsp-yaml-parser/src/loader/comments.rs` (4 sites,
    all test helpers)
  - `rlsp-yaml-parser/src/node.rs` (5 sites, test
    helpers and NF-1..NF-4 test cases)
  - `rlsp-yaml/src/schema_validation.rs` (2 sites, test
    fixtures)
  - `rlsp-yaml-parser/tests/loader.rs` (2 sites)
- **Pattern destructure sites:** `*leading_comments =
  comments` in `attach_leading_comments` is the only
  write through a field pattern. All other references
  are reads via `node.leading_comments()` or bindings
  that do not mutate the field; those remain valid.
- **Related memory:**
  `.ai/memory/potential-performance-optimizations.md`
  (remaining candidate L4 — full variant; this plan is a
  scoped subset). The memory file's L4 entry must be
  updated to reflect that the narrow scope was applied
  and the full `Option<Box<NodeMeta>>` refactor remains
  deferred.
- **Related docs:**
  `rlsp-yaml-parser/docs/architecture.md:420–426`
  (section "AST types") lists `Node::Scalar` fields
  including `leading_comments`. After this change the
  field type is `Option<Vec<String>>` not `Vec<String>`.

## Steps

- [ ] Change `leading_comments` field type from
      `Vec<String>` to `Option<Vec<String>>` in all four
      `Node<Loc>` variants at
      `rlsp-yaml-parser/src/node.rs:37–100`
- [ ] Update
      `Node::leading_comments(&self) -> &[String]` at
      `rlsp-yaml-parser/src/node.rs:115–130` to return
      `leading_comments.as_deref().unwrap_or(&[])`
- [ ] Update
      `attach_leading_comments` at
      `rlsp-yaml-parser/src/loader/comments.rs:7–27` —
      keep the existing empty-input early return; change
      the write from `*leading_comments = comments;` to
      `*leading_comments = Some(comments);`
- [ ] Update all 4 production construction sites in
      `rlsp-yaml-parser/src/loader.rs` (Scalar at
      `:427`, Mapping at `:528`, Sequence at `:625`,
      Alias via `resolve_alias` at `:694`, plus
      `empty_scalar` at `:851`) from
      `leading_comments: Vec::new()` to
      `leading_comments: None`
- [ ] Update the 1 production construction site in
      `rlsp-yaml-parser/src/loader/reloc.rs:102`
- [ ] Update all in-file test fixtures in `node.rs`,
      `loader.rs`, `loader/reloc.rs`, `loader/comments.rs`:
      `leading_comments: Vec::new()` → `leading_comments:
      None`; `leading_comments: vec![…]` → `leading_comments:
      Some(vec![…])`
- [ ] Update test fixtures in
      `rlsp-yaml/src/schema_validation.rs` (2 sites) and
      `rlsp-yaml-parser/tests/loader.rs` (2 sites)
- [ ] Update `rlsp-yaml-parser/docs/architecture.md`
      "AST types" section (around lines 420–426) so the
      `Node::Scalar` field list shows
      `leading_comments: Option<Vec<String>>`
- [ ] Update the Applied summary in
      `.ai/memory/potential-performance-optimizations.md`
      to record this scoped variant and leave the full
      `Option<Box<NodeMeta>>` variant as a deferred
      follow-up
- [ ] Update `.ai/memory/MEMORY.md` index line to reflect
      L4 (scoped) as applied and full L4 deferred
- [ ] Run `cargo fmt`, `cargo clippy --all-targets`, and
      `cargo test` — all pass with zero warnings
- [ ] Run `cargo test -p rlsp-yaml-parser --test
      conformance` and confirm 726 passed, 0 failed (351
      stream + 375 loader cases) — comment-attachment
      behavior must be preserved

## Tasks

### Task 1: Wrap `leading_comments` in `Option` (no Box)

Change the four `Node<Loc>` variant fields from
`Vec<String>` to `Option<Vec<String>>`, preserve the
public accessor contract, update the single write site,
update all 27 construction sites, and keep conformance
and tests green.

- [ ] `Node::Scalar`, `Node::Mapping`, `Node::Sequence`,
      `Node::Alias` each have
      `leading_comments: Option<Vec<String>>`
- [ ] `Node::leading_comments(&self) -> &[String]`
      returns `leading_comments.as_deref().unwrap_or(&[])`
      for all four variants (the pattern-match structure
      is unchanged; only the final expression changes)
- [ ] `attach_leading_comments` in `loader/comments.rs`
      still short-circuits on empty input; non-empty
      writes set `Some(comments)` on the correct variant
- [ ] All 4 production Node construction sites in
      `loader.rs` and 1 in `loader/reloc.rs` use
      `leading_comments: None`
- [ ] All ~22 test-fixture construction sites across
      parser and `rlsp-yaml` crates use `None` for empty
      cases and `Some(vec![…])` for populated cases
- [ ] `rlsp-yaml-parser/docs/architecture.md` "AST
      types" section shows the new field type
- [ ] `.ai/memory/potential-performance-optimizations.md`
      L4 section records this scoped variant as applied
      and keeps the full `Option<Box<NodeMeta>>` refactor
      as a deferred follow-up with the rationale "scoped
      variant landed first to measure the drop-cost win
      without Box indirection cost"
- [ ] `.ai/memory/MEMORY.md` index line updated
- [ ] `cargo fmt` produces zero diff
- [ ] `cargo clippy --all-targets` produces zero warnings
- [ ] `cargo test -p rlsp-yaml-parser` — all tests pass
      (including NF-1..NF-4 in `node.rs`, the 10
      attach_leading_comments / attach_trailing_comment
      tests in `loader/comments.rs`, and the reloc
      tests in `loader/reloc.rs`)
- [ ] `cargo test -p rlsp-yaml-parser --test conformance`
      — 726 passed, 0 failed (351 stream + 375 loader
      cases)
- [ ] `cargo test` (full workspace) — all tests pass,
      including `rlsp-yaml` crate's schema_validation
      tests
- [ ] Bench binary builds:
      `CARGO_PROFILE_BENCH_DEBUG=true cargo bench -p
      rlsp-yaml-parser --bench throughput --no-run`
      exits 0

## Decisions

- **Option, not Box.** `Option<Vec<String>>` avoids the
  heap indirection that a `Box<NodeMeta>` would impose
  on every `anchor()`/`tag()`/`leading_comments()` call.
  The only cost of `Option` on reads is a one-byte
  discriminant check (or zero bytes if niche optimization
  applies); on drops, `None` is a no-op. This is the
  cheapest possible wrapper that eliminates the measured
  drop cost.
- **Scope: `leading_comments` only.** The other three
  rare fields (`anchor: Option<String>`, `tag:
  Option<String>`, `trailing_comment: Option<String>`)
  are already wrapped in `Option` and their `None`
  drops are free. Wrapping them in further `Option`s or
  moving them into a boxed meta struct would add
  indirection cost without a corresponding measured
  drop-cost target. The broader
  `Option<Box<NodeMeta>>` refactor targets the 9.57%
  `drop_in_place<Node>` cumulative frame via cache
  locality, not any specific per-field drop. That win
  is architectural and uncertain; it stays deferred.
- **Accessor signature unchanged.** `node.leading_comments()`
  continues to return `&[String]`. Consumers that read
  comments via the accessor require no source changes.
  The internal wrap `.as_deref().unwrap_or(&[])` is
  trivially inlinable.
- **Test advisor consultation.** This is a refactor with
  many construction sites across tests and two crates,
  and the target behavior (comment attachment) is
  already covered by existing unit and conformance
  tests. Per `risk-assessment.md`, consult the
  test-engineer advisor at the input gate for a review
  of whether the existing tests cover every behavioral
  case after the field-type change, and again at the
  output gate for sign-off on the completed work.
- **No security consultation.** This is an internal
  type change on an AST data structure. No trust
  boundary is affected, no untrusted input handling
  changes, no new allocation pathway is introduced.
- **Measurement.** Pipeline acceptance is behavioral:
  726/0 conformance, zero clippy, zero fmt diff, bench
  binary builds. Throughput and flamegraph verification
  are run baremetal by the user in a follow-up step.
  Expected outcome on block_heavy rlsp/load: +1 to +4%
  (best estimate +2%), with smaller gains on other
  fixtures. If the bench shows no improvement, the
  empty-Vec drop cost was already amortized below
  measurable threshold and the full
  `Option<Box<NodeMeta>>` follow-up should not be
  pursued without new evidence.
