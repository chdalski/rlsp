**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-26

# Recover rlsp-yaml-parser perf regression: stop allocating constant tag URIs

## Goal

Recover the allocation-driven parser performance regression
introduced since commit `3bec2da` (2026-04-16). Two
mechanical changes target the dominant costs identified in
the 2026-04-26 flamegraph:

1. Stop allocating constant tag URI strings on every loaded
   node (4.93% of `block_heavy / load` runtime plus the
   dependent allocator cascade).
2. Fast-path the Core schema plain-scalar dispatch so
   common-case `Str` outcomes exit on a single byte
   comparison.

The plan delivers the two code changes; build, clippy, and
the full test suite must pass after each task. The actual
performance recovery is verified by the user out-of-band on
baremetal — agents in this environment cannot reproduce the
documented baseline measurements reliably, so per-task
throughput numbers are not part of acceptance. After the
user runs the baremetal benchmarks, any fixture that
remains outside the user's ±2% target is addressed in a
separate follow-up plan (see Decisions). This plan does not
revert the §10 schema feature.

## Context

A 2026-04-26 baremetal benchmark run
(`.ai/reports/bench-baremetal.log`,
`.ai/reports/flame-block_heavy-load.svg`) measured a
substantial regression vs the numbers documented in
`benchmarks.md`:

- `rlsp/load` throughput down 26–38% across all size
  fixtures (huge_1MB: 35.69 MiB/s → 22.06 MiB/s = −38%)
- `rlsp/events` throughput down 2–10% across all size
  fixtures
- First-event latency up ~25% (38.9 ns → 48.7 ns)
- `libfyaml` numbers in the same run are flat (±5%) — the
  regression is rlsp-specific, not environmental

The flamegraph for `block_heavy / load` identifies the
dominant cost: `apply_schema_to_node` →
`<str as ToOwned>::to_owned` → `__libc_malloc` at 4.93%
of total runtime, driven by four `resolved.as_str().to_owned()`
sites in `loader.rs` (lines 991, 997, 1016, 1025). Each
call clones a `&'static str` constant from
`ResolvedTag::as_str()` into a fresh `String`, once per
loaded node. Allocator pressure cascades into the visible
`_int_realloc` (9.5%) and `__memmove_avx_unaligned_erms`
bands.

Secondary cost: `resolve_core_plain` runs four cascading
matchers (`is_core_null` → `is_core_bool` → `is_core_int`
→ `is_core_float`) on every plain scalar before falling
back to `Str`. The flamegraph shows `is_core_int` at 2.40%
and `is_core_decimal_float` at 2.45%. For block-heavy and
real-world Kubernetes documents, the vast majority of
plain scalars are short strings that fall through every
matcher to `Str`.

**Key surface area:**

- `rlsp-yaml-parser/src/node.rs` — `Node::tag` field on
  `Scalar`, `Mapping`, `Sequence` variants
  (currently `Option<String>`)
- `rlsp-yaml-parser/src/loader.rs` — four `to_owned()` sites
  in `apply_schema_to_node` (lines 991, 997, 1016, 1025);
  three `Cow::into_owned` sites at the Event→Node boundary
  (lines 471, 495, 608)
- `rlsp-yaml-parser/src/schema.rs` — `resolve_core_plain`
  dispatch and the four `is_core_*` matchers
- `rlsp-yaml-parser/src/event.rs` — `Event::tag` is already
  `Option<Cow<'input, str>>`, no change needed
- `rlsp-yaml-parser/benches/{throughput,latency,memory}.rs`
  — Criterion benchmarks for verification
