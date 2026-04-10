**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-10

## Goal

Establish Unicode position correctness with comprehensive tests
(TDD safety net), fix existing byte/char conflation bugs in the
parser's position arithmetic, then optimize `Pos` tracking by
eliminating per-character overhead — following the approach used
by libfyaml and libyaml (neither tracks cumulative `char_offset`).

## Context

### The problem

The parser's `Pos` struct tracks four fields: `byte_offset`,
`char_offset`, `line`, `column`. Position values are constructed
throughout `lexer.rs` and `lib.rs` using arithmetic like:

```rust
let leading = line.content.len() - content.len(); // BYTES
let pos = Pos {
    byte_offset: base.byte_offset + leading,      // correct
    char_offset: base.char_offset + leading,      // BUG: bytes != chars
    column: base.column + leading,                // BUG: bytes != chars
};
```

For ASCII input (>99% of real YAML), byte count == char count, so
the bugs are latent. When multi-byte UTF-8 appears before a
position-sensitive token (CJK characters, emoji, accented chars in
keys/values/comments), `char_offset` and `column` become incorrect.

### Audit findings (lexer.rs)

An audit of `lexer.rs` identified 11 unsafe position arithmetic
sites — all following the same pattern (byte length used where
character count is needed):

| Site | Location | Context |
|------|----------|---------|
| 1 | lexer.rs:319 | `prefix_len` in `consume_marker_line()` |
| 2 | lexer.rs:764 | `leading` in `try_consume_single_quoted()` |
| 3 | lexer.rs:927 | `leading` in `try_consume_double_quoted()` |
| 4 | lexer.rs:1019 | `leading` in `try_consume_block_literal()` |
| 5 | lexer.rs:1240 | `leading` in `try_consume_block_folded()` |
| 6 | lexer.rs:1739 | `leading_spaces` in `peek_plain_scalar_first_line()` |
| 7 | lexer.rs:191 | `hash_byte_offset` in `try_consume_comment()` |
| 8 | lexer.rs:557 | `hash_byte_in_line` trailing comment position |
| 9 | lexer.rs:573 | `bad_i` cascading from site 8 |
| 10 | lexer.rs:601 | `after_scalar_start + bad_i` cascading |
| 11 | lexer.rs:336 | Span built from corrupted `inline_start` (site 1) |

`lib.rs` has ~25+ additional `char_offset` construction sites that
need the same audit — not yet categorized. Some are safe (e.g.,
`marker_pos.char_offset + 3` where `---` is always ASCII), others
use `.len()` on strings that could contain multi-byte chars (e.g.,
`name.len()` for anchor names in alias/anchor position arithmetic).

### Audit findings (lib.rs)

Complete classification of all `char_offset`/`column` construction
sites in `lib.rs`. Tag characters (`is_tag_char`) are ASCII-only so
`tag_token_bytes` == char count. Leading spaces come from
`trim_start_matches(' ')` so always ASCII. Only anchor names (via
`is_ns_anchor_char`) allow multi-byte characters.

| Site | Location | Value added | Classification |
|------|----------|-------------|----------------|
| 1 | lib.rs:524,526 | `+ leading_spaces` — spaces-only trim | **SAFE** |
| 2 | lib.rs:553,555 | `+ leading_spaces` — spaces-only trim | **SAFE** |
| 3 | lib.rs:729,731 | `+ total_offset` = leading_spaces + 1 + spaces_after_dash (all ASCII) | **SAFE** |
| 4 | lib.rs:805,807 | `+ total_offset` = leading_spaces + 1 + spaces_after_q (all ASCII) | **SAFE** |
| 5 | lib.rs:840,842 | `+ leading_spaces` — spaces-only trim | **SAFE** |
| 6 | lib.rs:862,864 | `+ leading_spaces + value_offset_in_trimmed`; `value_offset_in_trimmed = colon_offset + 1 + spaces` where `colon_offset` is a byte offset into trimmed content which can contain multi-byte chars in the mapping key | **UNSAFE** |
| 7 | lib.rs:1525,1527 | `+ 3` constant — `---`/`...` are always ASCII | **SAFE** |
| 8 | lib.rs:2078,2080 | `+ leading` — spaces-only trim | **SAFE** |
| 9 | lib.rs:2111,2113 | `+ 1 + name_char_count` — `name_char_count = name.chars().count()` | **SAFE** |
| 10 | lib.rs:2126,2132 | `rem_char_offset = line_char_offset + leading + 1 + name.len() + spaces`; `name.len()` is bytes, anchor names can be multi-byte | **UNSAFE** |
| 11 | lib.rs:2164,2166 | `+ leading` — spaces-only trim | **SAFE** |
| 12 | lib.rs:2214,2215 | `+ leading + tag_token_bytes + spaces` — tag chars are ASCII-only | **SAFE** |
| 13 | lib.rs:2260,2262 | `+ inline_char_offset` / `+ inline_col` derived from safe tag arithmetic | **SAFE** |
| 14 | lib.rs:2316,2318 | `+ leading` — spaces-only trim | **SAFE** |
| 15 | lib.rs:2335,2336 | `inline_char_offset = line_pos.char_offset + leading + 1 + name.len() + spaces`; `name.len()` is bytes, anchor names can be multi-byte | **UNSAFE** |
| 16 | lib.rs:2396,2398 | `+ inline_char_offset` / `+ inline_col` derived from unsafe site 15 | **UNSAFE** (cascading) |
| 17 | lib.rs:2423,2425 | `+ inline_char_offset` / `+ inline_col` derived from unsafe site 15 | **UNSAFE** (cascading) |
| 18 | lib.rs:3169,3171 | `+ total_offset` = leading_spaces + 1 + spaces_after_colon (all ASCII) | **SAFE** |
| 19 | lib.rs:4219,4221 | `+ 1 + name.chars().count()` — explicit char count | **SAFE** |

