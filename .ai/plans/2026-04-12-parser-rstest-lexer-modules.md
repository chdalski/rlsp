**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-12

## Goal

Consolidate repetitive `#[test]` functions across the
entire rlsp-yaml-parser crate into `#[rstest]` parameterized
tests. This reduces test LOC, eliminates near-identical
boilerplate, and makes it easier to add new test cases by
appending a single `#[case]` line.

Scope: all test files in the crate with parameterization
candidates — lexer submodules, lines.rs, chars.rs,
encoding.rs, pos.rs, lexer.rs, and all integration tests
including smoke.rs.

Excluded (genuinely heterogeneous): `tests/robustness.rs`
(stress tests with unique setup), `src/node.rs` (4 tests),
`src/loader.rs` (3 tests).

All tests must pass before and after each task. Test count
may decrease (parameterized tests count as one function
generating multiple cases) but behavioral coverage must
not decrease — every existing assertion must be preserved
in a `#[case]`.

## Context

- rstest v0.26 is already a dev-dependency in
  `rlsp-yaml-parser/Cargo.toml`
- The crate uses lettered test groups (Group A, B, C...)
  with comment headers — preserve these as section comments
  around the parameterized functions
- **Conversion rule from memory:** when a group has mixed
  assertion shapes (`assert_eq!`, `matches!`, span-tuple
  comparisons), split into multiple `#[rstest]` functions
  named after their assertion shape (e.g.
  `plain_block_cases_eq`, `plain_block_cases_matches`).
  Do NOT create comparable-type helpers that normalize
  diverse outputs into one unified return type
- **Leave heterogeneous tests alone.** Tests that have
  unique setup, unique assertion logic, or test error
  paths with complex setup should remain as standalone
  `#[test]` functions. The goal is to parameterize
  repetitive tests, not force every test into rstest
- **Descriptive `#[case]` comments required.** Every
  `#[case]` line must carry an inline comment that
  preserves the intent of the original test name. The
  original `#[test] fn block_trailing_whitespace_excluded`
  documents *what behavior* the case exercises — a bare
  `#[case("abc   ", "abc")]` does not. Format:
  `#[case("abc   ", "abc")] // trailing whitespace excluded`.
  Without these comments, a failing case shows only raw
  input/expected values, and a future developer cannot
  tell what scenario is being tested without reading
  production code
- **Implementation sequence — add first, then remove.**
  For each group being converted: (1) write the new
  `#[rstest]` parameterized test with all `#[case]`
  entries, (2) run `cargo test` with both old and new
  tests present to verify the new cases cover the same
  ground, (3) only then remove the old `#[test]`
  functions. This makes the transition auditable — if a
  case is missing or has a wrong expected value, it
  surfaces as a failure or mismatch in the intermediate
  state rather than silently dropping coverage
- **Test-engineer consultation on every task.** Each task
  involves restructuring tests, which is where silent
  coverage regression hides. The test-engineer should:
  (a) at the input gate, scan existing tests for
  duplicates (tests asserting the same thing under
  different names) and coverage gaps (cases that should
  exist but don't — this is the cheapest time to add
  them); (b) at the output gate, verify the parameterized
  tests cover every case the old tests covered — no
  dropped `#[case]`, no accidentally narrowed assertions
- Tasks are ordered by pattern simplicity (simplest
  patterns first) so established conventions carry forward

## Steps

- [ ] Parameterize `src/lexer/plain.rs` tests
- [ ] Parameterize `src/lexer/quoted.rs` tests
- [ ] Parameterize `src/lexer/block.rs` tests
- [ ] Parameterize `src/lexer/comment.rs` + `src/lines.rs`
- [ ] Parameterize `src/chars.rs` + `src/encoding.rs` +
      `src/pos.rs` + `src/lexer.rs`
- [ ] Parameterize integration tests: `unicode_positions.rs`
      + `encoding.rs` + `error_reporting.rs` +
      `loader_spans.rs` + `loader.rs`
- [ ] Parameterize `tests/smoke.rs`

## Tasks

### Task 1: Parameterize `src/lexer/plain.rs`

Convert repetitive test groups in `plain.rs` (92 tests)
to `#[rstest]` parameterized tests.

**Prime candidates (uniform `assert_eq!` on
`scan_plain_line_*`):**

- Group A (scan_plain_line_block — ASCII baseline): tests
  that call `scan_plain_line_block(input)` and assert_eq
  the result. ~10+ tests with identical shape.
