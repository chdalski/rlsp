**Repository:** root
**Status:** Completed (2026-04-16)
**Created:** 2026-04-16

## Goal

`cargo clippy --all-targets` now fails under the new Rust
1.95.0 lint set. The workspace treats `warnings = "deny"`,
so the clippy failures also block `cargo build --release`
in CI. Fix every 1.95.0 clippy regression on the current
tree so `cargo clippy --all-targets` reports zero warnings
with no behavioural change.

At plan time only three `clippy::while_let_loop` errors in
`rlsp-yaml-parser` were visible — the parser's compilation
failure aborted the workspace check before `rlsp-yaml` was
inspected. Once the parser is clean, two additional
`clippy::unnecessary_trailing_comma` errors in
`rlsp-yaml/src/schema_validation.rs` surface. Both lints
are Rust 1.95.0 promotions; the trailing commas were
already present in the baseline source. Fixing all five
sites is the minimum needed to satisfy the
"zero warnings under `--all-targets`" acceptance
criterion.

## Context

- Rust 1.95.0 activates `clippy::while_let_loop` on
  let-else-break patterns. The workspace lint configuration
  inherits clippy pedantic + nursery as warnings and treats
  `warnings = "deny"`, so any clippy warning is a build
  failure.
- The three `clippy::while_let_loop` errors sit in the
  block-scalar and plain-scalar lexer loops:

  | File | Line | Function context |
  |---|---|---|
  | `rlsp-yaml-parser/src/lexer/block.rs` | 129 | Literal-style block scalar body collection (loop ends line 265) |
  | `rlsp-yaml-parser/src/lexer/block.rs` | 375 | Folded-style block scalar body collection (loop ends line 491) |
  | `rlsp-yaml-parser/src/lexer/plain.rs` | 169 | `collect_plain_continuations` (loop ends line 241) |

- The two `clippy::unnecessary_trailing_comma` errors
  (surfaced post-parser-fix) sit in schema diagnostics:

  | File | Line | Context |
  |---|---|---|
  | `rlsp-yaml/src/schema_validation.rs` | 1141 | minimum-bound violation `format!(...)` call |
  | `rlsp-yaml/src/schema_validation.rs` | 1168 | maximum-bound violation `format!(...)` call |

  Both are single-argument-after-format-string calls where
  Rust 1.95.0's `unnecessary_trailing_comma` lint now flags
  the trailing comma after the last argument. Remove the
  comma — the fix is textual and has no behavioural effect.

