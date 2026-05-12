**Repository:** root
**Status:** Completed (2026-05-05)
**Created:** 2026-05-05

## Goal

Fix five testing methodology gaps identified in the 2026-05-05 audit by adding targeted test cases to existing test files in `rlsp-yaml-parser`. All are small, independent test additions — no production code changes. Covers: GAP-E2 (encoding BOM ordering), GAP-S2 (`1.e5` float), GAP-S3 (`0e5` JSON float), GAP-C2 (NEL-adjacent C1 boundaries), GAP-T1 (primary `!` handle percent-decode validation).

## Context

- **Audit source:** `.ai/audit/2026-05-05-testing-methodology-gaps/summary.md` — 28 gaps identified, 1 false positive (GAP-Y1). This plan addresses 1 Medium (E2) + 4 Medium (S2, S3, C2, T1) = 5 gaps.
- **GAP-E2 (Medium):** `tests/encoding.rs` has UTF-32-BE and UTF-16-BE BOM cases but no test explicitly documenting that `[0x00, 0x00, 0xFE, 0xFF]` must be UTF-32-BE, not UTF-16-BE. The existing rstest cases indirectly guard against reordering but the intent is not self-documenting.
- **GAP-S2 (Medium):** `1.e5` (trailing-dot-with-exponent) is a valid Core float per spec but has no test. Existing tests cover `3.14`, `.5`, `1e10` but not the empty-frac-with-exponent path.
- **GAP-S3 (Medium):** `0e5` (bare-zero-with-exponent) is a valid JSON float but has no test. Existing tests cover `1e10` but not the `strip_prefix('0')` + exponent path.
- **GAP-C2 (Medium):** U+0085 (NEL) is tested as accepted in `chars.rs`, but U+0084 and U+0086 (immediately adjacent C1 codepoints) have no tests. An off-by-one in the NEL exclusion guard would be invisible.
- **GAP-T1 (Medium):** The `resolve_tag()` path for the primary `!` handle with an invalid percent-decoded suffix is not tested. The `!!` and named handle paths are tested for this case but `!suffix` is not.
- **All files are in `rlsp-yaml-parser`:** `tests/encoding.rs`, `src/schema.rs` (or `tests/schema_resolution.rs`), `src/chars.rs`, `tests/smoke/` (or `tests/schema_resolution.rs`).
- **Performance:** Test-only changes. Zero runtime impact.

## Steps

- [x] Add encoding ordering test (GAP-E2)
- [x] Add schema resolution edge-case tests (GAP-S2, GAP-S3)
- [x] Add character boundary tests (GAP-C2)
- [x] Add primary handle percent-decode test (GAP-T1)
- [x] Verify all tests pass
- [x] Commit

## Tasks

### Task 1: Add five targeted test cases

**Completed:** commit `845aa87` (2026-05-05)

All five gaps are small, independent test additions to existing files in `rlsp-yaml-parser`. One commit.

- [x] **GAP-E2:** Add test `utf32_be_bom_takes_priority_over_utf16_be_prefix` in `tests/encoding.rs`. Feed `[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x0A]` (UTF-32-BE BOM + newline content) and assert `Encoding::Utf32Be`. Add a comment: the 4-byte UTF-32-BE BOM `[00 00 FE FF]` contains the 2-byte UTF-16-BE BOM `[FE FF]` at offset 2 — the detection must check 4-byte BOMs before 2-byte BOMs.
- [x] **GAP-S2:** Add rstest case `trailing_dot_with_exponent` for `1.e5` in `tests/schema_resolution.rs`. Assert it resolves to `!!float` under `Schema::Core`.
- [x] **GAP-S3:** Add rstest case `zero_with_exponent` for `0e5` in `tests/schema_resolution.rs`. Assert it resolves to `!!float` under `Schema::Json`.
- [x] **GAP-C2:** Add inline `#[cfg(test)]` fixtures in `src/chars.rs` for `find_non_c_printable` (or the appropriate boundary-test section): U+0084 (`\xC2\x84`) → rejected (returns `Some`), U+0086 (`\xC2\x86`) → rejected (returns `Some`), U+0085 (NEL, `\xC2\x85`) → accepted (returns `None`). The NEL test may already exist — if so, just add the two adjacent-codepoint tests.
- [x] **GAP-T1:** Add a test for the primary `!` handle with invalid percent-decoded suffix. Use a YAML input like `%TAG ! tag:example.com,\n---\n!%20invalid val\n` and assert it produces a parse/load error (the `%20` decodes to space, which is not a valid URI character). Place in `tests/smoke/directives.rs` alongside existing tag validation tests.
- [x] `cargo test -p rlsp-yaml-parser` passes (all new tests pass, no regressions)
- [x] `cargo clippy --all-targets` passes
- [x] `cargo fmt --check` passes
- [x] **Consolidation check:** Review `tests/encoding.rs` and `tests/smoke/directives.rs` for duplicate helpers, overlapping test patterns, or structural issues introduced by repeated additions across prior plans. Note whether consolidation is needed or "reviewed, no consolidation required."

## Decisions

- **Single task, single commit.** All five are small, independent test additions to the same crate. Splitting into five tasks would add overhead with no review benefit — the reviewer can evaluate each gap independently within one diff.
- **Place tests near existing patterns.** Each test goes into the file that already has the closest related tests, following the existing naming and structural conventions. No new test files.
- **No production code changes.** If any test reveals a real bug (e.g., `1.e5` actually fails to resolve as float), that's a separate fix plan — this plan only adds tests.

## Non-Goals

- Fixing bugs discovered by new tests (separate plans if needed).
- Adding the differential testing harness (GAP-D1, separate plan).
- Low-severity gaps (GAP-E1, E3, C1, C3, C4, S1, S4, K1, P3, P5, Y4, F2, F3) — deferred.
- The false-positive GAP-Y1 — already resolved in the audit document.