- Group B (scan_plain_line_block — memchr candidate bytes):
  same pattern, different inputs.
- Group C (NUL and BOM as terminators): same pattern.
- Group D (whitespace edge cases): same pattern.
- Group E (scan_plain_line_flow — multi-byte parity): tests
  that call `scan_plain_line_flow(input)` and assert_eq.
- Group SPF (scan_plain_line_flow — Task 14): 14 tests,
  all simple `assert_eq!(scan_plain_line_flow(input),
  expected)`.

**Mixed-shape groups — split by assertion type:**

- Group G (try_consume_plain_scalar): tests that use
  `make_lexer` + `try_consume_plain_scalar`. Some assert
  on value (`assert_eq!`), some on Cow variant
  (`matches!`), some on span fields. Split into separate
  `#[rstest]` functions per assertion shape.

**Leave alone:** tests with unique setup (e.g.
`plain_scalar_inline_after_marker_*` which calls
`consume_marker_line` first) unless there are 3+ tests
with the same setup pattern.

- [ ] Group scan_plain_line_block tests into `#[rstest]`
- [ ] Group scan_plain_line_flow tests into `#[rstest]`
- [ ] Group try_consume_plain_scalar value-eq tests
- [ ] Group try_consume_plain_scalar Cow/span tests
- [ ] Verify: `cargo test -p rlsp-yaml-parser`
- [ ] Verify: `cargo clippy -p rlsp-yaml-parser --all-targets`

### Task 2: Parameterize `src/lexer/quoted.rs`

Convert repetitive test groups in `quoted.rs` (92 tests)
to `#[rstest]` parameterized tests.

**Prime candidates:**

- Group H (try_consume_single_quoted):
  - H-A (happy path): tests calling `sq(input)` with
    `assert_eq!` on value. Uniform shape.
  - H-B (Cow allocation): tests calling `sq(input)` with
    `matches!(val, Cow::Borrowed(_))` or `Cow::Owned(_)`.
    Uniform shape — parameterize with a `bool` for
    expected-borrowed.
  - H-C (multi-line folding): tests calling `sq(input)`
    with `assert_eq!`. Uniform shape.
  - H-D (error cases): tests calling `sq_err(input)`.
    Some just verify error existence, some check message
    content.

- Group I (try_consume_double_quoted):
  - I-E (happy path): tests calling `dq(input)` with
    `assert_eq!`. Uniform shape.
  - I-F (hex/unicode escapes): tests calling `dq(input)`
    with `assert_eq!`. Uniform shape. Error variants
    (calling `dq_err`) go in a separate parameterized fn.
  - I-G (line continuation/folding): `dq(input)` +
    `assert_eq!`. Uniform.
  - I-H (Cow allocation): same pattern as H-B.
  - I-I (error cases + security controls): `dq_err(input)`
    tests. Some check specific message substrings — split
    by assertion: plain-error-exists vs message-contains.

- [ ] Group single-quoted happy-path value-eq tests
- [ ] Group single-quoted Cow allocation tests
- [ ] Group single-quoted folding tests
- [ ] Group single-quoted error tests
- [ ] Group double-quoted happy-path value-eq tests
- [ ] Group double-quoted escape value-eq tests
- [ ] Group double-quoted escape error tests
- [ ] Group double-quoted folding tests
- [ ] Group double-quoted Cow allocation tests
- [ ] Group double-quoted error + security tests
- [ ] Verify: `cargo test -p rlsp-yaml-parser`
- [ ] Verify: `cargo clippy -p rlsp-yaml-parser --all-targets`

### Task 3: Parameterize `src/lexer/block.rs`

Convert repetitive test groups in `block.rs` (61 tests)
to `#[rstest]` parameterized tests.

**Prime candidates:**

- Group H-A (header parsing — happy path): tests calling
  `lit_ok(input)` or `fold_ok(input)` and asserting on
  value and/or chomp. Split literal vs folded if helpers
  differ.
- Group H-C (Clip content collection): many `lit_ok`
  tests with `assert_eq!` on value. Uniform shape.
- Group H-D (Strip and Keep chomping): `lit_ok` tests
  varying header (`|-`, `|+`, `|`) and asserting value.
  Can parameterize with (input, expected_value,
  expected_chomp) tuples.
- Group H-E (explicit indent indicator): `lit_ok` tests
  with different indent headers. Uniform shape.

**Mixed-shape groups:**

- Group H-B (header parsing — errors): tests that expect
  errors. May need separate parameterized fn for error
  path.
