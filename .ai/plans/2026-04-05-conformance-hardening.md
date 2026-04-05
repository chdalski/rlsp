**Repository:** root
**Status:** Completed (2026-04-05)
**Created:** 2026-04-05

## Goal

Resolve all 114 remaining YAML test suite conformance
failures in `rlsp-yaml-parser`. The initial parser plan
(2026-04-04) targeted 100%/100% but Task 11 was accepted
with 114 failures remaining — this plan finishes that work.
Split: 58 valid YAML we fail to parse (parser bugs) and 56
invalid YAML we incorrectly accept (missing error detection).

## Context

- Prior plan: `.ai/plans/2026-04-04-rlsp-yaml-parser.md`
  (Completed 2026-04-05) built the full parser stack
- Conformance test: `rlsp-yaml-parser/tests/conformance.rs`
  — asserts 0 failures, currently fails with 114
- Test suite: `rlsp-yaml-parser/tests/yaml-test-suite/`
- Parser source: `rlsp-yaml-parser/src/` — `chars.rs`,
  `structure.rs`, `flow.rs`, `block.rs`, `stream.rs`,
  `event.rs`, `combinator.rs`
- 922 unit tests currently pass — regressions must be zero

### Failure Categories

**Valid YAML we fail to parse (58 "unexpected parse error"):**

| Category | Count | Test IDs |
|----------|-------|----------|
| Block scalar/folding | 9 | 5GBF, 6WPF, 7T8X, F6MC, F8F9, H2RW, M29M, MYW6, NB6Z |
| Block structure | 6 | 93JH, 9U5K, JQ4R, S3PD, V9D5, 735Y |
| Flow multiline/collections | 7 | 8KB6, 9BXH, 9SA2, M7NX, NJ66, VJP3, LP6E |
| Anchors in mappings | 5 | 6BFJ, 6M2F, 7BMT, E76Z, U3XV |
| Plain scalar/chars | 5 | 5MUD, DBG4, FBC9, K3WX, S7BG |
| Document handling | 5 | 82AN, M7A3, QT73, S4T7, UT92 |
| Tags/properties | 3 | 9WXW, HMQ5, P76L |
| Quoted scalar multiline | 3 | NAT4, Q8AD, T4YY |
| Complex/empty keys | 3 | 4FJ6, M2N8, NKF9 |
| Tabs | 4 | DC7X, DK95×2, Y79Y |
| Spec examples (mixed) | 8 | AZW3, M5DY, RZP5, RZT7, S9E8, UGM3, XW4D, ZF4X |

**Invalid YAML we accept (56 "expected parse error"):**

| Category | Count | Test IDs |
|----------|-------|----------|
| Bad indentation | 7 | 4HVU, 9C9N, DMG6, N4JP, QB6E, U44R, ZVH3 |
| Invalid mapping/seq | 9 | 236B, 2CMS, 5U3A, 6S55, 9CWY, BD7L, TD5N, ZCZ6, ZL4Z |
| Invalid flow | 7 | 9MAG, C2SP, CTN5, KS4U, N782, YJV2, ZXT5 |
| Comment boundary | 4 | 8XDJ, BF9H, BS4K, GDY7 |
| Anchor misuse | 6 | 4JVG, CXX2, G9HC, GT5M, H7J7, SY6V |
| Directive/document | 12 | 5TRB, 9HCY, 9MMA, 9MQT, B63P, EB22, H7TQ, MUS6×2, RHX7, RXY3, SF5V |
| Other (tabs, tags, keys) | 11 | 7MNF, DK95, JKF3, QLJ7, U99R, W9L4, Y79Y×5 |

## Steps

