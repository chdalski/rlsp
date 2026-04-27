**Repository:** root
**Status:** InProgress
**Created:** 2026-04-27

# Code-Action Fixture Tests

## Goal

Code-action behavior is currently described by ~199 inline
`#[test]` cases scattered across 8 files in
`rlsp-yaml/src/editing/code_actions/`. The behavior is hard
to browse — a non-Rust reader cannot see "what does this
action do to a YAML document" without reading test code.
The formatter solved this with `tests/fixtures/formatter/*.md`:
each fixture shows the input document and the expected
output document side-by-side in plain markdown.

This plan introduces the same pattern for the
**cursor-driven, transformational** subset of code actions:
new directory `rlsp-yaml/tests/fixtures/code_actions/`,
new harness `rlsp-yaml/tests/code_action_fixtures.rs`, and
ports the in-scope inline tests across 4 cursor-driven
modules (`tab_to_spaces`, `quoted_bool`, `block_scalar`,
the cursor-only subset of `block_to_flow`). The fixture
must be **visually self-explanatory**: a reader who opens
the `.md` file sees the input, the action invoked, and the
resulting document — no hidden ambient state required to
make sense of the test.

## Context

### Source of truth

The formatter fixture system at
`rlsp-yaml/tests/fixtures/formatter/*.md` (232 fixtures)
and its harness at `rlsp-yaml/tests/formatter_fixtures.rs`
are the model. The new code-action harness reuses the same
format-parsing approach (frontmatter delimited by `---`,
flat key-value pairs, fenced code blocks under `## Section`
headings) and the same `#[rstest] #[files(...)]` mechanism
for per-file pass/fail visibility.

### Pattern taxonomy

The Explore agent classified the 199 inline code-action
tests across 8 modules into three patterns:

- **Pattern A — clean fixture fit:** input doc + cursor +
  optional diagnostic → expected single TextEdit producing
  transformed output. Single positive transformation.
- **Pattern B — negative/no-action:** input doc + cursor →
  asserts a specific action title is absent.
- **Pattern C — does not fit fixture format:** property
  tests (e.g. `int_sequence_item_flow_map_preserves_all_scalars`),
  edit-range structure assertions (e.g.
  `quoted_bool_edit_range_is_scalar_span_not_full_line`),
  multi-cursor batch tests, signature smoke tests.

### In-scope vs out-of-scope modules

The user's scoping decision is that fixtures only make
sense when the user can read the `.md` file and see what
happens when the code action is applied. This excludes
**diagnostic-driven** modules — the action only fires when
a specific diagnostic is in scope, and that prerequisite
is hidden machinery the reader of the `.md` cannot see.

| Module | Driver | Fixture-scope | Tests in fixtures | Tests stay inline |
|--------|--------|---------------|-------------------|-------------------|
| `tab_to_spaces.rs` | cursor | in | 2 (A) | 0 |
| `quoted_bool.rs` | cursor | in | 32 (28 A + 4 B) | 1 (C: range assertion) |
| `block_scalar.rs` | cursor | in | 25 (A) | 0 |
| `block_to_flow.rs` | cursor | in | 26 (22 A + 4 B) | 2 (C) |
| `delete_anchor.rs` | diagnostic | out | 0 | 8 |
| `flow_to_block.rs` | diagnostic | out | 0 | 28 |
| `yaml11_bool.rs` | diagnostic | out | 0 | 51 |
| `yaml11_octal.rs` | diagnostic | out | 0 | 18 |

**Totals:** 85 inline tests ported to fixtures, 108
inline tests remain (105 diagnostic-driven + 3 Pattern C
in cursor-driven modules).

### Existing test helpers and visibility constraint

`rlsp-yaml/src/editing/code_actions.rs` exposes a
`test_helpers` module (lines 141–310) with:

- `cursor_range(line, col)` — zero-width range at position
- `line_range(line)` — full-line range (0..999)
- `docs_for(text)` — parse text into AST documents
- `flow_diags_for(text)` — compute flow-style diagnostics
  (used by diagnostic-driven actions, **not** used by
  cursor-driven fixtures)
- `make_diagnostic(...)` — diagnostic constructor
- `test_uri()` (in `crate::test_utils`) — fixed test URI