- Group H-F (termination/boundary): tests with unique
  setup patterns (checking remaining buffer after parse).
  Likely leave standalone.
- Group H-G (tab handling): error tests with message
  assertions.
- Group H-H (UTF-8/special content): small group, check
  if uniform enough.

- [ ] Group literal block value-eq tests (H-C, H-D, H-E)
- [ ] Group header parsing happy-path tests (H-A)
- [ ] Group header parsing error tests (H-B)
- [ ] Group tab handling / error tests (H-G)
- [ ] Leave standalone: termination/boundary tests (H-F)
- [ ] Verify: `cargo test -p rlsp-yaml-parser`
- [ ] Verify: `cargo clippy -p rlsp-yaml-parser --all-targets`

### Task 4: Parameterize `src/lexer/comment.rs` + `src/lines.rs`

Convert repetitive tests in `comment.rs` (26 tests) and
`lines.rs` (66 tests).

**comment.rs candidates:**

- Group A (returns None): tests asserting `None`. Uniform.
- Group B (happy path): `assert_eq!` on text value.
- Group C (span correctness): span field assertions.
- Group E (max_comment_len): varying limit, Ok/Err split.
- Group F (multibyte): check if parameterizable.
- Leave alone: Group D (state effects — heterogeneous).

**lines.rs candidates:**

- BreakType::advance tests: 7 tests varying BreakType
  variant and initial Pos. Parameterize with
  (BreakType, initial_pos, expected_byte, expected_line,
  expected_col) tuples.
- new and initial state: `LineBuffer::new(input)` tests.
- offset and Pos tracking: uniform subset.
- CR/CRLF/LF break detection: varying line ending type.
- Leave alone: complex iteration tests (while-let loops).

- [ ] Parameterize comment.rs groups A, B, C, E, F
- [ ] Parameterize lines.rs BreakType::advance tests
- [ ] Parameterize lines.rs initial-state and offset tests
- [ ] Leave standalone: comment.rs D, lines.rs iteration
- [ ] Verify: `cargo test -p rlsp-yaml-parser`
- [ ] Verify: `cargo clippy -p rlsp-yaml-parser --all-targets`

### Task 5: Parameterize `src/chars.rs` + `src/encoding.rs` + `src/pos.rs` + `src/lexer.rs`

Bundle the four smaller src-level test modules (26 + 22 +
24 + 23 = 95 tests total).

**chars.rs (26 tests, 20 uniform):** character predicate
tests — `assert!(is_predicate(ch))` and negations. Group
by predicate function into parameterized tests with
`#[case]` per character.

**src/encoding.rs (22 tests, 18 uniform):** encoding
detection and normalization tests —
`assert_eq!(detect_encoding(input), expected)`. Uniform
shape.

**pos.rs (24 tests, 22 uniform):** position advancement
tests — `pos.advance(ch)` then assert on fields. Very
uniform, natural for (char, expected_byte, expected_col)
tuples.

**lexer.rs (23 tests, 15 uniform):** boolean predicate
tests are uniform. Tests with complex state setup
(consume_marker_line) left standalone.

- [ ] Parameterize chars.rs predicate tests
- [ ] Parameterize src/encoding.rs detection tests
- [ ] Parameterize pos.rs advancement tests
- [ ] Parameterize lexer.rs uniform predicate tests
- [ ] Leave standalone: lexer.rs complex-state tests
- [ ] Verify: `cargo test -p rlsp-yaml-parser`
- [ ] Verify: `cargo clippy -p rlsp-yaml-parser --all-targets`

### Task 6: Parameterize integration tests

Bundle the smaller integration test files (21 + 31 + 18 +
13 + 55 = 138 tests total).

**tests/unicode_positions.rs (21 tests, 18 uniform):**
span/position assertions — input → collect_events →
assert on byte_offset and column. Parameterize with
(input, expected_byte, expected_col) tuples.

**tests/encoding.rs (31 tests, 24 uniform):** decode tests
— input_bytes → assert_eq!(decode(bytes), expected).
BOM stripping and encoding detection tests are uniform.
Error cases go in separate parameterized fn.

**tests/error_reporting.rs (18 tests, 12 uniform):** error
validation — input → first_error() → assert on message
or position. Uniform error-message tests parameterizable.

**tests/loader_spans.rs (13 tests, 11 uniform):** container
span tests — load(input) → pattern-match Node → assert
on span fields. Uniform shape.

**tests/loader.rs (55 tests, 32 uniform):** load_one →
assert_matches on Node variant. Tests with complex
multi-step validation left standalone.

