**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-22

# Strip Stale Numeric Test Markers in `rlsp-yaml`

## Goal

The `rlsp-yaml` crate carries comments like `// Test 68`,
`// Test 81: exclusive=false at boundary → no error`, and
`// T2.3 — plain bool scalar skips string constraints` that
index into an external test numbering scheme. The numbers
do not correspond to anything visible in the crate — they
refer to a separate test matrix or table that was never
checked into this repository. Crate-internal file splits
completed earlier this month made the markers actively
misleading: a comment numbered into the hundreds now sits
inside a `mod tests` block holding a dozen tests, where the
number references nothing local. Unsplit files
(`hover.rs`, `completion.rs`, `schema.rs`) were already
carrying the same kind of marker; they just never benefitted
from being numbered either.

Strip every stale numeric test marker so the comments
either say something useful or stop being there at all.

## Context

- **Audit results (2026-05-22):** 21 files in `rlsp-yaml/`
  hold ~340 stale numeric markers. The full inventory:

  | Path | Count |
  |------|------|
  | `src/hover.rs` | 46 |
  | `src/completion.rs` | 45 |
  | `src/schema_validation/mapping_constraints.rs` | 44 |
  | `src/schema.rs` | 39 |
  | `src/schema_validation/scalar_constraints.rs` | 27 |
  | `src/schema_validation/array_constraints.rs` | 27 |
  | `src/schema_validation/composition.rs` | 26 |
  | `src/analysis/symbols.rs` | 25 |
  | `src/schema_validation.rs` | 20 |
  | `src/schema_validation/type_validation.rs` | 11 |
  | `src/decorators/document_links.rs` | 7 |
  | `src/analysis/selection.rs` | 6 |
  | `tests/lsp_lifecycle/validators_integration.rs` | 4 |
  | `tests/lsp_lifecycle/rename.rs` | 4 |
  | `tests/lsp_lifecycle/navigation.rs` | 3 |
  | `tests/lsp_lifecycle/folding_ranges.rs` | 2 |
  | `tests/lsp_lifecycle/completion.rs` | 2 |
  | `src/validation/validators/key_ordering.rs` | 2 |
  | `src/server.rs` | 1 |
  | `src/schema_validation/support.rs` | 1 |
  | `src/schema_validation/formats.rs` | 1 |

  Audit command:
  `grep -rnE '//\s*(Test [0-9]+|T[0-9]+\.[0-9]+)' rlsp-yaml/src rlsp-yaml/tests`

- **Marker shapes observed:**
  1. Number only — `// Test 68`, `// Test 87`
  2. Number + colon + context — `// Test 81: exclusive=false at boundary → no error`
  3. Number + em-dash + context — `// Test 176 — format_validation disabled: no diagnostics emitted`
  4. Sub-numbered variant — `// T2.1 — plain string scalar applies string constraints`
  5. Doc-comment variant — `/// Test 14 — nested mapping value selection produces correct line bounds.`

- **Transformation rules (user-approved):**
  - **Number-only line** → delete the entire comment line.
    The well-named `#[test] fn` underneath documents the
    intent on its own; the marker added nothing.
  - **Number + context** → strip the `Test N: ` /
    `Test N — ` / `TX.Y — ` prefix, keep the context.
    The colon or em-dash separator and any surrounding
    whitespace go with the prefix; the remaining comment
    starts at the first word of the context.
  - **`///` doc-comment variant** → same rule, preserve
    the `///` style. `/// Test 14 — nested mapping...` →
    `/// nested mapping...` (the `n` stays lowercase
    because the original sentence began after the
    separator).

- **No code or test behavior changes.** This is a comment
  audit. Every test must still pass, no production code is
  touched. `cargo test` count must match the current
  baseline (6219).

- **Existing patterns to preserve:** section-divider
  comments like `// =================`, `//
  Scalar constraints — pattern`, and group banners
  like `// Group 2: tag-driven string gate` are out of
  scope — the user's directive applies only to the
  numeric markers.

## Steps

- [ ] Clarify scope and transformation rules with user
- [ ] Write plan and run plan-reviewer cycle
- [ ] Apply transformations to all 21 files
- [ ] Verify cleanup is complete and tests pass

## Tasks

### Task 1: Strip stale numeric test markers across `rlsp-yaml/`

Apply the two transformation rules from Context to every
stale numeric marker in the 21 files listed above. One
commit, one task slice — the work is mechanical and
splitting by file adds no review value (the same
verification grep covers all files at once).

- [ ] For every line in the inventory: apply the
      number-only rule (delete whole comment line) or the
      number+context rule (strip prefix, keep context) as
      appropriate to its shape. `///` doc-comment lines
      keep their `///` marker; `//` line comments keep
      their `//`.
- [ ] Verify with
      `grep -rnE '//\s*(Test [0-9]+|T[0-9]+\.[0-9]+)' rlsp-yaml/src rlsp-yaml/tests`
      → must return zero matches. Cite the result in the
      review handoff.
- [ ] `cargo build` succeeds without new warnings.
- [ ] `cargo clippy --all-targets -- -D warnings` passes.
- [ ] `cargo fmt --check` passes (the change is comments
      only — `fmt` should not flag anything).
- [ ] `cargo test` reports 6219 passing — matches the
      current baseline. Cite the count.
- [ ] `git diff --stat` shows only the 21 inventory files
      modified (no other source files touched).

## Non-Goals

- **Section dividers and group banners** (`// =====`,
  `// Scalar constraints — pattern`, `// Group 2:
  tag-driven string gate`) — the user's directive scopes
  cleanup to numeric markers only. Leaving these alone.
- **Test renames** — even where a numeric marker carried
  more meaning than the `#[test] fn` name, we are not
  renaming tests in this plan. Marker-stripping only.
- **Behavior changes, helper rewrites, test additions** —
  pure comment edits. If a file change requires a code
  edit to compile, that is out of scope; pause and ask.
- **Other files outside `rlsp-yaml/`** — `rlsp-fmt/`,
  `rlsp-yaml-parser/`, and integration paths in
  `rlsp-yaml/integrations/` are not part of this audit.
- **Comments referencing test numbers in plan files or
  docs** — `/workspace/.ai/plans/` and `rlsp-yaml/docs/`
  are documentation; the in-code markers are the user's
  concern.

## Decisions

- **One task, not per-file slicing.** The work is
  mechanical (find-and-replace style) and the verification
  is a single grep that covers every file. Splitting by
  file or area would add 4–8 review cycles with no
  structural benefit — exactly the kind of mechanical
  split the project's plan-format guide warns against.
- **Number-only lines get deleted, not blanked.** Leaving
  a `// ` empty comment behind would be marginal noise;
  removing the whole line yields cleaner code and lets
  any blank-line spacing between tests stay correct.
- **No advisors needed.** This is a refactor with no
  trust-boundary concerns, no testability decisions
  (tests are unchanged), and no behavior change. The
  `risk-assessment.md` Refactoring exemption applies.
- **Verification grep is the acceptance signal.** Zero
  matches after the change proves the cleanup is
  complete; the baseline 6219-test count proves nothing
  else was disturbed.
