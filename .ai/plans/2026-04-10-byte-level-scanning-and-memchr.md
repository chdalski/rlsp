**Repository:** root
**Status:** InProgress
**Created:** 2026-04-10

## Goal

Optimize the lexer's scanner inner loops by replacing
character-level iteration (`char_indices().peekable()`) with
byte-level scanning and SIMD-accelerated delimiter search
(`memchr`). This targets the remaining throughput gap after the
lazy Pos optimization, which measured +5.5% on mixed fixtures
but +13-16% on scalar-heavy content — indicating that scanner
inner loop overhead (not position tracking) is now the dominant
cost.

As a prerequisite, split the 4,200+ line `lexer.rs` into
submodules so each scanner can be optimized independently with
clean, reviewable diffs.

## Context

### Current inner loop pattern

All scanner functions in `lexer.rs` use `char_indices().peekable()`
to iterate over content:

```rust
let mut chars = content.char_indices().peekable();
while let Some((i, ch)) = chars.next() {
    if is_s_white(ch) { ... }
    let next_ch = chars.peek().map(|(_, c)| *c);
    if !ns_plain_char_block(prev_was_ws, ch, next_ch) { break; }
    committed_end = i + ch.len_utf8();
    ...
}
```

`char_indices()` decodes UTF-8 on every iteration. For ASCII
bytes (>99% of typical YAML), this decode is unnecessary — each
byte is its own character.

### Scanner functions to optimize

From `lexer.rs`:

| Function | Line | Pattern | Hotness |
|----------|------|---------|---------|
| `scan_plain_line_block()` | 1818 | `char_indices().peekable()` | **Hottest** — most YAML is plain scalars |
| `scan_plain_line_flow()` | 1871 | `char_indices().peekable()` | Hot — flow-context plain scalars |
| `scan_single_quoted_line()` | 2136 | `for (i, ch) in body.chars()` → index tracking | Medium |
| `scan_double_quoted_line()` | 2339 | `body[i..].chars().next()` per iteration | Medium |
| `extract_trailing_comment()` | 1986 | `suffix.char_indices()` | Lower — runs once per scalar |

Also in `lib.rs`:
| Function | Line | Pattern | Hotness |
|----------|------|---------|---------|
| tag URI scanning | 1399 | `content.char_indices()` | Low — tags are rare |
| tag handle scanning | 1277 | `.char_indices()` | Low |

### Optimization approach

**Byte-level ASCII fast path:** for the common case (ASCII-only
content), iterate over `content.as_bytes()` and match on raw
bytes. When a byte ≥ 0x80 is encountered, fall back to
character-level decode for that character. This eliminates
UTF-8 decode overhead for >99% of iterations.

**memchr for bulk delimiter search:** instead of checking each
byte against a set of delimiters, use `memchr::memchr2` or
`memchr::memchr3` to SIMD-scan for the most common terminators
in bulk, then validate context at the candidate position.

For plain scalars, the delimiter set is: `:`, `#`, `\n`, `\r`
(plus flow indicators in flow context). The strategy:
- `memchr2(b':', b'#', &bytes[pos..])` to find the next
  candidate terminator
- At the candidate: validate context (`:` needs following
  whitespace, `#` needs preceding whitespace)
- If not a real terminator, resume scanning from candidate + 1

For quoted scalars:
- Single-quoted: `memchr(b'\'', &bytes[pos..])` to find end
- Double-quoted: `memchr2(b'"', b'\\', &bytes[pos..])` to find
  end or escape

### Lexer submodule split

`lexer.rs` is 4,200+ lines. Splitting into submodules before
optimizing ensures:
- Each scanner has its own file for focused diffs
- Optimization commits are independently reviewable
- Future changes to one scanner don't touch 4K+ lines

Layout (Rust 2018 module style — `lexer.rs` is the parent):

```
rlsp-yaml-parser/src/
  lexer.rs          # Lexer struct, shared types, dispatch, re-exports
  lexer/
    plain.rs        # scan_plain_line_block, scan_plain_line_flow,
                    # peek_plain_scalar_first_line, try_consume_plain_scalar,
                    # collect_plain_continuations, extract_trailing_comment
    quoted.rs       # try_consume_single_quoted, try_consume_double_quoted,
                    # scan_single_quoted_line, scan_double_quoted_line,
                    # collect_double_quoted_continuations
    block.rs        # try_consume_literal_block_scalar,
                    # try_consume_folded_block_scalar,
                    # collect_folded_lines
    comment.rs      # try_consume_comment
```

Functions that remain in `lexer.rs`:
- `Lexer` struct and its core methods (`new`, `peek_next_line`)
- `consume_marker_line`, `consume_line`, `try_consume_directive_line`
- Shared types and helpers used across submodules

### memchr crate