**Visibility constraint:** `test_helpers` is `pub(super)`,
reachable only from `mod tests` blocks inside the
`code_actions` module tree. An integration test crate at
`rlsp-yaml/tests/code_action_fixtures.rs` is a separate
crate and cannot import these helpers. The harness
therefore inlines its own small equivalents:

- A local `cursor_range(line, col) -> Range` —
  `Range::new(Position::new(line, col), Position::new(line, col))`
- A local `docs_for(text) -> Vec<Document<Span>>` calling
  the public parser entry point in `rlsp_yaml_parser`
- A local `test_uri() -> Url` — fixed `file:///test.yaml`

Re-implementing these is cheaper than widening visibility
on the inline `test_helpers` module — the helpers are
trivial and inlining keeps the integration-test surface
self-contained.

The harness calls the public `code_actions(...)` function
from `rlsp_yaml::editing::code_actions` with `&[]` for
diagnostics — fixtures only express cursor-driven cases.

### Code-action result structure

`code_actions(...)` returns `Vec<CodeAction>` from
`tower_lsp`. The harness extracts the matching action by
title substring, then reads
`action.edit.changes[&test_uri()][0].new_text` and applies
the edit's `range` to the input text to produce the actual
post-edit document for comparison against
`Expected-Document`.

### Project conventions

- Workspace-level Clippy pedantic + nursery enforcement;
  `cargo clippy --all-targets` must produce zero warnings
- All tests via `cargo test`; `cargo fmt` before commit
- Maximum strictness: `#[expect(lint, reason = "...")]`,
  not `#[allow(lint)]`
- The harness is test code; `#![expect(missing_docs,
  reason = "test code")]` at the file level matches
  `formatter_fixtures.rs`

### Specifications and references

None — this plan introduces internal test infrastructure;
no external specification governs the fixture format.

## Steps

- [x] Define the fixture format (frontmatter fields,
      sections, assertion modes)
- [x] Implement the fixture parser and harness in
      `rlsp-yaml/tests/code_action_fixtures.rs`
- [x] Port `tab_to_spaces` (2 tests) as proof-of-concept
- [x] Update `rlsp-yaml/tests/fixtures/CLAUDE.md` with the
      "visually self-explanatory" rule and the code-action
      fixture conventions
- [x] Delete the 2 inline `tab_to_spaces` tests that were
      ported
- [x] Port `quoted_bool` cursor-driven tests (30 ports
      after misclassification correction); delete the 30
      ported inline tests; keep 3 Pattern C tests inline
- [x] Port `block_scalar` cursor-driven tests (23 ports
      after Pattern C corrections); delete the 23 ported
      inline tests; keep 2 Pattern C tests inline
- [ ] Port `block_to_flow` cursor-driven tests (22 A +
      4 B); delete the 26 ported inline tests; keep the
      2 Pattern C tests inline

## Tasks

### Task 1: Foundation — fixture format, harness, and tab_to_spaces port

Establish the fixture format, harness, and conventions by
delivering the smallest possible end-to-end slice
(`tab_to_spaces` has only 2 tests). This task locks in the
format that subsequent ports follow.

**Fixture format**

Frontmatter (delimited by `---`, flat key-value pairs):

- `test-name: <kebab-case-name>` — informational, mirrors
  formatter fixtures
- `category: <short-label>` — informational (e.g.
  `quoted-bool`)
- `cursor: <line>:<character>` — zero-width cursor
  position passed to `cursor_range(line, character)`
- `applies-action: <title-substring>` — title substring
  the harness uses to find the expected action; the action
  must produce a `TextEdit` that, applied to
  `Test-Document`, yields `Expected-Document`. Mutually
  exclusive with `omits-action`.
- `omits-action: <title-substring>` — title substring the
  harness asserts is **absent** from the returned actions.
  No `Expected-Document` section required. Mutually
  exclusive with `applies-action`.

Sections:

- `## Test-Document` — fenced YAML input (always required)
- `## Expected-Document` — fenced YAML output (required
  when `applies-action` is set; absent when
  `omits-action` is set)

**Harness behavior**

For each `tests/fixtures/code_actions/*.md`:

