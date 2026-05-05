**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-05

## Goal

Fix all 21 remaining testing methodology gaps from the 2026-05-05 audit. Task 1 covers `rlsp-yaml-parser` (15 gaps: boundary fixtures, standalone unit tests, proptests, cross-schema check). Task 2 covers `rlsp-yaml` and `rlsp-fmt` (6 gaps: formatter/code-action invariants, printer edge cases). All test-only — no production code changes.

## Context

- **Audit source:** `.ai/audit/2026-05-05-testing-methodology-gaps/summary.md`
- **Already fixed:** E2, S2, S3, C2, T1 (commit `ad5c426`)
- **Invalidated:** Y1 (false positive — I10 already catches via loader-injected tags), D1 (downgraded — YAML Test Suite is the differential reference)
- **Remaining 21 gaps by crate:**
  - `rlsp-yaml-parser`: E1, E3, C1, C3, C4, S1, S4, K1, T2, P1, P2, P3, P5, D2 (14 gaps) + consolidation
  - `rlsp-yaml`: Y2, Y3, Y4, P4 (4 gaps)
  - `rlsp-fmt`: F1, F2, F3 (3 gaps)
- **Proptest dependency:** Already in `rlsp-yaml-parser` dev-deps. NOT in `rlsp-yaml` or `rlsp-fmt` — Task 2 needs to add it where proptests are introduced.
- **Performance:** Test-only changes. Zero runtime impact.

## Steps

- [x] Add parser test hardening (Task 1)
- [ ] Add rlsp-yaml + rlsp-fmt test hardening (Task 2)
- [ ] Verify all tests pass
- [ ] Commit

## Tasks

### Task 1: rlsp-yaml-parser test hardening (14 gaps)

**Completed:** commit `a873ef3` (2026-05-05)

Add boundary fixtures, standalone unit tests, proptests, and a cross-schema structural check.

**Boundary fixtures (7 gaps):**
- [x] **GAP-E1:** Add fixture in `tests/encoding.rs` for `[0xEF, 0xBB, 0xBF]` (UTF-8 BOM, 3 bytes, no content) asserting UTF-8 detection. Low.
- [x] **GAP-E3:** Add fixture in `tests/encoding.rs` for BOM-less UTF-16 LE odd-length input (e.g., `[0x41, 0x00, 0x42]`) asserting `LoadError::TruncatedUtf16`. Low.
- [x] **GAP-C1:** Add inline `#[cfg(test)]` fixtures in `src/chars.rs` for `is_c_printable` at `'\u{D7FF}'` (accepted), `'\u{E000}'` (accepted), `'\u{FFFD}'` (accepted), `'\u{FFFE}'` (rejected). Low.
- [x] **GAP-C3:** Add inline `#[cfg(test)]` fixture in `src/chars.rs` for `decode_escape('\t')` (literal tab as escape code) asserting it returns `'\t'`. Low.
- [x] **GAP-C4:** Add inline `#[cfg(test)]` fixtures in `src/chars.rs` for `is_ns_char('\u{10000}')` and `is_ns_char('\u{10FFFF}')` both returning `true`. Low.
- [x] **GAP-S1:** Add fixture in `tests/schema_resolution.rs` for a long octal string (`0o` + 100 `7`s) asserting `!!int`. Low.
- [x] **GAP-S4:** Add parameterized test in `tests/schema_resolution.rs` loading `[1, 2, 3]` under all three schemas asserting root tag is `!!seq` in all cases. Low.

**Standalone unit tests (2 gaps):**
- [x] **GAP-T2:** Add inline `#[cfg(test)]` fixtures in `src/event_iter/directive_scope.rs` for `percent_decode`: `%2F` → `/`, `%GG` → pass-through, truncated `%0` → pass-through. Also add end-to-end test: `!!foo%2Fbar scalar` asserting tag expands to `tag:yaml.org,2002:foo/bar`. Medium.
- [x] **GAP-K1:** Add fixtures in `tests/schema_resolution.rs` for `Schema::Json` loading `![1, 2]` and `!{a: b}` asserting bare `!` local tag. Low.