`memchr` is the standard Rust crate for SIMD-accelerated byte
searching. It provides:
- `memchr(needle, haystack)` — find one byte
- `memchr2(n1, n2, haystack)` — find first of two bytes
- `memchr3(n1, n2, n3, haystack)` — find first of three bytes

All use SIMD (SSE2/AVX2 on x86, NEON on ARM) when available,
with scalar fallback. Zero unsafe in the public API.

### Files involved

**Modified:**
- `rlsp-yaml-parser/src/lexer.rs` — split into parent module
- `rlsp-yaml-parser/Cargo.toml` — add `memchr` dependency

**New:**
- `rlsp-yaml-parser/src/lexer/plain.rs`
- `rlsp-yaml-parser/src/lexer/quoted.rs`
- `rlsp-yaml-parser/src/lexer/block.rs`
- `rlsp-yaml-parser/src/lexer/comment.rs`

**Not modified:**
- `rlsp-yaml-parser/src/lib.rs` — tag scanning is low-priority,
  defer to a follow-up if needed
- `rlsp-yaml-parser/src/lines.rs` — already optimized (lazy Pos)
- `rlsp-yaml-parser/src/chars.rs` — predicates unchanged

### Acceptance criteria

1. **All existing tests pass** — 351 conformance, 21 Unicode
   position, all integration tests
2. **Throughput improvement** — target: 2× or better on
   scalar_heavy and block_sequence fixtures (where scanner
   inner loops dominate). Measured via `cargo bench`.
3. **`cargo clippy --all-targets` clean**, zero warnings
4. **No behavior changes** — same events, same spans, same
   errors for all inputs

## Steps

- [x] Split lexer.rs into submodules (Task 1) — c1ff3ce
- [ ] Add memchr dependency and byte-level plain scalar scanning (Task 2)
- [ ] Byte-level scanning for quoted scalars (Task 3)
- [ ] Byte-level scanning for block scalars and comment (Task 4)
- [ ] Benchmark and verify improvement (Task 5)

## Tasks

### Task 1: Split lexer.rs into submodules

Pure refactoring — no behavior change, no new dependencies.

- [x] Create `rlsp-yaml-parser/src/lexer/` directory
- [x] Move plain scalar functions to `lexer/plain.rs`:
  `scan_plain_line_block`, `scan_plain_line_flow`,
  `peek_plain_scalar_first_line`, `try_consume_plain_scalar`,
  `collect_plain_continuations`, `extract_trailing_comment`,
  and related helper types
- [x] Move quoted scalar functions to `lexer/quoted.rs`:
  `try_consume_single_quoted`, `try_consume_double_quoted`,
  `scan_single_quoted_line`, `scan_double_quoted_line`,
  `collect_double_quoted_continuations`, and related types
  (`SingleQuotedScan`, `DoubleQuotedLine`, etc.)
- [x] Move block scalar functions to `lexer/block.rs`:
  `try_consume_literal_block_scalar`,
  `try_consume_folded_block_scalar`, `collect_folded_lines`
- [x] Move comment scanning to `lexer/comment.rs`:
  `try_consume_comment`
- [x] Keep in `lexer.rs`: `Lexer` struct, `new()`,
  `peek_next_line()`, `consume_marker_line()`,
  `consume_line()`, `try_consume_directive_line()`, shared
  types, module declarations, and re-exports
- [x] Adjust visibility: functions called by `Lexer` methods
  in `lexer.rs` need `pub(super)` or `pub(crate)`
- [x] All tests pass unchanged
- [x] `cargo clippy --all-targets` clean
- [x] Commit: `refactor(parser): split lexer into submodules` — c1ff3ce

**Reference impl consultation:** Not applicable (refactoring).
**Advisors:** None (pure code movement).

### Task 2: Byte-level scanning + memchr for plain scalars

Optimize the hottest inner loops: `scan_plain_line_block()` and
`scan_plain_line_flow()`.

- [ ] Add `memchr` dependency to `rlsp-yaml-parser/Cargo.toml`
- [ ] Rewrite `scan_plain_line_block()` in `lexer/plain.rs`:
  - Use `content.as_bytes()` and iterate byte-by-byte for ASCII
  - Use `memchr2(b':', b'#', ...)` to skip ahead to the next
    candidate terminator in bulk
  - At each candidate: validate context (`:` + following
    whitespace, `#` + preceding whitespace)
  - For non-ASCII bytes (≥ 0x80): decode one char, apply
    `ns_plain_char_block`, advance by `char.len_utf8()`
  - Return the same `&str` slice as before
- [ ] Rewrite `scan_plain_line_flow()` in `lexer/plain.rs`:
  - Same pattern but include flow indicators (`,`, `[`, `]`,
    `{`, `}`) in the delimiter set
  - `memchr3` can cover `:`, `#`, `,` — check remaining flow
    indicators at candidate validation
- [ ] All tests pass (351 conformance, 21 Unicode position, all
  integration tests)