**lib.rs unsafe sites summary (5 root sites, 2 cascading):**

- **Site 6** (lib.rs:862,864) — `colon_offset` byte offset into trimmed
  mapping-key content; plain scalars used as keys can contain multi-byte
  UTF-8
- **Site 10** (lib.rs:2126,2132) — alias `rem_char_offset` uses
  `name.len()` (bytes) for anchor name length; anchor names allow
  multi-byte via `is_ns_anchor_char`
- **Site 15** (lib.rs:2335,2336) — anchor `inline_char_offset`/`inline_col`
  uses `name.len()` (bytes); same issue as site 10
- **Sites 16–17** (lib.rs:2396,2398 and 2423,2425) — cascade from site
  15; `inline_char_offset`/`inline_col` used in `Pos` construction

### Related: LSP UTF-16 conversion

`rlsp-yaml/src/parser.rs:47` passes `pos.column` (codepoints)
directly to LSP `Position` (which expects UTF-16 code units).
These differ for supplementary plane characters (emoji, rare CJK).
A correct conversion exists in `document_links.rs:208-215`
(`byte_to_utf16_offset`). This is a separate issue from the parser
bugs but was found during the same audit. **Out of scope for this
plan** — noted here for a future followup.

### Reference: libfyaml's approach

libfyaml (high-performance C YAML parser) tracks only three fields
per position mark:

```c
struct fy_mark {
    size_t input_pos;  // byte offset (pointer subtraction — free)
    int line;          // 0-indexed
    int column;        // 0-indexed, codepoint-based
};
```

Key design choices:
- **No `char_offset`** — neither libfyaml nor libyaml tracks
  cumulative Unicode scalar count. Only byte offset, line, column.
- **Byte offset is free** — pointer subtraction at mark capture
  time, not maintained incrementally.
- **Column is maintained incrementally** — `column++` per character,
  but folded into the scanning loop (not a separate pass).
- **Specialized ASCII fast paths** — `fy_reader_advance_printable_ascii()`
  skips UTF-8 width calculation for ASCII bytes.

### Performance overhead: advance_pos_past_line()

`lines.rs:381-386` walks every character of every line consumed:

```rust
fn advance_pos_past_line(line: &Line<'_>) -> Pos {
    let mut pos = line.pos;
    for ch in line.content.chars() {
        pos = pos.advance(ch);
    }
    line.break_type.advance(pos)
}
```

This is called by `prime()` on every `consume_next()`. For a 1 MB
file with ~20K lines, that's ~1M `Pos::advance()` calls solely for
position bookkeeping — a separate pass over every byte of input,
independent of the actual parsing work.

### Approach: TDD then optimize

Per user direction:

1. Audit all position-sensitive code paths for Unicode correctness
2. Write comprehensive tests covering multi-byte UTF-8 in every
   position-sensitive context
3. Fix bugs to make all tests pass
4. With the safety net in place, optimize Pos tracking:
   - Drop `char_offset` (not used by any consumer in its native
     form; LSP needs UTF-16, not scalar count)
   - Eliminate `advance_pos_past_line()` as a separate pass
   - Compute `column` lazily at span emission time with ASCII
     fast path (`content.is_ascii()` → column == byte position)

### Files involved

**Parser (bugs + optimization):**
- `rlsp-yaml-parser/src/pos.rs` — `Pos` and `Span` structs
- `rlsp-yaml-parser/src/lines.rs` — `advance_pos_past_line()`,
  `LineBuffer::prime()`
- `rlsp-yaml-parser/src/lexer.rs` — 11 unsafe position sites
- `rlsp-yaml-parser/src/lib.rs` — ~25 position construction sites

