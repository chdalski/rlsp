**Repository:** root
**Status:** InProgress
**Created:** 2026-04-12

## Goal

Add rstest as a dev-dependency to rlsp-yaml and consolidate
repetitive `#[test]` functions into `#[rstest]` parameterized
tests across all inline test modules in the crate. This
reduces test boilerplate, makes it trivial to add new test
cases, and brings consistency with rlsp-yaml-parser which
already uses rstest.

Scope: all source files with parameterization candidates —
22 files totaling ~1,328 tests. Each file's uniform-pattern
tests are converted; heterogeneous tests stay as standalone
`#[test]` functions.

Excluded: `server.rs` (53% uniform — too heterogeneous to
justify), `code_lens.rs` (5 tests — too few),
`tests/ecosystem_fixtures.rs` (fixture-specific assertions),
`tests/lsp_lifecycle.rs` (async LSP protocol tests with
complex setup).

All tests must pass before and after each task. Behavioral
coverage must not decrease — every existing assertion must
be preserved in a `#[case]`.

## Context

- rstest is NOT currently a dev-dependency of rlsp-yaml —
  must be added in the first task
- rstest v0.26 is already used by rlsp-yaml-parser, so
  the version is established in the workspace
- The crate uses `// ═══` section headers and `// Test N`
  numbering — preserve section headers around parameterized
  functions for navigability
- Same conversion rule as the parser plan: when a group has
  mixed assertion shapes, split into separate `#[rstest]`
  functions per shape. Do NOT unify diverse outputs into a
  common return type
- Leave tests with complex schema setup (building nested
  `JsonSchema` structs with multiple fields) as standalone
  tests — the parameterization win requires that the setup
  is near-identical across cases
- **Named `#[case::name]` syntax required.** Same rule
  as the parser plan: every `#[case]` must use rstest's
  named-case syntax to preserve the original test name's
  intent. Bare `#[case]` lines are grounds for rejection
- **Implementation sequence — add first, then remove.**
  Same as the parser plan: (1) write new `#[rstest]`
  parameterized tests, (2) run `cargo test` with both old
  and new tests to verify coverage parity, (3) only then
  remove old `#[test]` functions
- **Test-engineer consultation on every task.** Same as
  the parser plan: input gate scans for duplicates and
  coverage gaps before conversion; output gate verifies
  no cases were dropped or narrowed during conversion
- The parser plan runs first (no dependency, but establishes
  team conventions)

## Steps

- [x] Add rstest + parameterize scalar_helpers.rs +
      suppression.rs (74df89b)
- [x] Parameterize schema.rs modeline extraction groups
      (d608e6f)
- [x] Parameterize schema_validation.rs groups
      (f969e21)
- [x] Parameterize validators.rs (f3b8444)
- [x] Parameterize formatter.rs (77b2033)
- [x] Parameterize completion.rs + hover.rs (6a918bc)
- [x] Parameterize document_links.rs + rename.rs +
      references.rs (deffa03)
- [x] Parameterize code_actions.rs + semantic_tokens.rs +
      symbols.rs (71c5c2f)
- [ ] Parameterize selection.rs + folding.rs +
      on_type_formatting.rs + document_store.rs + color.rs +
      parser.rs

## Tasks

### Task 1: Add rstest + parameterize `scalar_helpers.rs` + `suppression.rs`

Add rstest v0.26 as a dev-dependency to
`rlsp-yaml/Cargo.toml`. Then convert the tests in both
small files.

**scalar_helpers.rs (28 tests, all uniform):**

- is_null positive/negative: `assert!(is_null(val))` and
  `assert!(!is_null(val))`. Parameterize with (input,
  expected_bool).
- is_bool true/false/negative: same pattern.
- parse_integer cases: decimal, octal, hex, leading zeros,
  empty prefix — all `assert_eq!(parse_integer(input),
  expected)`.
- parse_float cases: decimal, exponent, inf, nan —
  `assert_eq!` (except NaN which needs `is_nan()`).

**suppression.rs (24 tests, ~14 uniform):**

- Sections 1–4: tests calling `build_suppression_map(text)`
  then asserting `is_suppressed(line, code)`. Single-
  assertion tests parameterize with (text, target_line,
  code, expected_suppressed).
- Multi-assertion tests stay standalone.

- [x] Add `rstest = "0.26"` to `[dev-dependencies]`
- [x] Parameterize scalar_helpers.rs tests
- [x] Parameterize suppression.rs single-assertion tests
- [x] Leave multi-assertion suppression tests standalone
- [x] Verify: `cargo test -p rlsp-yaml`
- [x] Verify: `cargo clippy -p rlsp-yaml --all-targets`

