**Repository:** root
**Status:** InProgress
**Created:** 2026-04-05

## Goal

Add extended test coverage to `rlsp-yaml-parser` beyond the
YAML test suite. The test suite validates spec conformance
but doesn't cover robustness edge cases: malformed UTF-8,
NUL bytes, deeply nested structures, duplicate keys, emitter
round-trips across scalar styles, and panic safety under
adversarial input. These tests are original — inspired by
the categories that mature parsers (libfyaml) cover, but
written for our API.

## Context

- `rlsp-yaml-parser` passes 351/351 YAML test suite
- 922 unit tests cover individual productions and API
- Missing coverage areas identified from libfyaml's test
  suite (383 custom tests across 6 categories):
  - UTF-8 validation and malformed input handling
  - NUL bytes in various positions
  - Emitter round-trip across all scalar/collection styles
  - Duplicate key detection edge cases
  - Panic safety under adversarial input (fuzzing targets)
  - Security controls under stress (alias bombs, deep
    nesting)
- Key files:
  - `rlsp-yaml-parser/src/` — parser source
  - `rlsp-yaml-parser/tests/conformance.rs` — existing
    conformance tests
  - `rlsp-yaml-parser/src/loader.rs` — alias expansion
    limits, nesting depth
  - `rlsp-yaml-parser/src/emitter.rs` — YAML serialization

## Steps

- [x] Add UTF-8 and encoding edge case tests — `d972cb4`
- [x] Add emitter round-trip test corpus — `b6c01e4`
- [x] Add security and robustness stress tests — `b505a65`
- [ ] Add duplicate key and error reporting tests

## Tasks

### Task 1: UTF-8 and encoding edge cases

Test that the parser correctly handles or rejects malformed
input at the byte/character level.

- [x] Reject incomplete 2-byte UTF-8 sequences
- [x] Reject incomplete 3-byte UTF-8 sequences
- [x] Reject incomplete 4-byte UTF-8 sequences
- [x] Reject lone continuation bytes (0x80-0xBF without
      lead byte)
- [x] Reject overlong encodings (e.g., 0xC0 0x80 for NUL)
- [x] Reject invalid bytes 0xFE and 0xFF
- [x] Reject truncated UTF-8 at end of stream
- [x] Accept valid UTF-8 including multibyte (emoji, CJK,
      Arabic)
- [x] Handle NUL byte (0x00) in scalar values — reject or
      handle per spec
- [x] Handle NUL byte in comments
- [x] Handle BOM (0xEF 0xBB 0xBF) at start of stream
- [x] Handle BOM mid-stream (should be rejected or treated
      as content)

**Files:** `rlsp-yaml-parser/tests/encoding.rs` — new

### Task 2: Emitter round-trip corpus

Test that parse → emit → re-parse produces semantically
equivalent results across all YAML features.

- [x] Plain scalars (simple, multiline, with special chars)
- [x] Single-quoted scalars (with escaped quotes, multiline)
- [x] Double-quoted scalars (with escape sequences,
      multiline, Unicode escapes)
- [x] Literal block scalars (clip, strip, keep chomping)
- [x] Folded block scalars (clip, strip, keep, with
      more-indented lines)
- [x] Block sequences (simple, nested, compact)
- [x] Block mappings (simple, nested, compact, explicit keys)
- [x] Flow sequences (simple, nested, empty, trailing comma)
- [x] Flow mappings (simple, nested, empty, adjacent values)
- [x] Anchors and aliases (round-trip preserves anchors)
- [x] Tags (shorthand, verbatim, non-specific)
- [x] Multi-document (with ---, ..., directives)
- [x] Comments (inline, full-line, between entries)
- [x] Mixed styles (flow inside block, block inside flow)
- [x] Empty documents and empty collections
- [x] Complex keys (flow collection as key, multiline key)
- [x] JSON-in-YAML (pure JSON parsed as YAML)
- [x] Large documents (1000+ entries, deeply nested)

**Files:** `rlsp-yaml-parser/tests/round_trip.rs` — new

### Task 3: Security and robustness stress tests

Test that the parser handles adversarial and pathological
input safely — no panics, no unbounded memory, no hangs.

- [x] Alias bomb (billion laughs) — verify expansion limit
      triggers error, not OOM
- [x] Deep nesting (1000+ levels) — verify depth limit
      triggers error
- [x] Circular alias reference — verify cycle detection
- [x] Very long scalar (10MB+) — verify no panic
- [x] Very long line (100K+ chars) — verify no panic
- [x] Very many documents (10K+) — verify no hang
- [x] Very many anchors (100K+) — verify anchor count limit
- [x] Pathological backtracking input — verify parser
      completes in reasonable time
- [x] Empty input, whitespace-only input, comment-only input
- [x] Binary garbage (random bytes) — verify error, no panic
- [x] Input with every possible byte value (0x00-0xFF)
- [x] Maximum indentation depth

**Files:** `rlsp-yaml-parser/tests/robustness.rs` — new

### Task 4: Duplicate key and error reporting tests

Test error detection and reporting quality for common
real-world mistakes.

- [ ] Duplicate plain scalar keys in block mapping
- [ ] Duplicate quoted vs unquoted keys (same value)
- [ ] Duplicate keys in flow mapping
- [ ] Duplicate keys in nested mappings (different scopes
      — should NOT be flagged)
- [ ] Duplicate keys with different types (int vs string
      representation)
- [ ] Error positions point to correct line and column
- [ ] Error messages are descriptive (not just "parse error")
- [ ] Multiple errors in one document (error recovery)
- [ ] Error on invalid merge key values
- [ ] Error on unterminated quoted scalars
- [ ] Error on unterminated flow collections

**Files:** `rlsp-yaml-parser/tests/error_reporting.rs` — new

## Decisions

- **Original tests, not copies.** All test cases are written
  from scratch for our API. Inspired by the categories that
  libfyaml covers, but no copied test data or assertions.
- **Separate test files by category.** Each category gets
  its own integration test file for clear organization and
  independent execution.
- **No fuzzing harness in this plan.** The robustness tests
  are hand-crafted adversarial inputs. A proper fuzzing
  setup (cargo-fuzz / proptest) is separate future work.
- **Round-trip tests use our own emitter.** Parse with our
  parser, emit with our emitter, re-parse, compare ASTs.
  This tests both parser and emitter together.
