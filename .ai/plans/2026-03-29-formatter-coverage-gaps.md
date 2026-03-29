# Formatter Coverage Gaps

**Repository:** root
**Status:** Completed (2026-03-29)
**Created:** 2026-03-29

## Goal

Close the test coverage gaps in `formatter.rs` (currently
91.8% line coverage, 57 uncovered lines). The formatter is
a core feature that reformats user documents — uncovered
code paths risk silent regressions on less-common YAML
constructs.

## Context

- Coverage measured with `cargo-llvm-cov` on 2026-03-29
- `formatter.rs` is the lowest-coverage non-trivial module
- Uncovered lines fall into 6 categories:
  1. Comment extraction: escape sequences in double-quoted
     strings (lines 66-69, 75)
  2. Comment reattachment: blank-line handling, trailing
     comments at EOF, signature mismatch fallback
     (lines 128, 173, 188-189, 196-200)
  3. `node_to_doc` uncommon branches: `Representation`
     with quoted/plain styles, `Tagged`, `Alias`, `BadValue`
     (lines 296-322)
  4. String/float formatting: NaN/Inf, empty strings,
     numeric-looking strings, escape chars
     (lines 340-384, 454-458)
  5. Nested sequence formatting: sequence-in-sequence
     rendering (lines 471-490, 536, 548, 562-564)
  6. Range/multi-doc edge cases (lines 805, 959, 1111)
- All changes are test-only — no production code
  modifications
- Existing test patterns in `formatter.rs` use a
  `format_and_compare` helper that formats input text and
  asserts against expected output

## Steps

- [x] Add tests for comment extraction edge cases — a803a2a
- [x] Add tests for node_to_doc uncommon branches — bf71d21
- [x] Add tests for string/float formatting edge cases — bf71d21
- [x] Add tests for nested sequences and range formatting — cbce995

## Tasks

### Task 1: Comment and escape edge cases

**Files:** `rlsp-yaml/src/formatter.rs` (test module only)

Add tests covering:

- Double-quoted string with escaped quote before a `#`
  comment: `key: "value with \" escaped"  # comment`
  → verifies the escape-skip logic (lines 66-69)
- Leading comments with blank-line gaps between groups
  → verifies `pending_blanks` logic (line 128)
- Comment-only lines at end of file after all content
  → verifies trailing leading comments (lines 196-200)
- Trailing comment that fails signature matching
  → verifies fallback path (lines 188-189)

- [ ] Test: escaped quote before inline comment
- [ ] Test: blank-line gaps between comment groups
- [ ] Test: trailing comments at end of file
- [ ] Test: comment signature mismatch fallback
- [ ] `cargo clippy` and `cargo test` pass

### Task 2: Uncommon node types and string edge cases

**Files:** `rlsp-yaml/src/formatter.rs` (test module only)

Add tests covering:

**`node_to_doc` branches:**
- `Tagged` node: `!custom_tag value` → formatted correctly
  (lines 315-317)
- `Alias` node: document with anchors and aliases
  → alias rendered (line 320)
- `BadValue` fallback: this may be hard to trigger via
  valid YAML — if untriggerable, add a comment explaining
  why and leave uncovered

**`Representation` branches:**
- Single-quoted representation: `'quoted value'` preserves
  quotes (line 305)
- Double-quoted representation: `"quoted value"` preserves
  quotes (line 306)
- Plain representation: value preserved as-is (line 307)
- Literal block scalar `|` and folded block scalar `>`
  → header preserved, content formatted (lines 300-303)

**Float formatting:**
- `NaN` → `.nan` (line 340)
- `Infinity` → `.inf` (line 342-343)
- Negative infinity → `-.inf` (line 345)
- Float without decimal point gets `.0` appended (line 354)

**String edge cases:**
- Empty string → quoted (line 380)
- Numeric-looking string (e.g., `"123"`) → quoted (line 384)
- String with `\n`, `\r`, `\t` → properly escaped in
  double quotes (lines 454-458)

- [ ] Test: tagged node formatting
- [ ] Test: alias formatting
- [ ] Test: block scalar formatting (literal and folded)
- [ ] Test: quoted representation preservation
- [ ] Test: float special values (NaN, Inf, -Inf)
- [ ] Test: empty and numeric-looking strings
- [ ] Test: string escaping (\n, \r, \t)
- [ ] `cargo clippy` and `cargo test` pass

### Task 3: Nested sequences and range formatting

**Files:** `rlsp-yaml/src/formatter.rs` (test module only)

Add tests covering:

**Nested sequences:**
- Sequence containing a nested sequence:
  ```yaml
  - - inner1
    - inner2
  - outer2
  ```
  → verifies sequence-in-sequence rendering (lines 562-564)

**Range formatting:**
- Format a range that covers only the middle of a
  multi-key document → verifies range extraction logic
  (line 805)
- Format a range in a multi-document YAML file
  → verifies document boundary handling (line 959)

**Multi-document formatting:**
- Document with `---` separator between documents
- Document with `...` terminator

- [ ] Test: nested sequence formatting
- [ ] Test: range formatting on partial document
- [ ] Test: range formatting in multi-document file
- [ ] Test: document separators preserved
- [ ] Re-run coverage, report final numbers
- [ ] `cargo clippy` and `cargo test` pass

## Decisions

- **Test-only changes:** No production code is modified.
  All tasks add test cases to the existing test module in
  `formatter.rs`.

- **BadValue branch:** If this branch cannot be reached
  through valid YAML parsing, document it as a defensive
  fallback and accept the uncovered line. Don't manufacture
  invalid AST nodes just to hit a coverage number.

- **Coverage target:** Aim for ≥97% line coverage on
  `formatter.rs`, matching the project average. 100% is
  not a goal if it requires testing unreachable defensive
  code.
