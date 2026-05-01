**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-30

## Goal

Fix the BOM-less UTF-32 encoding-detection gap in `rlsp-yaml-parser` (Phase 2 §5.2 audit's single Non-conformant finding NC1) and add testing-methodology infrastructure that prevents recurrence of this class of incomplete-dispatch bug. The fix itself is mechanical (two additional `match` arms in `detect_encoding`); the larger value is the proptest encoding round-trip property that will catch any future addition of an encoding without corresponding dispatch coverage. The plan also adds spec-fixture tests covering every row of the §5.2 detection table so future readers see explicit per-row coverage rather than implicit "the existing tests probably hit each row."

## Context

- Phase 2 §5.2 audit at `.ai/audit/2026-04-30-phase2-prose/reconciliation-§5.2.md` (defect NC1) found that `rlsp-yaml-parser/src/encoding.rs:55-72` (`detect_encoding`) implements 7 of the 9 normative §5.2 spec-table rows. The two missing rows are the BOM-less UTF-32-BE (`[0x00, 0x00, 0x00, any] → Utf32Be`) and BOM-less UTF-32-LE (`[any, 0x00, 0x00, 0x00] → Utf32Le`) heuristics.
- Behavioral evidence from the audit: BOM-less UTF-32-BE input is misclassified as UTF-8 (decoded with embedded NUL bytes); BOM-less UTF-32-LE input is misclassified as UTF-16-LE because the existing `[a, 0x00, ..]` arm (line 68) is a strict prefix of the missing UTF-32-LE pattern.
- The existing comment at `encoding.rs:51-53` already articulates the prefix-overlap reasoning for BOM rows ("UTF-32 LE BOM `FF FE 00 00` is a superset of the UTF-16 LE BOM `FF FE`"); the same logic applies to BOM-less heuristics — UTF-32 patterns must precede UTF-16 patterns in match order.
- **Fix shape (from the audit):** insert two arms before the existing UTF-16 heuristic arms (lines 66-69):

  ```rust
  [0x00, 0x00, 0x00, a, ..] if *a != 0 => Encoding::Utf32Be,
  [a, 0x00, 0x00, 0x00, ..] if *a != 0 => Encoding::Utf32Le,
  ```

  The `*a != 0` guards prevent misclassifying all-zero input.
- **Why the original implementation missed this:** spec table transcribed top-down with BOM rows + UTF-16 heuristics covered, but BOM-less UTF-32 heuristics were never added. No test fixture exercised BOM-less UTF-32 input (real-world encoders typically emit a BOM with UTF-32, so the gap doesn't surface organically). The Phase 1 BNF audit verdicted [3] c-byte-order-mark as `Strict-conformant` correctly at the predicate level but did not test the dispatch table behaviorally — Phase 2's behavioral audit found the gap.
- **Testing-methodology gap to close** (per audit summary's recommendation): the existing `tests/encoding.rs` covers BOM-prefixed cases extensively but has zero fixtures for BOM-less UTF-32. The plan adds (a) spec-fixture tests covering every §5.2 detection-table row, and (b) a proptest property asserting that all five supported encodings produce identical parse output for the same source string. The property is a permanent guardrail against future encoding-dispatch incompleteness; adding a new encoding without dispatch coverage will fail the property automatically.
- **Existing test infrastructure to leverage:** the project uses `proptest` (per `lang-rust-testing.md`) and `rstest` for parameterized cases. Test inputs for the round-trip property are restricted to ASCII content so the encoders can be inline (each ASCII char maps trivially to all five encoding forms), avoiding a full Unicode encoder dependency.
- Spec reference: YAML 1.2.2 §5.2 detection table (local copy at `.ai/references/yaml-1.2.2-spec.md` lines 1542–1553; canonical at `https://yaml.org/spec/1.2.2/#52-character-encodings`).

## Steps

- [ ] Add the two BOM-less UTF-32 match arms to `detect_encoding` in `encoding.rs:55-72` with the `*a != 0` guards
- [ ] Update the doc comment at `encoding.rs:49-53` to mention the BOM-less heuristic ordering rationale (UTF-32 patterns must precede UTF-16 patterns for both BOM and BOM-less rows)
- [ ] Update the `Encoding::Utf32Le` and `Encoding::Utf32Be` variant doc comments at `encoding.rs:12,14` to mention both detection paths (BOM and null-byte heuristic), matching the existing `Utf16Le`/`Utf16Be` doc style
- [ ] Add inline `#[cfg(test)]` unit tests in `encoding.rs` covering: positive BOM-less UTF-32-BE detection, positive BOM-less UTF-32-LE detection, all-zero input falls through to UTF-8 (boundary), single-zero-followed-by-ASCII still detects as UTF-16-BE (regression — confirms the new arms don't shadow existing UTF-16 detection)
- [ ] Add spec-fixture parameterized integration test in `tests/encoding.rs` (`detect_encoding_covers_all_spec_rows`) with one `rstest` case per §5.2 detection-table row — both currently-covered rows (regression baseline) and the two newly-supported rows
- [ ] Add encoding round-trip proptest in `tests/encoding.rs` (`encoding_choice_invariant_under_parse`): for ASCII-only generated YAML strings, encode in all five supported encodings × {with-BOM, without-BOM} and assert `parse_events` produces the same event sequence in every form
- [ ] Verify build, clippy, all tests pass; verify the round-trip property catches a regression by manually deleting one of the new arms locally and observing the property fail (then restore)
- [ ] Mark plan Completed and commit

## Tasks

### Task 1: Fix BOM-less UTF-32 detection and add encoding test infrastructure

Single committable unit. The fix and the tests land together so the test infrastructure pins the new behavior atomically.

**Implementation:**

1. **Edit `rlsp-yaml-parser/src/encoding.rs`:**
   - In `detect_encoding` at lines 55-72, insert two arms between the UTF-16 BOM arms (lines 61-62) and the UTF-16 heuristic arms (lines 66-69):

     ```rust
     // BOM-less UTF-32 (must come BEFORE UTF-16 heuristics — the [a, 0x00, ..]
     // UTF-16-LE pattern is a strict prefix of [a, 0x00, 0x00, 0x00, ..],
     // so without these arms first, UTF-32-LE input would be misclassified
     // as UTF-16-LE).
     [0x00, 0x00, 0x00, a, ..] if *a != 0 => Encoding::Utf32Be,
     [a, 0x00, 0x00, 0x00, ..] if *a != 0 => Encoding::Utf32Le,
     ```

   - Update the doc comment at lines 49-53 to mention that the same prefix-overlap reasoning applies to BOM-less heuristics (UTF-32 before UTF-16).
   - Update the `Utf32Le` variant doc comment at line 12 from `/// UTF-32 little-endian (BOM `FF FE 00 00`).` to `/// UTF-32 little-endian (BOM `FF FE 00 00` or null-byte heuristic).` and the `Utf32Be` variant doc comment at line 14 from `/// UTF-32 big-endian (BOM `00 00 FE FF`).` to `/// UTF-32 big-endian (BOM `00 00 FE FF` or null-byte heuristic).` This brings the `Utf32` variant doc comments to parity with the existing `Utf16` variant doc comments at lines 8-10, which already state "or null-byte heuristic."

2. **Add `#[cfg(test)]` unit tests in `encoding.rs`** — append to the existing tests module (or create one if absent). Tests:

   - `detect_encoding_bom_less_utf32_be` — input `[0x00, 0x00, 0x00, 0x6B, ...]` (encoding of `"k: 1\n"`) detects as `Utf32Be` and decodes correctly to `"k: 1\n"`.
   - `detect_encoding_bom_less_utf32_le` — input `[0x6B, 0x00, 0x00, 0x00, ...]` detects as `Utf32Le` and decodes correctly.
   - `detect_encoding_all_zero_input_is_utf8` — input `[0x00; 16]` detects as `Utf8` (the `*a != 0` guard prevents misclassification as UTF-32-BE).
   - `detect_encoding_single_zero_then_ascii_is_utf16_be` — input `[0x00, 0x6B, 0x00, 0x3A, ...]` still detects as `Utf16Be` (regression: the new arms don't shadow existing UTF-16 detection because they require three leading zeros for BE or three trailing zeros for LE).

3. **Add spec-fixture parameterized test in `rlsp-yaml-parser/tests/encoding.rs`:**

   ```rust
   #[rstest]
   #[case::utf32_be_with_bom(&[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x6B], Encoding::Utf32Be)]
   #[case::utf32_le_with_bom(&[0xFF, 0xFE, 0x00, 0x00, 0x6B, 0x00, 0x00, 0x00], Encoding::Utf32Le)]
   #[case::utf16_be_with_bom(&[0xFE, 0xFF, 0x00, 0x6B], Encoding::Utf16Be)]
   #[case::utf16_le_with_bom(&[0xFF, 0xFE, 0x6B, 0x00], Encoding::Utf16Le)]
   #[case::utf8_with_bom(&[0xEF, 0xBB, 0xBF, 0x6B], Encoding::Utf8)]
   #[case::utf32_be_no_bom(&[0x00, 0x00, 0x00, 0x6B, 0x00, 0x00, 0x00, 0x3A], Encoding::Utf32Be)]
   #[case::utf32_le_no_bom(&[0x6B, 0x00, 0x00, 0x00, 0x3A, 0x00, 0x00, 0x00], Encoding::Utf32Le)]
   #[case::utf16_be_no_bom(&[0x00, 0x6B, 0x00, 0x3A], Encoding::Utf16Be)]
   #[case::utf16_le_no_bom(&[0x6B, 0x00, 0x3A, 0x00], Encoding::Utf16Le)]
   #[case::utf8_default(&[0x6B, 0x3A], Encoding::Utf8)]
   fn detect_encoding_covers_all_spec_rows(#[case] bytes: &[u8], #[case] expected: Encoding) {
       assert_eq!(detect_encoding(bytes), expected);
   }
   ```

   This test mechanically covers every row of the §5.2 detection table, including a regression baseline for the 7 currently-correct rows and explicit verification for the 2 newly-fixed rows. Future readers see the spec table reflected directly in the test fixture.

4. **Add encoding round-trip proptest in `rlsp-yaml-parser/tests/encoding.rs`:**

   ```rust
   proptest! {
       #[test]
       fn encoding_choice_invariant_under_parse(
           // Small ASCII-only YAML strings to keep the inline encoder trivial.
           s in "[a-z]{1,4}: [0-9]{1,4}\n"
       ) {
           let utf8_bytes = s.as_bytes();
           let utf8_events: Vec<_> = parse_events(s.as_str())
               .map(|r| r.map(|(e, _)| e))
               .collect();

           // Test each supported encoding × {with BOM, without BOM}.
           for encoding in [
               Encoding::Utf16Le, Encoding::Utf16Be,
               Encoding::Utf32Le, Encoding::Utf32Be,
           ] {
               for include_bom in [false, true] {
                   let bytes = encode_ascii_yaml(s.as_bytes(), encoding, include_bom);
                   let decoded = decode(&bytes).expect("ASCII-only input must decode");
                   let events: Vec<_> = parse_events(&decoded)
                       .map(|r| r.map(|(e, _)| e))
                       .collect();
                   prop_assert_eq!(&events, &utf8_events,
                       "encoding {:?} (bom={}) parse output differs from UTF-8",
                       encoding, include_bom);
               }
           }
       }
   }

   /// Test helper: encode ASCII-only `bytes` in the given encoding.
   /// Each ASCII byte maps trivially to one, two, or four output bytes.
   fn encode_ascii_yaml(bytes: &[u8], encoding: Encoding, include_bom: bool) -> Vec<u8> {
       // Implementation per encoding: for ASCII-only input, each char maps to:
       // UTF-16-LE: [c, 0x00]; UTF-16-BE: [0x00, c]
       // UTF-32-LE: [c, 0x00, 0x00, 0x00]; UTF-32-BE: [0x00, 0x00, 0x00, c]
       // BOMs prepended where include_bom=true.
       // (full implementation in test file)
   }
   ```

   The property fails any time an encoding's dispatch is incomplete or produces wrong output. Restricting the proptest's input to ASCII keeps the inline encoder trivial; multi-byte coverage is provided by the manual fixture tests.

5. **Verify the property catches the regression manually before committing:** delete one of the new arms, run `cargo test`, observe the proptest fails, restore. (This step happens locally; not committed.)

**Acceptance criteria:**

- [ ] `detect_encoding` at `encoding.rs:55-72` contains the two new BOM-less UTF-32 arms inserted between the UTF-16 BOM arms and the UTF-16 heuristic arms.
- [ ] Each new arm has the `*a != 0` guard.
- [ ] Doc comment at `encoding.rs:49-53` mentions BOM-less heuristic ordering rationale.
- [ ] `Encoding::Utf32Le` and `Encoding::Utf32Be` variant doc comments at `encoding.rs:12,14` both mention the null-byte heuristic detection path in addition to the BOM, matching the doc style of the existing `Utf16Le`/`Utf16Be` variants at lines 8-10.
- [ ] Four new inline unit tests in `encoding.rs` (`detect_encoding_bom_less_utf32_be`, `detect_encoding_bom_less_utf32_le`, `detect_encoding_all_zero_input_is_utf8`, `detect_encoding_single_zero_then_ascii_is_utf16_be`) all pass.
- [ ] New parameterized test `detect_encoding_covers_all_spec_rows` in `tests/encoding.rs` passes with one `rstest` case per §5.2 detection-table row (10 cases total: 5 BOM rows including UTF-8 BOM + 4 BOM-less UTF-16/UTF-32 heuristic rows + 1 UTF-8 default row).
- [ ] New proptest `encoding_choice_invariant_under_parse` in `tests/encoding.rs` runs `proptest`'s default 256 iterations and passes; a manual local check (delete one new arm → property fails) verifies the proptest catches the regression class.
- [ ] `cargo build`, `cargo clippy --all-targets`, and `cargo test -p rlsp-yaml-parser` all pass with zero warnings.
- [ ] `cargo fmt --check` passes.
- [ ] Single commit with conventional format `fix(rlsp-yaml-parser): detect BOM-less UTF-32 input per YAML §5.2 table`.

## Decisions

- **Single commit, atomic fix + tests.** The two-line fix is small but fundamentally changes behavior for previously-misclassified input; landing the spec-fixture tests and round-trip proptest together with the fix makes the safety net visible from the same diff. Splitting would either ship the fix without the safety net (regression risk) or ship the tests against unfixed code (misleading test failure).
- **Tests in `tests/encoding.rs`, not in a new file.** The existing integration tests for encoding live there; co-locating preserves the testing-by-feature organization the project's `lang-rust-testing.md` describes.
- **Restrict the round-trip proptest to ASCII inputs.** Inline encoders for non-ASCII strings would require either a real Unicode encoder dependency or a substantial fixture. ASCII covers the dispatch property (the bug class is "wrong encoding chosen," not "encoded incorrectly"); multi-byte coverage already exists via manual fixture tests in the same file (e.g., the existing UTF-16 BOM-stripped tests use multi-byte content).
- **Don't update `docs/yaml-spec-conformance.md` for [3] c-byte-order-mark in this plan.** The doc-rewrite is consolidated as a separate plan in step 5 of the post-Phase-2 orchestration entry. Touching it here would scope-creep and risk merge conflicts with the larger rewrite.
- **Don't add tests for behavior outside §5.2 in this plan.** The audit surfaced other Lenient findings (double BOM at stream start, etc.) — those have their own follow-up entries and should be fixed separately so each commit has a focused scope and an independent rollback.
- **Verify regression-catching manually but don't commit a "delete arm to verify" step.** The proptest's strength comes from being correct in the committed state. A "delete to verify" automated test would be a mutation-test fixture and adds infrastructure outside this plan's scope. Manual verification before committing is sufficient to confirm the proptest's discriminating power.
- **Keep `[3] c-byte-order-mark` Phase 1 verdict as-is.** Phase 1 verdicted `Strict-conformant` at the predicate level (BOM character correctly recognized) — that verdict was not wrong, it was orthogonal to the dispatch-coverage gap this plan fixes. The fix here doesn't retroactively change Phase 1's verdict; the audit's NC1 finding stays attributed to §5.2 (Phase 2) where it surfaced.

## Non-Goals

- **Other Phase 2 Lenient findings.** L1 (double BOM at stream start), L4 (`%TAG` comment-after-prefix), L5–L8 (verbatim-tag laxity), L9–L10 (signed octal/hex), L11 (1 MiB cap bypass), L12–L17 (error-position imprecision) all have their own follow-up entries and are out of scope.
- **Conformance doc rewrite.** Deferred to the consolidated doc-rewrite plan (post-Phase-2 step 5).
- **Testing-methodology coverage-gap audit.** Filed as a separate follow-up entry (committed at `2e19b13`); will produce a survey document at `.ai/audit/<future-date>-testing-methodology-gaps/`.
- **Differential testing against libyaml/PyYAML.** Out of scope for this plan; the round-trip proptest catches the bug class without adding cross-implementation dependencies.
- **Encoder for non-ASCII Unicode in the proptest.** ASCII coverage is sufficient for the dispatch-coverage property; non-ASCII encoder work is deferred to the testing-methodology audit follow-up.
- **Phase 1 conformance doc updates.** [3] c-byte-order-mark stays at its current Phase 1 `Strict-conformant` verdict; the gap was at the dispatch level, not at the predicate level.
