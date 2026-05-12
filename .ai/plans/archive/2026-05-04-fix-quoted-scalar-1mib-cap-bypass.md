**Repository:** root
**Status:** Completed (2026-05-04)
**Created:** 2026-05-04

## Goal

Close the 1 MiB quoted-scalar length-cap bypass identified by the Phase 2 error-and-limits audit (defect L11). The documented cap — "scalar exceeds maximum allowed length (1 MiB)" — fires only on the double-quoted escape-decode (owned) path. Double-quoted scalars without escapes (borrow path) and ALL single-quoted scalars bypass it entirely. This plan makes the cap universal across both quoted-scalar styles so the documented limit is the actual limit — a 100 MiB raw quoted scalar is rejected, not silently passed through. DoS-relevant: without the fix, an adversarial input can force the parser to process an arbitrarily-long quoted scalar without hitting any documented resource limit.

## Context

- Phase 2 error-and-limits audit at `.ai/audit/2026-04-30-phase2-prose/reconciliation-error-and-limits.md` (defect L11) found the double-quoted borrow-path bypass: `lexer/quoted.rs:641-646` and `:709-714` check `if buf.len() > 1_048_576` only when `owned.as_mut()` returns `Some`. On the borrow path (`owned == None`, i.e., no `\` escapes in the scalar), `borrow_end` advances unchecked.
- Codebase analysis for this plan reveals a broader gap: `try_consume_single_quoted` at `lexer/quoted.rs:27-175` has **zero** length checks on any path (neither the single-line return at line 72 nor the multi-line accumulation loop starting at line 79). The `unescape_single_quoted` helper at line 461 also has no cap.
- The fix applies uniformly to both quoted styles: add a length check wherever the accumulated scalar content (borrow length or owned buffer length) can grow past 1 MiB.
- **Performance consideration (from prior-session analysis):** the borrow path is the fast path — no allocation, no escape decoding; `borrow_end` advances only at `memchr2` hit sites (not per byte). Adding a comparison per hit (`if borrow_end > 1_048_576`) is negligible overhead. For single-quoted, the multi-line loop at lines 79-175 accumulates `owned.push_str(...)` per continuation line; the cap check fires per line, not per byte. Perf impact on normal scalars (well under 1 MiB) is unmeasurable.
- The error message "scalar exceeds maximum allowed length (1 MiB)" is already user-facing for the existing owned-path cap. No change to the message; the cap is the same value and meaning, just uniformly enforced.
- The literal `1_048_576` currently appears at three sites in `lexer/quoted.rs` (lines 606, 641, 709). The plan centralizes it as `pub const MAX_SCALAR_LEN: usize = 1_048_576` in `limits.rs`, matching the existing Tier 1 constant pattern (`MAX_COMMENT_LEN`, `MAX_TAG_LEN`, etc.). All existing and new cap-check sites reference the constant.
- Spec reference: YAML 1.2.2 does not mandate a scalar length limit; this is an implementation-defined resource limit (same category as `MAX_COLLECTION_DEPTH=512`, `MAX_ANCHOR_NAME_BYTES=1024`, etc.). The limit is DoS protection, not spec conformance.

## Steps

- [x] Add `pub const MAX_SCALAR_LEN: usize = 1_048_576` to `limits.rs` with a doc comment matching the existing constant style
- [x] Replace the three existing `1_048_576` literals in `lexer/quoted.rs` (lines 606, 641, 709) with `MAX_SCALAR_LEN`
- [x] Add borrow-path length check in `scan_double_quoted_line` at the two sites where `borrow_end` advances (line 651 and line 726 in current code)
- [x] Add length check in `try_consume_single_quoted` multi-line accumulation loop (where `owned.push_str(...)` and `owned.push(...)` grow the buffer)
- [x] Add length check in `try_consume_single_quoted` single-line return path (where `value.into_cow(body_start)` returns a borrow whose length is unchecked)
- [x] Add inline unit tests: double-quoted borrow-path cap fires; single-quoted single-line cap fires; single-quoted multi-line cap fires
- [x] Add integration test via `parse_events`: a >1 MiB double-quoted scalar with no escapes produces an error
- [x] Add a row for the 1 MiB quoted-scalar cap to the "Parser limits" table in `rlsp-yaml-parser/docs/architecture.md`
- [x] Update "Security Limits" entry in `rlsp-yaml-parser/docs/feature-log.md`: count "Seven" → "Eight", add `MAX_SCALAR_LEN` to the enumerated list
- [x] Verify build, clippy, all tests pass
- [x] Mark plan Completed and commit

## Tasks

### Task 1: Enforce 1 MiB length cap uniformly across both quoted-scalar styles

Add `MAX_SCALAR_LEN` constant to `limits.rs` (matching the existing Tier 1 pattern) and enforce it on all quoted-scalar accumulation paths: double-quoted borrow path, double-quoted owned path (already partially enforced — replace literals with the constant), and all single-quoted paths (currently uncapped). Update `architecture.md` "Parser limits" table.

**Completed:** commit `00c2f5e` (2026-05-04)

- [x] `MAX_SCALAR_LEN` constant in `limits.rs` with value `1_048_576` and doc comment
- [x] Existing `1_048_576` literals in `lexer/quoted.rs` replaced with constant
- [x] Double-quoted borrow-path cap check at both accumulation sites in `scan_double_quoted_line`
- [x] Single-quoted single-line return-path cap check in `try_consume_single_quoted`
- [x] Single-quoted multi-line accumulation-loop cap check in `try_consume_single_quoted`
- [x] Inline unit test: `double_quoted_borrow_path_length_cap_fires`
- [x] Inline unit test: `single_quoted_single_line_length_cap_fires`
- [x] Inline unit test: `single_quoted_multi_line_length_cap_fires`
- [x] Regression: existing `double_quoted_length_cap_exceeded_raises_error` still passes
- [x] Integration test in `tests/scalar_limits.rs`: `parse_events_rejects_overlong_double_quoted_scalar_without_escapes`
- [x] "Parser limits" table in `rlsp-yaml-parser/docs/architecture.md` updated with new row
- [x] "Security Limits" entry in `rlsp-yaml-parser/docs/feature-log.md` updated: count "Seven" → "Eight", `MAX_SCALAR_LEN` added to the enumerated list
- [x] `cargo build`, `cargo clippy --all-targets`, `cargo test -p rlsp-yaml-parser` — zero warnings, zero failures
- [x] `cargo fmt --check` passes
- [x] Single commit: `fix(rlsp-yaml-parser): enforce 1 MiB cap on all quoted-scalar paths`

## Decisions

- **Cap both quoted styles, not just double-quoted.** The audit finding (L11) was specifically about the double-quoted borrow path, but the single-quoted gap is the same bug class (documented limit not enforced). The error message "scalar exceeds maximum allowed length" doesn't qualify by style; making the cap universal is the correct scope. Plain and block scalars remain unbounded (they're bounded by the caller's input `&str` size, which is the caller's responsibility — the parser doesn't own that memory).
- **Same cap value (1 MiB) for single-quoted.** Introducing a different cap would create user confusion. The existing constant `1_048_576` is used in three places; adding it to single-quoted paths uses the same constant.
- **Extract the constant into `limits.rs` as `MAX_SCALAR_LEN`.** The literal `1_048_576` currently appears at 3 sites and will grow to 5+ with this fix. Centralizing as `pub const MAX_SCALAR_LEN: usize = 1_048_576` in `limits.rs` matches the existing Tier 1 pattern (MAX_COMMENT_LEN, MAX_TAG_LEN, etc.) and puts the value in one documented location. All existing and new cap-check sites reference the constant. Not runtime-configurable — matches the other 7 parser-side limits which are also hardcoded constants; runtime configurability via `LoaderOptions` is a separate future enhancement if needed.
- **Cap check fires per `memchr` hit or per continuation line, not per byte.** This keeps the perf impact at zero for normal scalars. The check is a comparison, not a computation — it adds no measurable cost to the hot path.
- **Don't add the cap to plain or block scalars in this plan.** Those styles borrow directly from the source `&str`; the caller controls the input size. Adding a parser-side cap would be a separate design decision about whether the parser should reject large-but-valid documents.
- **Don't update `docs/yaml-spec-conformance.md` in this plan.** The conformance doc rewrite is queued as a holistic follow-up (post-Phase-2 orchestration step 5) that incorporates all Phase 1 + Phase 2 findings, including this cap enforcement change. Adding a per-fix incremental doc update here would create churn that the holistic rewrite has to reconcile — one doc-rewrite plan covering everything is cleaner than N per-fix doc patches. The fix commit's message documents the behavioral change; the doc-rewrite will propagate it.
- **Don't make the cap configurable in this plan.** The existing 1 MiB hardcoded value matches the three existing call sites. Making it configurable via `LoaderOptions` is a separate enhancement that the post-Phase-2 design-decisions batch can address.
- **Tests in `lexer/quoted.rs` inline module, not in a new file.** The existing cap test (`double_quoted_length_cap_exceeded_raises_error`) lives there; co-locating follows the same pattern. The integration test goes in `tests/` per the integration-testing rule.

## Non-Goals

- **Other Phase 2 Lenient findings.** Double BOM at stream start, `%TAG` comment-after-prefix, verbatim-tag laxity, signed octal/hex, error-position imprecision all have their own follow-up entries.
- **Conformance doc rewrite.** Deferred to the consolidated doc-rewrite plan.
- **Configurable scalar length limit.** Deferred to the post-Phase-2 design-decisions batch.
- **Plain and block scalar length caps.** Different concern (caller-controlled input size vs parser-enforced resource limit); separate design decision.
- **Testing-methodology audit.** Filed as a separate follow-up (`2e19b13`).