- Cross-crate consumers — three patterns are in use:
  - `tag.as_deref()` returning `Option<&str>` directly:
    `rlsp-yaml/src/analysis/symbols.rs:157`,
    `rlsp-yaml/src/schema_validation.rs:1532, 1585`,
    `rlsp-yaml/src/validation/validators.rs:298`,
    `rlsp-yaml/src/editing/formatter.rs:695, 711`
  - `tag.as_ref()` binding the inner value as `&String` /
    `&Cow<…, str>` and relying on Deref coercion to call
    `&str` APIs (`is_core_schema_tag`, `format_tag`,
    `trim_start_matches`):
    `rlsp-yaml/src/editing/formatter.rs:547, 1313, 1336,
    1413, 1444, 1581, 1614`
  - Pattern matching on `Node::Scalar { tag, … }` with no
    method call: `rlsp-yaml-parser/src/loader/reloc.rs:353,
    377` (binds and re-emits)

**Constraints from clarification:**

- User's perf target: documented baseline ±2%, verified
  out-of-band on baremetal. Agents do not measure perf —
  Docker timing is unreliable enough that any agent-side
  number would either fail tasks for environmental reasons
  or train agents to ignore the criterion. Per-task
  acceptance is structural (build, clippy, tests) plus
  diff-shape checks that confirm the optimization actually
  landed.
- Public API may break: parser is at 0.6.0 (pre-1.0);
  changing `Node::tag` field type is allowed under
  the project's pre-1.0 SemVer convention.