**Tests (new):**
- `rlsp-yaml-parser/tests/unicode_positions.rs` — new test file

**Not modified (out of scope):**
- `rlsp-yaml/src/parser.rs` — LSP UTF-16 conversion (separate plan)

### Acceptance criteria

1. **All existing tests pass** — 351 conformance, 24 encoding,
   48 error_reporting, 3 loader_spans, robustness, round_trip
2. **New Unicode position tests pass** — covering multi-byte UTF-8
   in every position-sensitive context (plain scalars, quoted
   scalars, block scalars, comments, anchors, tags, aliases,
   document markers with inline content)
3. **Measured throughput improvement** from lazy Pos — target
   15-25% on large fixtures (measured via `cargo bench`)
4. **`cargo clippy --all-targets` clean**, zero warnings

## Steps

- [x] Audit all position arithmetic in lib.rs (Task 1)
- [x] Write Unicode position tests as TDD safety net (Task 2)
- [ ] Fix byte/char conflation bugs (Task 3)
- [ ] Implement lazy Pos optimization (Task 4)
- [ ] Benchmark and verify improvement (Task 5)

## Tasks

### Task 1: Audit lib.rs position arithmetic

Complete the position arithmetic audit for `lib.rs`. The lexer.rs
audit is done (11 unsafe sites documented above). lib.rs has ~25+
`char_offset`/`column` construction sites that need classification.

- [x] Read every `char_offset:` and `column:` construction in
  lib.rs
- [x] For each, determine: is the value being added a byte count
  or a char count? Could the source string contain multi-byte
  UTF-8?
- [x] Classify each as SAFE (guaranteed ASCII arithmetic) or
  UNSAFE (byte length used where char count needed)
- [x] Update this plan with the definitive list of all unsafe
  sites across both files
- [x] Commit: `docs(parser): audit lib.rs position arithmetic
  for Unicode correctness`

**Reference impl consultation:** Not applicable (audit only).
**Advisors:** None (read-only analysis).

### Task 2: Write Unicode position tests

Add comprehensive tests verifying `Span` positions for events
produced from YAML input containing multi-byte UTF-8 characters.
These tests serve as the TDD safety net — they will expose the
byte/char conflation bugs before any fixes are applied.

- [x] Create `tests/unicode_positions.rs`
- [x] Test plain scalars: multi-byte chars in value, verify
  event Span start/end have correct byte_offset, char_offset,
  line, column
- [x] Test plain scalars: multi-byte chars in leading content
  (before the scalar on the same line), verify Span positions
- [x] Test single-quoted scalars with multi-byte leading content
- [x] Test double-quoted scalars with multi-byte leading content
- [x] Test block scalars (literal and folded) with multi-byte
  content on the indicator line
- [x] Test comments with multi-byte content before `#`
- [x] Test anchor names containing multi-byte chars (`&名前`)
- [x] Test tag names containing multi-byte chars
- [x] Test alias references with multi-byte names (`*名前`)
- [x] Test document markers with inline multi-byte content
  (`--- 中文`)
- [x] Test mapping keys that are multi-byte (`日本語: value`)
- [x] Test mixed: multi-byte key with trailing comment
- [x] Verify that `byte_offset` is always correct (it uses
  `.len()` which is bytes — should be right)
- [x] Verify that `char_offset` and `column` match expected
  character counts
- [x] Mark tests that fail as `#[should_panic]` or document
  expected failures — these become the bug fix acceptance gate
  for Task 3
- [x] Build, clippy, commit: `7031829`

**Reference impl consultation:** Not applicable (test-only).
**Advisors:** test-engineer — new test file establishing position
verification patterns. Consult for test list before implementing;
get sign-off on completed test suite before submitting to reviewer.

### Task 3: Fix byte/char conflation bugs

Fix all unsafe position arithmetic sites identified in the audit
(Task 1 + lexer.rs findings in Context). The pattern is consistent:
replace `.len()` byte arithmetic with proper character counting
when computing `char_offset` or `column`.

- [ ] Fix all 11 lexer.rs sites (documented in Context above)
- [ ] Fix all unsafe lib.rs sites (identified in Task 1)
- [ ] For each fix, apply the pattern:
  ```rust
  // Before (UNSAFE):
  let leading = line.content.len() - content.len();
  char_offset: base.char_offset + leading,

  // After (CORRECT):
  let leading_bytes = line.content.len() - content.len();
  let leading_chars = line.content[..leading_bytes].chars().count();
  byte_offset: base.byte_offset + leading_bytes,
  char_offset: base.char_offset + leading_chars,
  column: base.column + leading_chars,
  ```