- [x] Fix block scalar and folding failures (9 tests) — 8db865e
- [x] Fix block structure failures (5/6 tests; 735Y deferred) — 9984a77
- [x] Fix flow multiline and collection failures (4/7 tests; 8KB6,NJ66,VJP3 deferred) — b619517
- [x] Fix anchor/property handling failures (3/8 tests; E76Z,HMQ5,P76L,7BMT,U3XV deferred) — 45ac394
- [x] Fix plain/quoted scalar failures (5/8 tests + P76L bonus; FBC9,T4YY,NAT4 deferred) — fb583af
- [x] Fix document handling and tab failures (10/19 tests; QT73,UT92,AZW3,DK95,Y79Y,RZP5,S9E8,UGM3,XW4D deferred) — fc19ac1
- [x] Fix remaining valid-YAML failures and deferred items (14 tests) + 1 invalid rejection — 644b55c
- [x] Add invalid input rejection (12/15 tests; S98Z,ZXT5,Y79Y[2] deferred) — 19822fe
- [x] Fix final 3 failures (ZXT5, S98Z, Y79Y[2]) via pre-event validation — 80c213d
- [x] Verify 0 conformance failures, 0 unit test regressions — 351/351, 922/0 ✓

## Tasks

### Task 1: Block scalar and folding fixes

Fix 9 valid-YAML failures in block scalar parsing and line
folding. These are spec §6 (empty lines, flow folding) and
§8 (literal/folded block scalars, chomping).

- [x] 5GBF — Spec Example 6.5. Empty Lines
- [x] 6WPF — Spec Example 6.8. Flow Folding
- [x] 7T8X — Spec Example 8.10. Folded Lines
- [x] F6MC — More indented lines at beginning of folded
- [x] F8F9 — Spec Example 8.5. Chomping Trailing Lines
- [x] H2RW — Blank lines
- [x] M29M — Literal Block Scalar
- [x] MYW6 — Block Scalar Strip
- [x] NB6Z — Multiline plain with tabs on empty lines

**Files:** `block.rs`, `structure.rs`

### Task 2: Block structure fixes

Fix 6 valid-YAML failures in block sequence/mapping
handling.

- [x] 93JH — Block Mappings in Block Sequence
- [x] 9U5K — Spec Example 2.12. Compact Nested Mapping
- [x] JQ4R — Spec Example 8.14. Block Sequence
- [x] S3PD — Spec Example 8.18. Implicit Block Mapping
- [x] V9D5 — Spec Example 8.19. Compact Block Mappings
- [ ] 735Y — Spec Example 8.20. Block Node Types (deferred — whitespace-before-colon conflict)

**Files:** `block.rs`

### Task 3: Flow multiline and collection fixes

Fix 7 valid-YAML failures in flow collections and multiline
flow keys/values.

- [x] 8KB6 — Multiline plain flow mapping key without value (fixed in Task 7 — 644b55c)
- [x] 9BXH — Multiline double-quoted flow mapping key
- [x] 9SA2 — Multiline double-quoted flow mapping key
- [x] M7NX — Nested flow collections
- [x] NJ66 — Multiline plain flow mapping key (fixed in Task 7 — 644b55c)
- [ ] VJP3 — Flow collections over many lines (deferred — stream-level issue)
- [x] LP6E — Whitespace after scalars in flow

**Files:** `flow.rs`

### Task 4: Anchor and property fixes

Fix 8 valid-YAML failures in anchor handling and node
properties.

- [x] 6BFJ — Mapping, key and flow sequence item anchors
- [x] 6M2F — Aliases in Explicit Block Mapping
- [x] 7BMT — Node and Mapping Key Anchors (fixed in Task 7 — 644b55c)
- [ ] E76Z — Aliases in Implicit Block Mapping (deferred — whitespace-before-colon conflict)
- [x] U3XV — Node and Mapping Key Anchors (fixed in Task 7 — 644b55c)
- [x] 9WXW — Spec Example 6.18. Primary Tag Handle
- [ ] HMQ5 — Spec Example 6.23. Node Properties (deferred — whitespace-before-colon conflict)
- [x] P76L — Spec Example 6.19. Secondary Tag Handle (fixed in Task 5)

**Files:** `structure.rs`, `flow.rs`, `block.rs`

### Task 5: Scalar parsing fixes

Fix 8 valid-YAML failures in plain and quoted scalar
handling.