### Task 2: Parameterize `schema.rs` modeline groups

Convert the modeline extraction test groups in `schema.rs`
(192 tests, targeting the most uniform sections).

**Prime candidates:**

- extract_schema_url (14+ tests): most are
  `assert_eq!(extract_schema_url(text), Some/None)`.
  Position-based and format-based tests are near-identical.
- extract_yaml_version: same structure — position +
  validity tests.
- extract_custom_tags: check structure before deciding.
- match_schema_by_filename: filename → schema pattern
  matching. Likely uniform.
- detect_kubernetes_resource + kubernetes_schema_url:
  check if uniform.
- SchemaStore catalog parsing: check if uniform.

**Leave alone:**

- parse_schema (complex schema object construction)
- Security tests (unique setup and assertions)
- $ref resolution, SchemaCache, build_agent (complex setup)
- Draft-04 dependencies parsing
- Remote $ref functional tests (tiny_http server setup)

- [x] Parameterize extract_schema_url tests
- [x] Parameterize extract_yaml_version tests
- [x] Parameterize extract_custom_tags if uniform
- [x] Parameterize match_schema_by_filename if uniform
- [x] Parameterize detect_kubernetes tests if uniform
- [x] Leave complex groups standalone
- [x] Verify: `cargo test -p rlsp-yaml`
- [x] Verify: `cargo clippy -p rlsp-yaml --all-targets`

### Task 3: Parameterize `schema_validation.rs` groups

Convert the most repetitive test groups in
`schema_validation.rs` (243 tests total).

**Prime candidates (uniform setup + assertion):**

- Type mismatches (Tests 7–13): one-property schema, parse
  YAML, check diagnostic code and count.
- Enum violations (Tests 19–23): enum match/mismatch.
- Scalar constraints — pattern, minLength/maxLength,
  minimum/maximum, Draft-04 exclusive bounds, Draft-06+
  exclusive bounds, multipleOf, const.
- Format validation: tests per format string (date-time,
  date, time, email, uri, duration, etc.) — valid/invalid
  input cases with identical assertion shape.
- Message consistency sections: diagnostic message wording.

**Leave alone:**

- Required properties (setup varies)
- additionalProperties (complex schema construction)
- Composition (allOf/anyOf/oneOf — heterogeneous)
- Nested validation (multi-level schemas)
- Security tests (unique setups)
- contentSchema (multi-step validation)

- [x] Parameterize type mismatch tests
- [x] Parameterize enum violation tests
- [x] Parameterize scalar constraint groups
- [x] Parameterize format validation groups
- [x] Parameterize message consistency groups
- [x] Leave complex groups standalone
- [x] Verify: `cargo test -p rlsp-yaml`
- [x] Verify: `cargo clippy -p rlsp-yaml --all-targets`

### Task 4: Parameterize `validators.rs`

Convert the repetitive test groups in `validators.rs`
(104 tests). Read the file to identify groups. Expected
candidates: duplicate key detection, flow style
enforcement, key length/naming validation.

- [x] Read and identify parameterization candidates
- [x] Parameterize uniform groups
- [x] Leave heterogeneous groups standalone
- [x] Verify: `cargo test -p rlsp-yaml`
- [x] Verify: `cargo clippy -p rlsp-yaml --all-targets`

### Task 5: Parameterize `formatter.rs`

Convert repetitive tests in `formatter.rs` (110 tests,
~85 uniform). Tests follow a highly uniform pattern:
`format_yaml(input, opts)` → assert result contains
expected strings. Group by formatting feature (indentation,
comments, flow style, etc.).

- [x] Parameterize format-and-assert tests by feature group
- [x] Leave tests with unique format options standalone
- [x] Verify: `cargo test -p rlsp-yaml`
- [x] Verify: `cargo clippy -p rlsp-yaml --all-targets`

### Task 6: Parameterize `completion.rs` + `hover.rs`

**completion.rs (89 tests, ~72 uniform):** tests follow
`parse_docs → complete_at → extract labels/check contents`
pattern. Group by completion context (key, value, nested).

**hover.rs (65 tests, ~51 uniform):** tests follow
`parse_docs → hover_at → hover_content → assert string
contains` pattern. Very consistent assertion shape.

- [x] Parameterize completion.rs uniform groups
- [x] Parameterize hover.rs uniform groups
- [x] Leave heterogeneous tests standalone
- [x] Verify: `cargo test -p rlsp-yaml`
- [x] Verify: `cargo clippy -p rlsp-yaml --all-targets`