**Proptests (4 gaps):**
- [x] **GAP-P1:** Add proptest in `tests/` generating simple YAML (key-value pairs with ASCII scalars), parsing to events, rendering to canonical form, re-parsing, and asserting event sequences match. Use `proptest::string::string_regex("[a-z]{1,8}")` for keys/values. Medium.
- [x] **GAP-P2:** Add proptest in `src/chars.rs` inline tests using `proptest::char::any()` asserting: `is_ns_tag_char_single(c) → is_ns_uri_char_single(c)` and `is_ns_uri_char_single(c) → is_c_printable(c)`. Medium.
- [x] **GAP-P3:** Add proptest in `tests/schema_resolution.rs` generating random ASCII scalars asserting `resolve_scalar(s, schema)` returns same result when called twice. Low.
- [x] **GAP-P5:** Extend the existing encoding proptest strategy in `tests/encoding.rs` to include non-ASCII unicode scalars (use `proptest::char::range('\u{0080}', '\u{07FF}')` for 2-byte UTF-8). Low.

**Cross-schema check (1 gap):**
- [x] **GAP-D2:** Add parameterized test in `tests/schema_resolution.rs` loading the same input under all three schemas and asserting structural equality (node kinds, children count, anchor names) — only tag fields may differ. Medium.

**Verification:**
- [x] `cargo test -p rlsp-yaml-parser` passes
- [x] `cargo clippy --all-targets` passes
- [x] `cargo fmt --check` passes

### Task 2: rlsp-yaml + rlsp-fmt test hardening (6 gaps)

Add formatter/code-action invariants and printer edge-case tests.

**rlsp-yaml (4 gaps):**
- [ ] **GAP-Y2:** Add fixture in `tests/formatter_conformance.rs` (or new `tests/formatter_tags.rs`) with inputs containing explicit tags (`!!custom`, `!local`) asserting the tag string appears unchanged in formatted output. Medium.
- [ ] **GAP-Y3:** Audit existing code-action fixture files; for each action category missing block or flow context variants, add fixture pairs (input + expected output). Acceptance criterion: each code-action category has at least one block-context fixture and one flow-context fixture. Medium.
- [ ] **GAP-Y4:** Add a proptest that applies each available code action twice to selected inputs and asserts the second application is a no-op (proptest dev-dep is added for GAP-P4). Low.
- [ ] **GAP-P4:** Add proptest (add `proptest` to `rlsp-yaml` dev-deps) generating random `YamlFormatOptions` (varying `print_width`, `indent_width`, `single_quote`) combined with yaml-test-suite inputs, asserting `format(format(input)) == format(input)`. Medium.

**rlsp-fmt (3 gaps):**
- [ ] **GAP-F1:** Add unit test in `src/printer.rs` constructing `Doc::Group(Doc::FlatAlt { flat: short_doc, expanded: long_doc })` where `short_doc` fits, asserting flat mode and flat variant used. Medium.
- [ ] **GAP-F2:** Add unit test in `src/printer.rs` calling `indent_width(3, true)` asserting returns `3` (not `3 * tab_stop`), with comment explaining tab-as-1-column. Low.
- [ ] **GAP-F3:** Add unit test in `src/printer.rs` asserting `format(Doc::Concat(vec![]), ...)` returns empty string. Low.

**Verification:**
- [ ] `cargo test -p rlsp-yaml` passes
- [ ] `cargo test -p rlsp-fmt` passes
- [ ] `cargo clippy --all-targets` passes
- [ ] `cargo fmt --check` passes

## Decisions

- **Two tasks by crate boundary.** Task 1 stays in `rlsp-yaml-parser` (one test suite, one dev-dep set). Task 2 covers `rlsp-yaml` and `rlsp-fmt` (both downstream crates, may need proptest added to dev-deps).
- **No production code changes.** If any test reveals a real bug, file it as a separate follow-up.
- **Proptest strategies should be simple.** Use narrow strategies (ASCII, small ranges) for initial proptests. Broader strategies can be added later once the infrastructure is proven.
- **GAP-Y3 scope:** The developer audits existing fixtures and adds missing variants. The exact count of new fixtures depends on what's missing — the acceptance criterion is "each action category has at least one block and one flow context fixture."

## Non-Goals

- Fixing bugs discovered by new tests (separate plans).
- GAP-D1 (downgraded — not cost-effective).
- GAP-Y1 (false positive).
- Previously fixed: E2, S2, S3, C2, T1.