- [ ] All Unicode position tests from Task 2 pass
- [ ] All 351 conformance tests pass
- [ ] All existing integration tests pass
- [ ] `cargo clippy --all-targets` clean
- [ ] Commit: `fix(parser): correct byte/char conflation in
  position arithmetic`

**Reference impl consultation:**
1. Local: check `pos.rs` `Pos::advance()` for the correct
   per-character update pattern
2. libfyaml: `fy_reader_advance_lb_mode()` for reference on
   how column tracking works correctly

**Advisors:** None (mechanical fixes following an established
pattern; tests from Task 2 serve as the verification gate).

### Task 4: Implement lazy Pos optimization

With all position tests passing (Tasks 2-3), optimize `Pos`
tracking to eliminate the per-character walk in
`advance_pos_past_line()`.

- [ ] Drop `char_offset` from `Pos` struct — change `Pos` to:
  ```rust
  pub struct Pos {
      pub byte_offset: usize,
      pub line: usize,
      pub column: usize,
  }
  ```
  Rationale: neither libfyaml nor libyaml tracks cumulative
  Unicode scalar count. The LSP protocol needs UTF-16 code units,
  not char_offset. No consumer uses char_offset in its native form.
- [ ] Update all `Pos` construction sites in lib.rs and lexer.rs
  to remove `char_offset` field
- [ ] Update all tests that assert on `char_offset` — remove
  those assertions
- [ ] Replace `advance_pos_past_line()` with a lightweight
  function that only computes `byte_offset` and `line` (both
  available without character walking):
  ```rust
  fn pos_after_line(line: &Line<'_>) -> Pos {
      let byte_offset = line.offset + line.content.len()
          + line.break_type.byte_len();
      Pos {
          byte_offset,
          line: line.pos.line + 1,
          column: 0,
      }
  }
  ```
- [ ] For `column` computation at mid-line positions, use an
  ASCII fast path:
  ```rust
  fn column_at(line_content: &str, byte_offset_in_line: usize) -> usize {
      let prefix = &line_content[..byte_offset_in_line];
      if prefix.is_ascii() {
          byte_offset_in_line  // 1 byte = 1 char for ASCII
      } else {
          prefix.chars().count()
      }
  }
  ```
- [ ] Update Span emission points to compute column lazily
  using `column_at()` instead of maintaining it incrementally
- [ ] All Unicode position tests pass (char_offset assertions
  removed, column assertions still verified)
- [ ] All 351 conformance tests pass
- [ ] All existing integration tests pass
- [ ] `cargo clippy --all-targets` clean
- [ ] Commit: `perf(parser): lazy Pos tracking — drop char_offset,
  eliminate per-character walk`

**Reference impl consultation:**
1. libfyaml: `fy_mark` struct (3 fields: input_pos, line, column)
   as precedent for dropping char_offset
2. libfyaml: `fy_reader_advance_printable_ascii()` for ASCII
   fast path pattern

**Advisors:** test-engineer — public API change (Pos struct loses
a field). Consult for test update strategy before implementing;
get sign-off on completed implementation before submitting to
reviewer.

### Task 5: Benchmark and verify

Run benchmarks to measure the throughput improvement from lazy Pos
and verify no performance regressions.

- [ ] Run `cargo bench` (full benchmark suite)
- [ ] Compare throughput (MB/s) for all fixture sizes against
  the baseline in `docs/benchmarks.md`
- [ ] Compare first-event latency (should be unchanged or
  improved)
- [ ] Compare allocation counts (should be unchanged — this
  optimization reduces CPU work, not allocations)
- [ ] Report: target, measured result, whether target met
  (target: 15-25% throughput improvement on large_100KB and
  huge_1MB fixtures)
- [ ] Update `docs/benchmarks.md` with new results if the
  improvement is significant
- [ ] Commit: `docs(parser): benchmark results for lazy Pos
  optimization`

**Reference impl consultation:** Not applicable.
**Advisors:** None (measurement only).

## Decisions

- **Drop `char_offset` from Pos** — follows libfyaml/libyaml
  precedent. No consumer uses cumulative Unicode scalar count.
  The LSP protocol needs UTF-16 code units, which is a different
  value. Maintaining char_offset is O(n) cost for zero benefit.
- **TDD approach** — write tests before fixes, fixes before
  optimization. User direction: "if all tests are in place we
  should update the parser logic to optimize the performance."
- **LSP UTF-16 conversion out of scope** — `parser.rs:47` needs
  fixing but it's in `rlsp-yaml`, not `rlsp-yaml-parser`. Separate
  followup to avoid scope creep.
- **ASCII fast path for column** — `str::is_ascii()` is
  SIMD-accelerated in std. For >99% of real YAML lines, this
  avoids the character walk entirely.