### Task 7: Parameterize `document_links.rs` + `rename.rs` + `references.rs`

**document_links.rs (54 tests, ~48 uniform):**
`find_document_links(text, base)` → assert on result
length/tuples. Very high uniformity (89%).

**rename.rs (43 tests, ~38 uniform):**
`prepare_rename/rename` with position → assert on
range/name. Very uniform (88%).

**references.rs (25 tests, ~20 uniform):**
`goto_definition(text, uri, pos)` → assert on
location/range. Uniform (80%).

- [x] Parameterize document_links.rs link-result tests
- [x] Parameterize rename.rs rename-result tests
- [x] Parameterize references.rs definition-result tests
- [x] Leave heterogeneous tests standalone
- [x] Verify: `cargo test -p rlsp-yaml`
- [x] Verify: `cargo clippy -p rlsp-yaml --all-targets`

### Task 8: Parameterize `code_actions.rs` + `semantic_tokens.rs` + `symbols.rs`

**code_actions.rs (47 tests, ~32 uniform):** mixed
diagnostic-driven and context-driven actions. Group by
action type (flow→block, tab→spaces, etc.). ~68% uniform.

**semantic_tokens.rs (41 tests, ~28 uniform):**
`semantic_tokens(text)` → check raw tokens. Group by token
type (keyword, string, number). ~68% uniform.

**symbols.rs (23 tests, ~17 uniform):**
`document_symbols(text, docs)` → `find_symbol` → assert on
kind/children. ~74% uniform.

- [x] Parameterize code_actions.rs by action type
- [x] Parameterize semantic_tokens.rs by token type
- [x] Parameterize symbols.rs symbol-kind tests
- [x] Leave heterogeneous tests standalone
- [x] Verify: `cargo test -p rlsp-yaml`
- [x] Verify: `cargo clippy -p rlsp-yaml --all-targets`

### Task 9: Parameterize remaining small files

Bundle the remaining smaller test modules (31 + 26 + 16 +
21 + 19 + 23 = 136 tests total).

**selection.rs (31 tests, ~24 uniform):**
`selection_ranges(text, docs, positions)` → assert range
chain structure. ~77% uniform.

**folding.rs (26 tests, ~21 uniform):**
`folding_ranges(text)` → `ranges_as_tuples` → assert
contains expected pairs. ~81% uniform.

**on_type_formatting.rs (16 tests, ~14 uniform):**
`format_on_type(text, pos, char, tab)` → assert on
edits. ~88% uniform.

**document_store.rs (21 tests, ~19 uniform):**
open/change/close → assert get result. ~90% uniform.

**color.rs (19 tests, ~13 uniform):** RGB/hex/CSS tests.
Group by color format. ~68% uniform.

**parser.rs (23 tests, ~16 uniform):** parse operations
with AST structure checks. ~70% uniform.

- [ ] Parameterize selection.rs range tests
- [ ] Parameterize folding.rs range tests
- [ ] Parameterize on_type_formatting.rs edit tests
- [ ] Parameterize document_store.rs state tests
- [ ] Parameterize color.rs by color format
- [ ] Parameterize parser.rs uniform parse tests
- [ ] Leave heterogeneous tests standalone
- [ ] Verify: `cargo test -p rlsp-yaml`
- [ ] Verify: `cargo clippy -p rlsp-yaml --all-targets`

## Decisions

- **rstest version 0.26** — matches what rlsp-yaml-parser
  already uses, keeping the workspace consistent
- **Large files get their own task** — schema.rs,
  schema_validation.rs, validators.rs, formatter.rs each
  have enough tests to justify a dedicated task
- **Small files bundled** — files under ~50 tests are
  grouped into tasks of 2–6 files to reduce the number of
  team cycles while keeping each task independently
  committable
- **Add-then-remove implementation** — same as parser plan.
  New parameterized tests verified alongside old tests
  before removal
- **Test-engineer on every task** — same as parser plan.
  Input gate: scan for duplicates/gaps. Output gate:
  verify case parity
- **Named `#[case::name]` syntax** — every `#[case]` uses
  rstest's named-case syntax. Bare `#[case]` lines
  without names are grounds for rejection
- **Excluded files** — `server.rs` (too heterogeneous at
  53%), `code_lens.rs` (only 5 tests),
  `ecosystem_fixtures.rs` and `lsp_lifecycle.rs` (async/
  fixture-specific setup that dominates over assertion
  pattern)