- [x] 5MUD — Colon and adjacent value on next line
- [x] DBG4 — Spec Example 7.10. Plain Characters
- [x] FBC9 — Allowed characters in plain scalars (fixed in Task 7 — 644b55c)
- [x] K3WX — Colon and adjacent value after comment
- [x] S7BG — Colon followed by comma
- [ ] NAT4 — Various empty/newline quoted strings (deferred — empty quoted edge cases)
- [x] Q8AD — Spec Example 7.5. Double Quoted Line Breaks
- [x] T4YY — Spec Example 7.9. Single Quoted Lines (fixed in Task 7 — 644b55c)

**Files:** `flow.rs`, `chars.rs`

### Task 6: Document, tab, and misc valid fixes

Fix 14 remaining valid-YAML failures: document handling,
tabs, complex keys, and spec examples.

- [x] 82AN — Three dashes and content without space
- [x] M7A3 — Spec Example 9.3. Bare Documents
- [ ] QT73 — Comment and document-end marker (deferred — multiline flow in explicit docs)
- [x] S4T7 — Document with footer
- [x] UT92 — Spec Example 9.4. Explicit Documents (fixed in Task 7 — 644b55c)
- [x] DC7X — Various trailing tabs
- [ ] DK95 — Tabs that look like indentation (deferred — tab-in-indentation detection)
- [ ] Y79Y — Tabs in various contexts (deferred — tab-in-indentation detection)
- [x] 4FJ6 — Nested implicit complex keys
- [x] M2N8 — Question mark edge cases (fixed in Task 4)
- [x] NKF9 — Empty keys in block and flow mapping
- [x] AZW3 — spec example (fixed in Task 7 — 644b55c)
- [x] M5DY — spec example
- [ ] RZP5 — spec example (deferred — verbatim tags)
- [x] RZT7 — spec example
- [ ] S9E8 — spec example (deferred — explicit key/value with comments)
- [ ] UGM3 — spec example (deferred — verbatim tags)
- [ ] XW4D — spec example (deferred — explicit key/value with comments)
- [x] ZF4X — spec example

**Files:** `stream.rs`, `block.rs`, `flow.rs`, `structure.rs`

### Task 7: Remaining valid-YAML fixes + initial invalid rejection

Fix deferred valid-YAML failures from Tasks 2-6 and begin
invalid-YAML rejection. 14 valid-YAML + 1 invalid-YAML
fixed in 644b55c. 15 invalid-YAML failures remain.

- [x] Deferred valid-YAML fixes (14 tests) — 644b55c
- [x] SF5V — duplicate %YAML directive rejection — 644b55c

### Task 8: Invalid input rejection (remaining 15 tests)

Add error detection for remaining 15 invalid-YAML cases.
12/15 fixed via post-parse validation in `stream.rs` (19822fe).

- [x] Invalid mapping/sequence structure (5U3A, ZCZ6, ZL4Z) — 19822fe
- [x] Invalid flow syntax (N782) — 19822fe
- [x] Anchor misuse (CXX2, G9HC, SY6V) — 19822fe
- [x] Directive/document violations (5TRB, 9MQT, RXY3) — 19822fe
- [x] Other: tabs, tags, keys (QLJ7, W9L4, Y79Y[0]) — 19822fe
- [x] ZXT5 — pre-event validation in event.rs (spec §7.4) — 80c213d
- [x] S98Z — fixed blank-line indent condition in event.rs — 80c213d
- [x] Y79Y[2] — fixed via flow_seq tracking refinement — 80c213d

**Files:** `event.rs`, `stream.rs`

### Task 9: Final verification

- [x] Conformance test passes: 351/351 (0 failures) — 80c213d
- [x] All 922 unit tests pass: 0 regressions — 80c213d
- [x] `cargo clippy --all-targets` zero warnings — 80c213d
- [x] `cargo fmt --check` clean — 80c213d

## Decisions

- **Categorize by root cause, not test name.** Many test
  failures share the same parser bug (e.g., all block
  scalar folding failures likely stem from 1-2 production
  bugs). Fixing by category is more efficient than
  case-by-case.
- **Valid-YAML fixes before invalid-YAML rejection.** Fix
  the parser bugs (things we should parse but can't) before
  adding error detection (things we should reject but
  don't). Parser bugs are more likely to have cascading
  effects.
- **This plan completes the 100% target from the original
  plan.** The original Task 11 was accepted prematurely.