1. Parse frontmatter and sections (mirrors
   `formatter_fixtures.rs` parser; share idioms, not
   functions — keeping each harness self-contained matches
   the project's style)
2. Build `cursor_range(line, character)` from the
   `cursor:` field
3. Call `code_actions(&docs_for(test_doc), test_doc,
   cursor_range, &[], &test_uri())`
4. **For `applies-action`:** find the action whose
   `title.contains(substring)`; extract its first
   `TextEdit`; apply the edit's `range` and `new_text` to
   `test_doc` to produce the actual post-edit text; assert
   equal to `expected_doc`
5. **For `omits-action`:** assert no action's
   `title.contains(substring)`

**Sub-tasks:**

- [x] Add `rlsp-yaml/tests/code_action_fixtures.rs`
      implementing the parser, harness,
      `#[rstest] #[files("tests/fixtures/code_actions/*.md")]`
      driver, and the local `cursor_range`, `docs_for`,
      `test_uri` helpers (since `test_helpers` is
      `pub(super)` and unreachable from integration tests)
- [x] Add `rlsp-yaml/tests/fixtures/code_actions/`
      directory with the 2 fixtures ported from
      `tab_to_spaces.rs`
- [x] Add a `#[cfg(test)] mod self_tests` block in the
      harness file with **at least 7** self-test cases
      covering: frontmatter parsing, section extraction,
      `applies-action` happy path, `omits-action` happy
      path, missing-section error, mutually-exclusive
      `applies-action` + `omits-action` error, missing
      `cursor:` error
- [x] Rename the top-level heading of
      `rlsp-yaml/tests/fixtures/CLAUDE.md` from
      `# Formatter Fixtures` to `# Test Fixtures`, and
      demote the existing "Setting Interaction Coverage"
      and "Idempotency-Only Fixtures" sections under a
      new `## Formatter Fixtures` H2 so the file's scope
      cleanly covers both feature categories
- [x] Add to `rlsp-yaml/tests/fixtures/CLAUDE.md` two
      new top-level subsections under the renamed
      heading: (a) "When to Write a Fixture vs an Inline
      Test" stating the visually-self-explanatory rule
      and the cursor-driven / transformational scope, and
      (b) "Code-Action Fixtures" documenting the
      frontmatter fields and assertion modes
- [x] Delete the 2 inline `#[test]` functions in
      `rlsp-yaml/src/editing/code_actions/tab_to_spaces.rs`
      that the new fixtures cover; if the `#[cfg(test)]
      mod tests` block becomes empty, delete it as well

**Completed:** 2026-04-27 — `e7491606ee70a1b35ab5b6defb82bc5db863ad06`

**Acceptance:**

- `cargo test --test code_action_fixtures` runs and all
  fixtures pass; the test output shows at least 7 named
  self-test cases for the harness (covering the 7
  scenarios listed above) plus 2 fixture cases
- `cargo test -p rlsp-yaml` (full crate) passes
- `cargo clippy --all-targets -- -D warnings` exits 0
- `cargo fmt --check` exits 0
- `rlsp-yaml/tests/fixtures/code_actions/` contains exactly
  2 fixture files
- `rlsp-yaml/src/editing/code_actions/tab_to_spaces.rs`
  contains 0 `#[test]` functions
- `rlsp-yaml/tests/fixtures/CLAUDE.md` top-level heading
  is `# Test Fixtures`; the existing "Setting Interaction
  Coverage" and "Idempotency-Only Fixtures" sections live
  under a `## Formatter Fixtures` H2; two new H2
  subsections "## When to Write a Fixture vs an Inline
  Test" and "## Code-Action Fixtures" exist; the
  formatter prose content is unchanged in substance

### Task 2: Port quoted_bool

Port the cursor-driven, fixture-shaped tests from
`rlsp-yaml/src/editing/code_actions/quoted_bool.rs`. This
task exercises both `applies-action` and `omits-action`
modes at scale.

The Explore agent's initial classification (28 A + 4 B + 1 C)
proved incomplete: two additional Pattern C tests were
identified during execution — one (`empty_docs`) cannot be
expressed because the fixture harness always parses
non-empty documents, and one (`kind_is_quickfix`) cannot
be expressed because the fixture format has no `kind:`
frontmatter field. The corrected classification is
**30 ports + 3 Pattern C tests inline**.

**Sub-tasks:**

- [x] Add 30 fixtures in
      `rlsp-yaml/tests/fixtures/code_actions/`, one per
      ported test, named in kebab-case derived from the
      original test function names (e.g.
      `quoted-bool-double-quoted-true-to-unquoted.md`)
- [x] For each fixture, the `## Test-Document` and
      `## Expected-Document` (or absence thereof) must
      reproduce the exact transformation the inline test
      verified
- [x] Delete the 30 ported `#[test]` functions from
      `quoted_bool.rs`; keep the 3 Pattern C tests inline:
      `quoted_bool_edit_range_is_scalar_span_not_full_line`
      (range structure),
      `quoted_bool_action_not_offered_for_empty_docs`
      (empty-docs boundary), and
      `quoted_bool_action_kind_is_quickfix` (kind assertion)
- [x] Verify the surviving inline tests still import only
      what they use; remove now-unused `use` lines

**Completed:** 2026-04-27 — `b76d846044e7873238aae9a8546e387888fd2ca5`

**Acceptance:**

- `rlsp-yaml/tests/fixtures/code_actions/` contains 32
  fixture files (2 from Task 1 + 30 from this task)
- `cargo test --test code_action_fixtures` passes; all 32
  fixtures pass plus the 21 self-tests
- `rlsp-yaml/src/editing/code_actions/quoted_bool.rs`
  contains exactly 3 `#[test]` functions:
  `quoted_bool_edit_range_is_scalar_span_not_full_line`,
  `quoted_bool_action_not_offered_for_empty_docs`, and
  `quoted_bool_action_kind_is_quickfix`
- `cargo test -p rlsp-yaml` passes
- `cargo clippy --all-targets -- -D warnings` exits 0
- `cargo fmt --check` exits 0

### Task 3: Port block_scalar

Port the cursor-driven, fixture-shaped tests from
`rlsp-yaml/src/editing/code_actions/block_scalar.rs`. The
Explore agent's classification said all 25 tests were
Pattern A; the developer's pre-scan and implementation
surfaced 2 Pattern C tests that stay inline:
- `should_preserve_trailing_comment_when_converting_to_block_scalar`
  (column-exact range assertion, same shape as the
  quoted_bool precedent)
- `should_offer_block_scalar_when_char_count_meets_threshold_with_multibyte`
  (multibyte input panics the harness's `apply_text_edit`,
  which uses `Position.character` as a byte index — fix
  bundled into Task 4)

**Sub-tasks:**

- [x] Add 23 fixtures in
      `rlsp-yaml/tests/fixtures/code_actions/`, one per
      ported test, named in kebab-case
- [x] For each fixture, reproduce the input and expected
      output from the inline test, including the
      escape-sequence resolution cases (literal `\n`,
      `\t`, `''` mappings) — the fixture body is the
      verification surface
- [x] Delete the 23 ported `#[test]` functions from
      `block_scalar.rs`; keep 2 Pattern C tests inline

**Completed:** 2026-04-27 — `94d5cfc22dc8687f783014c3ffc4ecb710f5596a`

**Acceptance:**

- `rlsp-yaml/tests/fixtures/code_actions/` contains 55
  fixture files (32 from prior tasks + 23 from this task)
- `cargo test --test code_action_fixtures` passes; all 55
  fixtures pass plus 21 self-tests
- `rlsp-yaml/src/editing/code_actions/block_scalar.rs`
  contains exactly 2 `#[test]` functions:
  `should_preserve_trailing_comment_when_converting_to_block_scalar`
  and
  `should_offer_block_scalar_when_char_count_meets_threshold_with_multibyte`
- `cargo test -p rlsp-yaml` passes
- `cargo clippy --all-targets -- -D warnings` exits 0
- `cargo fmt --check` exits 0

### Task 4: Port block_to_flow cursor-driven subset

Port the 26 cursor-driven fixture-shaped tests from
`rlsp-yaml/src/editing/code_actions/block_to_flow.rs` (22
Pattern A + 4 Pattern B). The 2 Pattern C tests stay
inline.

**Sub-tasks:**

- [ ] Add 26 fixtures in
      `rlsp-yaml/tests/fixtures/code_actions/`, one per
      ported test, named in kebab-case
- [ ] For each fixture, reproduce the input and expected
      output (or absence assertion) from the inline test
- [ ] Delete the 26 ported `#[test]` and `#[rstest]`
      functions from `block_to_flow.rs`; keep the 2
      Pattern C tests inline
- [ ] Remove now-unused `use` imports from the surviving
      `#[cfg(test)] mod tests` block

**Acceptance:**

- `rlsp-yaml/tests/fixtures/code_actions/` contains 81
  fixture files (55 from prior tasks + 26 from this task)
- `cargo test --test code_action_fixtures` passes; all 81
  fixtures pass
- `rlsp-yaml/src/editing/code_actions/block_to_flow.rs`
  contains exactly 2 test functions (the 2 Pattern C
  tests)
- `cargo test -p rlsp-yaml` passes
- `cargo clippy --all-targets -- -D warnings` exits 0
- `cargo fmt --check` exits 0

## Decisions

- **Scope:** fixtures cover only **cursor-driven,
  transformational** code actions. Diagnostic-driven
  modules (`delete_anchor`, `flow_to_block`,
  `yaml11_bool`, `yaml11_octal`) stay 100% inline because
  the diagnostic prerequisite is invisible in the `.md`
  body. The fixture format intentionally has no
  `trigger-validator` or `requires-diagnostic` field —
  adding one would make fixtures less self-explanatory.
- **Visually-self-explanatory rule:** documented in
  `rlsp-yaml/tests/fixtures/CLAUDE.md` as a top-level
  subsection. The rule is the gate: if a future test's
  semantics cannot be inferred from `Test-Document` +
  cursor + action title + `Expected-Document`, it stays
  inline.
- **Pattern B (negative cases) included despite being
  borderline:** a reader of an `omits-action` fixture
  sees what the action **doesn't** do, which is part of
  documenting behavior. The user accepted this
  explicitly during clarification.
- **Pattern C tests stay inline:** range-structure
  assertions, property/invariant tests, and signature
  smoke tests do not have a `Test-Document → Expected-Document`
  shape and would require frontmatter fields that hide
  the verification surface.
- **One fixture per inline test:** no consolidation of
  multiple `#[rstest] #[case]` arms into a single
  fixture, and no merging of similar cases. Each fixture
  is a discrete, browsable artifact — same convention as
  the formatter fixtures, where each `.md` is one case.
- **Harness in `tests/code_action_fixtures.rs`, not a
  shared crate:** the parser is small (~150 lines) and
  the formatter and code-action harnesses have different
  frontmatter shapes. Sharing parser code now is
  premature; the formatter harness is `pub(crate)` to its
  own integration-test crate and cannot be imported
  anyway.
- **Inline tests are deleted in the same task that ports
  them:** keeping a duplicate inline + fixture pair would
  double the maintenance surface and dilute the signal of
  "fixture coverage equals what's reachable through the
  harness."
- **Rename feature is a separate plan:** the user
  confirmed rename gets the same migration policy in a
  follow-up plan after this one lands. This plan does not
  touch `rlsp-yaml/src/navigation/rename.rs` or add
  `tests/fixtures/rename/`.

## Non-Goals

- Diagnostic-driven code-action fixtures (the four
  diagnostic-driven modules: `delete_anchor`,
  `flow_to_block`, `yaml11_bool`, `yaml11_octal`) — they
  stay 100% inline; this plan does not modify them
- A `trigger-validator:` or `requires-diagnostic:`
  frontmatter field — explicitly excluded
- Pattern C test ports — range-structure assertions,
  property tests, signature smoke tests stay inline
- Rename feature fixtures — separate follow-up plan
- Hover, completion, diagnostics, document_symbols,
  document_links, navigation, folding, semantic-tokens
  fixtures — not in this plan and not currently
  scheduled
- Sharing fixture-parsing code between
  `formatter_fixtures.rs` and `code_action_fixtures.rs` —
  premature; revisit if a third feature gets fixtures
- Refactoring the existing formatter fixture harness or
  its 232 fixtures