- Slice 3 (shrink `anchor_loc` / `tag_loc` on the hot
  Node path) is explicitly out of scope. If the user's
  baremetal benchmark shows the ±2% threshold is not met
  after Tasks 1+2, the user files (or the lead files at
  the user's direction) a separate follow-up plan rather
  than expanding this one.

**Specifications and references:**

- YAML 1.2.2 §10 — schema tag resolution
  (https://yaml.org/spec/1.2.2/)
- `rlsp-yaml-parser/docs/benchmarks.md` — baseline numbers
  to recover (commit `3bec2da`, 2026-04-16 baremetal)
- `.ai/reports/bench-baremetal.log` — current (regressed)
  measurements from 2026-04-26
- `.ai/reports/flame-block_heavy-load.svg` — flamegraph
  identifying hot allocation paths

## Steps

- [ ] Task 1: Migrate `Node::tag` to `Option<Cow<'static, str>>`
- [ ] Task 2: First-byte fast-path in `resolve_core_plain`
- [ ] User runs baremetal benchmarks out-of-band and decides
  whether to file a follow-up plan for any remaining gap

## Tasks

### Task 1: Migrate Node::tag to Option<Cow<'static, str>>

Change the `tag` field on every variant of `Node<Loc>` from
`Option<String>` to `Option<Cow<'static, str>>`. Resolver-
injected tags become zero-allocation `Cow::Borrowed`
pointing at `ResolvedTag::as_str()`'s `&'static str`
constants. User-authored tags remain owned via `Cow::Owned`
— same allocation cost as today, no behavior change.

**Implementation:**

- [ ] `node.rs`: change `tag: Option<String>` to
  `tag: Option<Cow<'static, str>>` on `Node::Scalar`,
  `Node::Mapping`, `Node::Sequence`
- [ ] `loader.rs`: replace the four
  `Some(<…>.as_str().to_owned())` sites in
  `apply_schema_to_node` with
  `Some(Cow::Borrowed(<…>.as_str()))`. Sites are:
  line 991 (bare-`!` scalar → `ResolvedTag::Str`),
  line 997 (resolved scalar tag), line 1016 (resolved
  mapping tag), line 1025 (resolved sequence tag)
- [ ] `loader.rs`: at Event→Node boundaries
  (lines 471, 495, 608) replace
  `tag.map(Cow::into_owned)` with
  `tag.map(|t| Cow::Owned(t.into_owned()))` — keeps
  user-authored owning, decouples from the input lifetime
- [ ] Audit cross-crate consumers and update each call
  site so it compiles and behaves identically against the
  new field type:
  - `rlsp-yaml/src/analysis/symbols.rs` —
    `tag.as_deref()` is type-agnostic, expected to need
    no edit
  - `rlsp-yaml/src/schema_validation.rs` —
    `tag.as_deref()` is type-agnostic, expected to need
    no edit
  - `rlsp-yaml/src/validation/validators.rs` —
    `tag.as_deref()` is type-agnostic, expected to need
    no edit
  - `rlsp-yaml/src/editing/formatter.rs` — seven
    `tag.as_ref()` sites (lines 547, 1313, 1336, 1413,
    1444, 1581, 1614) bind the inner value and call
    `&str` APIs via Deref coercion. Verify each compiles
    against `Option<&Cow<'static, str>>`; if any closure
    body fails to coerce, switch the call to
    `tag.as_deref()` (which yields `Option<&str>`
    directly) — both behavior and the surrounding
    semantics are unchanged
  - `rlsp-yaml-parser/src/loader/reloc.rs` — the
    production destructure-and-rebuild at lines 7–71
    binds `tag` by name and re-emits it; the field type
    flows through and needs no edit. The match-arm
    pattern at lines 353, 377 binds only `tag_loc` and
    needs no edit. **Tests in the same file (lines 108+)
    construct `Node::Scalar` / `Node::Mapping` /
    `Node::Sequence` literals with `tag: Some("!t".to_owned())`
    or similar — these construction sites must be updated
    to `tag: Some(Cow::Owned("!t".to_owned()))` (or
    `Cow::Borrowed("!t")` where the literal is
    `'static`).** Sites: 128, 168, 345, 393, 417 (and any
    others the build surfaces).
- [ ] Run `cargo build -p rlsp-yaml` and
  `cargo build -p rlsp-yaml-parser` to surface any other
  call sites the audit missed
- [ ] Update `Cargo.toml`: bump `rlsp-yaml-parser` from
  `0.6.0` to `0.7.0` (breaking change to public Node
  field type)
- [ ] Update parser tests that construct `Node` literals
  with non-`None` tags. Known sites:
  `rlsp-yaml-parser/src/node.rs` (test module),
  `rlsp-yaml-parser/src/loader.rs` (test module),
  `rlsp-yaml-parser/src/loader/reloc.rs` (test module —
  lines 128, 168, 345, 393, 417),
  any `tests/` integration tests in either crate.
  Wrap each tag in `Cow::Owned(...)` for runtime strings
  or `Cow::Borrowed(...)` for `'static` literals
- [ ] Update `rlsp-yaml-parser/docs/feature-log.md` with a
  user-facing entry describing the API change

**Acceptance:**

- [ ] `cargo build` succeeds in the workspace
- [ ] `cargo clippy --all-targets` zero warnings
- [ ] `cargo test` passes in the workspace (parser tests,
  rlsp-yaml tests, integration tests)
- [ ] No remaining `tag.map(Cow::into_owned)` calls in
  `loader.rs` (replaced by the `Cow::Owned(t.into_owned())`
  form documented above) and no `to_owned()` on
  `ResolvedTag::as_str()` results

### Task 2: First-byte fast-path in resolve_core_plain

Replace the four-step cascading matcher in
`resolve_core_plain` with a single byte-prefix dispatch
that prunes the common-case `Str` outcome before any
matcher runs. Preserves identical behavior for all
existing schema test cases.

**Implementation:**

- [ ] `schema.rs`: rewrite `resolve_core_plain` to dispatch
  on `value.as_bytes().first().copied()`:
  - `None` → `Null` (empty string is null per `is_core_null`)
  - `Some(b'~')` → `Null`
  - `Some(b't' | b'T' | b'f' | b'F')` → `is_core_bool`
    else `Str`
  - `Some(b'-' | b'+' | b'0'..=b'9')` →
    `is_core_int` else `is_core_float` else `Str`
  - `Some(b'.')` → `is_core_float` else `Str`
    (covers `.inf`, `.Inf`, `.INF`, `.nan`, `.NaN`,
    `.NAN`, and leading-dot decimal floats like `.5`)
  - any other byte → `Str` (no further checks)
- [ ] Preserve `is_core_null`, `is_core_bool`,
  `is_core_int`, `is_core_float` as-is — they are still
  used by the dispatch and by the public API
- [ ] All existing rstest cases in `schema.rs` pass
  unchanged (no test edits required)

**Acceptance:**

- [ ] `cargo build` succeeds
- [ ] `cargo clippy --all-targets` zero warnings
- [ ] `cargo test schema` passes with zero test
  modifications
- [ ] `cargo test` passes in the workspace
- [ ] `resolve_core_plain` body is a single `match` on
  `value.as_bytes().first().copied()` — no cascading
  if-let-else chain remains

## Decisions

- **Cow lifetime:** `Cow<'static, str>` (not
  `Cow<'input, str>`). The Node AST outlives the input
  buffer, so user-authored tags must be `Cow::Owned`
  (cloned from the borrowed event tag). The `'static`
  bound matches the resolver-injected case
  (`&'static str` constants from `ResolvedTag::as_str()`)
  and does not constrain the owned case.
- **Why not extend Cow to `Event::tag` lifetime everywhere:**
  `Event::tag` is already `Option<Cow<'input, str>>` and
  needs no change. The allocation only happens at the
  Event→Node boundary where ownership transfers.
- **Slice 3 (anchor_loc / tag_loc shrinking) deferred:**
  user direction. If the user's baremetal benchmark shows
  the ±2% threshold is not met by Tasks 1+2, file a
  follow-up plan with the right approach informed by
  post-fix flamegraph data.
- **Perf measurement is the user's job:** agents in this
  environment run inside Docker, where benchmark numbers
  do not match the baremetal target. Putting baremetal
  numbers in agent acceptance criteria forces tasks to
  fail for environmental reasons. The agent's job is to
  ship the code change; the user verifies recovery on
  baremetal.
- **API break is acceptable:** parser is at 0.6.0 (pre-1.0).
  Bumping to 0.7.0 documents the breaking change.
- **No benchmarks.md update in this plan:** the doc is the
  *target* we are restoring to. If the targets are met,
  the doc remains accurate (the numbers reflect what the
  parser can do). If the targets are not met, the
  follow-up plan handles documentation as part of its
  scope.
- **Why a first-byte dispatch (Task 2) is safe:** the
  byte-prefix branches map exactly to the disjoint
  prefix sets of the existing matchers. `is_core_null`
  matches `null`/`Null`/`NULL`/`~`/`""`; the only
  starting bytes are `n`, `N`, `~`, or empty. The
  empty-string and `~` cases are handled directly; the
  `n`/`N` cases would be missed by a strict
  byte-prefix-only check, so any byte outside the
  enumerated branches falls through to `Str` — which is
  the correct answer because no other plain-scalar
  pattern starts with `n` or `N`. The exhaustive rstest
  cases in `schema.rs` verify this.

## Non-Goals

- **Slice 3 — shrinking `anchor_loc` / `tag_loc` on Node
  and Event.** Explicitly deferred per user direction.
  Will be a separate plan if measurements warrant.
- **Documenting new benchmark numbers.** Out of scope —
  the docs are the recovery target, not an artifact of
  this plan.
- **Lazy schema resolution (`Node::resolved_tag(schema)`
  accessor).** A larger API change that would eliminate
  schema overhead from the load path entirely. Defer to a
  separate plan if needed after measuring Tasks 1+2.
- **Tuning libfyaml comparison.** This plan recovers
  rlsp's own regression. Any libfyaml gap remaining
  after recovery is a separate question.
- **First-event latency optimization.** The 22–25%
  latency increase (38.9 ns → 48.7 ns) is real but
  remains ~20,500× under the 1 ms acceptance criterion.
  Not in scope unless slices 1+2 happen to recover it
  as a side effect.