- [ ] `cargo clippy --all-targets` clean
- [ ] Commit: `perf(parser): byte-level scanning with memchr for
  plain scalars`

**Reference impl consultation:**
1. Local: existing `scan_plain_line_block()` and
   `scan_plain_line_flow()` for exact termination rules
2. libfyaml: `fy_reader_advance_printable_ascii()` for ASCII
   fast path pattern

**Advisors:** test-engineer — this changes the implementation of
the hottest inner loop. The behavioral contract is unchanged but
the implementation is entirely different. Consult for edge cases
to verify (null bytes, BOM mid-scalar, mixed ASCII/non-ASCII).
Get sign-off on the completed implementation.

### Task 3: Byte-level scanning for quoted scalars

Optimize `scan_single_quoted_line()` and
`scan_double_quoted_line()`.

- [ ] Rewrite `scan_single_quoted_line()` in `lexer/quoted.rs`:
  - Use `memchr(b'\'', &bytes[pos..])` to find end-of-string
    or `''` escape
  - Between quotes: content is passed through directly (no
    per-character decode needed for ASCII)
  - Non-ASCII: fall back to char decode at that position
- [ ] Rewrite `scan_double_quoted_line()` in `lexer/quoted.rs`:
  - Use `memchr2(b'"', b'\\', &bytes[pos..])` to find
    end-of-string or escape sequence
  - Between delimiters: bulk copy (no per-character decode)
  - At `\\`: decode escape sequence (existing `decode_escape`
    logic, may need byte-level adaptation)
  - Non-ASCII: fall back to char decode
- [ ] All tests pass
- [ ] `cargo clippy --all-targets` clean
- [ ] Commit: `perf(parser): byte-level scanning with memchr for
  quoted scalars`

**Reference impl consultation:**
1. Local: existing quoted scanner implementations
2. libfyaml: quoted scalar scanning in `fy-parse.c`

**Advisors:** test-engineer — consult for edge cases in quoted
scalars (escape sequences at buffer boundaries, multi-byte chars
inside quotes, `''` adjacent to non-ASCII). Get sign-off.

### Task 4: Byte-level scanning for block scalars and comments

Optimize remaining scanner functions.

- [ ] Optimize `try_consume_comment()` in `lexer/comment.rs`:
  - Use `memchr(b'\0', ...)` for the null-byte validation check
    (currently `char_indices().find()`)
  - The main comment body is already `&str` slice — no
    per-character iteration needed for content extraction
- [ ] Review block scalar content collection in `lexer/block.rs`:
  - Block scalars iterate lines (via `LineBuffer`), not chars
    within lines — the per-line `for ch in content.chars()`
    walks are for position tracking (already fixed by lazy Pos)
  - Identify any remaining char-level iteration and optimize
    if present
- [ ] Optimize `extract_trailing_comment()` in `lexer/plain.rs`:
  - Currently uses `suffix.char_indices()` to find `#`
  - Replace with `memchr(b'#', suffix.as_bytes())` + context
    validation
- [ ] All tests pass
- [ ] `cargo clippy --all-targets` clean
- [ ] Commit: `perf(parser): byte-level scanning for comments and
  trailing comment extraction`

**Reference impl consultation:** Local implementations.
**Advisors:** None (mechanical optimization, lower-impact paths).

### Task 5: Benchmark and verify

- [ ] Run `cargo bench` (full Criterion suite)
- [ ] Compare throughput against baseline in `docs/benchmarks.md`
  (updated after Task 4 of the lazy Pos plan)
- [ ] Report: target vs measured for each fixture
  - Target: 2× or better on scalar_heavy and block_sequence
  - Target: measurable improvement on all fixture sizes
- [ ] Compare first-event latency (should be unchanged)
- [ ] Compare allocation counts (should be unchanged — this
  optimization reduces CPU work, not allocations)
- [ ] Update `docs/benchmarks.md` with new results
- [ ] Commit: `docs(parser): benchmark results for byte-level
  scanning optimization`

**Reference impl consultation:** Not applicable.
**Advisors:** None (measurement only).

## Decisions

- **Lexer split as Task 1** — refactoring before optimization
  keeps diffs clean and independently reviewable. User
  direction: include in this plan, use `lexer.rs` as parent
  module (Rust 2018 style, not `mod.rs`).
- **memchr over manual SIMD** — `memchr` crate is mature,
  portable, zero-unsafe API. No reason to hand-roll SIMD.
- **All scanners in scope** — user direction: optimize plain,
  quoted, block, and comment scanners. Consistent optimization
  across the lexer.
- **lib.rs tag scanning deferred** — tags are rare in typical
  YAML and the scanning functions are low-traffic. Not worth
  the scope expansion.
- **2× throughput target for scalar-heavy** — the lazy Pos
  benchmark showed scanner inner loops are now the dominant
  cost. Eliminating UTF-8 decode + adding SIMD scanning should
  yield a substantial improvement on content that's mostly
  scalars.