- Each site follows the identical shape:

  ```rust
  loop {
      let Some(next) = self.buf.peek_next() else {
          break;
      };
      // body — may call self.buf.consume_next(), may
      // `break;` or `return …` on other conditions
  }
  ```

  Clippy's own suggestion is the direct rewrite to `while
  let Some(next) = self.buf.peek_next() { … }`. Semantics
  match exactly — the `else { break; }` arm fires only on
  `None`, which is precisely when `while let` exits.

- `self.buf.peek_next()` is a borrowing call. In both
  `block.rs` sites, the body already drops the borrow
  before the next iteration by copying `next.content`,
  `next.pos`, and `next.indent` into locals or using them
  only in expressions that end before `consume_next()` is
  called. The `plain.rs` site does the same. None of the
  three bodies hold the `peek_next()` borrow across a call
  to `consume_next()` or another `&mut self` operation —
  so the `while let` rewrite will type-check without
  additional borrow restructuring.

- The existing yaml-test-suite conformance tests exercise
  all three loops — literal block scalars, folded block
  scalars, and plain-scalar continuation lines are all
  heavily represented in the suite. Running the workspace
  test set after the rewrite is sufficient to verify no
  behavioural regression.

- References:
  - `clippy::while_let_loop` —
    https://rust-lang.github.io/rust-clippy/rust-1.95.0/index.html#while_let_loop
  - `rlsp-yaml-parser/README.md`
  - YAML 1.2 spec §8.1 (block scalars) and §7.3.3 (plain
    scalars) — unchanged; this is a mechanical refactor,
    not a spec reinterpretation.

## Steps

- [x] Rewrite the three flagged loops to `while let`
- [x] Confirm `cargo fmt`, `cargo clippy --all-targets`,
      and `cargo test` all succeed with zero warnings

## Tasks

### Task 1: Fix all Rust 1.95.0 clippy regressions

Rewrite the three `loop { let Some(next) =
self.buf.peek_next() else { break; }; ... }` patterns at
the three parser sites listed in Context as `while let
Some(next) = self.buf.peek_next() { ... }`. The loop body
— including every `break;`, `return …`, `continue;`, and
every call to `self.buf.consume_next()` — is preserved
verbatim; only the outer loop header and the opening
`let ... else { break; };` block are removed.

Additionally, remove the unnecessary trailing commas at
the two schema-validation sites listed in Context so the
`unnecessary_trailing_comma` lint passes.

- [x] `rlsp-yaml-parser/src/lexer/block.rs:129` (literal
      block scalar) rewritten to `while let`; the `break`
      previously taken on `peek_next() == None` is now
      supplied by the `while let` itself
- [x] `rlsp-yaml-parser/src/lexer/block.rs:375` (folded
      block scalar) rewritten to `while let` with the same
      preservation of body statements
- [x] `rlsp-yaml-parser/src/lexer/plain.rs:169`
      (`collect_plain_continuations`) rewritten to `while
      let`; the `break` previously taken on `peek_next()
      == None` is now supplied by the `while let` itself
- [x] `rlsp-yaml/src/schema_validation.rs:1141` trailing
      comma removed from the `format!(...)` argument list
- [x] `rlsp-yaml/src/schema_validation.rs:1168` trailing
      comma removed from the `format!(...)` argument list
- [x] `cargo fmt` produces no diff
- [x] `cargo clippy --all-targets` exits 0 with zero
      warnings (the three `while_let_loop` errors are
      gone, no new warnings are introduced by the rewrite)
- [x] `cargo test` passes across the workspace — in
      particular, the `rlsp-yaml-parser` yaml-test-suite
      conformance harness runs the same number of passing
      cases as before the change

**Commit:** `60202c2` — fix(clippy): rewrite while_let_loop
sites and remove trailing commas.

## Decisions

- **Refactor over suppression** — chosen by the user over
  adding `#[expect(clippy::while_let_loop, reason = "…")]`
  attributes. The clippy canonical form is readable and
  semantically identical, so suppression offers no
  benefit.
- **Single commit covering all three sites** — the
  rewrites are mechanical and identical in shape;
  splitting per file would produce review churn without
  making any individual commit easier to understand.
- **No advisor consultation** — pattern-following,
  compiler-verified mechanical refactor of internal
  lexer code with comprehensive existing conformance-test
  coverage for all three loops. No behaviour change, no
  trust-boundary change, no new test patterns introduced.
- **Post-review scope amendment (user-approved)** — the
  plan as originally written enumerated only the three
  `while_let_loop` sites because only those errors were
  visible at plan time (the parser's compilation failure
  aborted the workspace check before `rlsp-yaml` was
  inspected). Once the parser was clean, two additional
  Rust 1.95.0 clippy errors
  (`clippy::unnecessary_trailing_comma`) surfaced in
  `rlsp-yaml/src/schema_validation.rs`. The reviewer
  rejected the first submission because the schema fix
  was outside the plan's enumerated scope. The user
  directed that the plan scope be amended to cover all
  five Rust 1.95.0 clippy sites — the acceptance
  criterion was always "zero warnings under
  `cargo clippy --all-targets`," and this amendment
  aligns the Goal, Context, and Task 1 description with
  that criterion. The corresponding fix is commit
  `60202c2` (see Task 1 above).