- [ ] Parameterize unicode_positions.rs span tests
- [ ] Parameterize tests/encoding.rs decode tests
- [ ] Parameterize error_reporting.rs message tests
- [ ] Parameterize loader_spans.rs span tests
- [ ] Parameterize loader.rs uniform Node-matching tests
- [ ] Leave standalone: heterogeneous tests in each file
- [ ] Verify: `cargo test -p rlsp-yaml-parser`
- [ ] Verify: `cargo clippy -p rlsp-yaml-parser --all-targets`

### Task 7: Parameterize `tests/smoke.rs`

Convert uniform test groups in `smoke.rs` (546 tests, ~180
parameterizable across 18 modules) to `#[rstest]`.

The file uses shared helpers `event_variants(input)` and
`parse_to_vec(input)`. Most uniform groups parse input →
assert_eq on event sequence or find specific event →
assert on fields.

**Tier 1 — strongest candidates (~118 tests):**

- `scalars` module (14 tests): input → event_variants →
  assert_eq on (value, style). All uniform.
- `quoted_scalars` module (14 tests): same pattern with
  quoted inputs.
- `block_scalars` module (17 tests): input → find scalar
  event → assert on (value, style, chomp).
- `folded_scalars` module (28 tests): same pattern as
  block_scalars with folded inputs.
- `directives` groups A–E (24 tests): directive parsing →
  assert on DocumentStart fields.
- `tags` groups A, A2, B, F (21 tests): tag URI parsing
  and resolution.

**Tier 2 — good candidates (~65 tests):**

- `sequences` groups A, B, G (9 tests): uniform event
  sequence assertions.
- `mappings` groups A, B, C (9 tests): same pattern.
- `flow_collections` groups B–E, L (8 tests): flow input
  → event matching.
- `anchors_and_aliases` groups A–E (18 tests): anchor/alias
  event assertions.
- `comments` groups A, D (7 tests): comment event presence.
- `scalar_dispatch` groups E, H (8 tests): style dispatch.
- `stream` groups B, D (6 tests): empty/whitespace inputs.

**Leave alone (~283 tests):** `documents` (heterogeneous
span/error/event assertions), `nested_flow_block_mixing`
(unique nesting patterns), span-correctness groups,
security/depth-limit groups, error-assertion groups,
`probe_dispatch` (4 tests), `conformance` (fixture-based).

- [ ] Parameterize Tier 1 groups (scalars, quoted, block,
      folded, directives, tags)
- [ ] Parameterize Tier 2 groups (sequences, mappings,
      flow, anchors, comments, dispatch, stream)
- [ ] Leave standalone: all Tier 4 heterogeneous groups
- [ ] Verify: `cargo test -p rlsp-yaml-parser`
- [ ] Verify: `cargo clippy -p rlsp-yaml-parser --all-targets`

## Decisions

- **One file per task for large files** — plain.rs,
  quoted.rs, block.rs, and smoke.rs each get their own
  task due to size and complexity
- **Bundle small files** — comment.rs + lines.rs,
  chars/encoding/pos/lexer, and the smaller integration
  tests are bundled into single tasks to reduce the number
  of team cycles
- **Split by assertion shape, not by group** — when a
  logical group has mixed assertion types, create separate
  `#[rstest]` functions per shape
- **Preserve group comments** — keep the `// Group X`
  section headers as documentation around parameterized
  functions
- **Descriptive `#[case]` comments** — every `#[case]`
  line must have an inline comment preserving the intent
  of the original test name (e.g. `// trailing whitespace
  excluded`). Bare `#[case]` lines without comments are
  grounds for reviewer rejection
- **smoke.rs tiered approach** — focus on Tier 1+2 (~183
  tests with uniform patterns). Leave Tier 4 (~283 tests
  with heterogeneous assertions) as standalone
- **Add-then-remove implementation** — new parameterized
  tests are written and verified alongside old tests before
  old tests are removed. This prevents silent coverage
  drops during conversion
- **Test-engineer on every task** — this is test
  restructuring, not a behavioral change, but coverage
  regression hides in exactly this kind of refactor. The
  TE scans for duplicates and gaps at the input gate and
  verifies case parity at the output gate
- **Excluded files** — `tests/robustness.rs` (stress tests,
  dynamic generation), `src/node.rs` (4 tests, trait tests),
  `src/loader.rs` (3 tests, each unique),
  `tests/conformance.rs` (rstest `#[files]` already)
