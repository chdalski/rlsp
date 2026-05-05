**Repository:** root
**Status:** Completed (2026-05-05)
**Created:** 2026-05-05

## Goal

Reject double BOM at stream start per YAML 1.2.2 §5.2 production [202], closing the last Phase 2 Lenient entry (L1). Currently, two BOM-stripping code paths both fire for the first document: `scan_line` (when `is_first=true`) strips a BOM at stream start, AND `signal_document_boundary` strips a BOM at the first document's prefix position. A stream starting with `\u{FEFF}\u{FEFF}` silently strips both BOMs (6 bytes). Inter-document positions correctly reject double BOMs. After this fix, the behavior is uniform: at most one BOM is stripped at any document prefix, and a second consecutive BOM at stream start produces a parse error.

## Context

- **Spec production [202]:** `l-document-prefix ::= c-byte-order-mark? l-comment*` — at most one BOM at any document prefix. A second U+FEFF is invalid.
- **Phase 2 audit finding:** `.ai/audit/2026-04-30-phase2-prose/reconciliation-§5.2.md` Defect 2 (REQ-§5.2-9) — B found double BOM at stream start silently accepted; A's test only covered inter-document (correctly rejected).
- **Two BOM-strip paths:**
  1. `lines.rs:115-127` — `scan_line` strips leading BOM when `is_first=true` (first line of stream)
  2. `lines.rs:292-305` — `signal_document_boundary` strips leading BOM from the primed `next` line at every document-prefix position
- **Overlap:** For the first document, both paths fire: `scan_line` strips one BOM during initial `LineBuffer::new` priming, then `signal_document_boundary` strips a second if present. Inter-document transitions only run path 2, so a second BOM there is preserved and rejected by the body check.
- **Fix approach:** Remove BOM stripping from `scan_line` (`is_first` path) and let `signal_document_boundary` be the single BOM-strip site. The `signal_document_boundary` call already runs for the first document (in `step_between_docs` / initial state). This eliminates the overlap — one site, one strip, uniformly. If a second BOM follows, it remains in the content and is rejected as a non-`ns-char` byte by the existing c-printable enforcement or by explicit check.
- **Alternative approach:** Keep both paths but add a `bom_already_stripped` flag that prevents the second strip. The flag approach is more complex (mutable state, two sites to maintain) and the single-site approach is simpler (remove code rather than add code).
- **Performance:** Removes one conditional from `scan_line` (slightly fewer instructions on first line). Zero regression.
- **Spec reference:** [YAML 1.2.2 §5.2](https://yaml.org/spec/1.2.2/#52-character-encodings)
- **User directive:** "security hardened, fine. Lenient not fine."
- **Existing test:** `tests/encoding.rs:317-325` — `parse_events_rejects_double_bom_at_document_prefix` tests the inter-doc case. No test for stream-start double BOM.

## Steps

- [x] Remove BOM stripping from `scan_line` first-line path
- [x] Ensure `signal_document_boundary` is called before first document processing
- [x] Add integration test for double BOM at stream start → error
- [x] Update follow-up queue: remove L1 entry
- [x] Verify all tests pass
- [x] Mark plan Completed and commit

## Tasks

### Task 1: Unify BOM stripping to single site

**Completed:** commit `6f55282` (2026-05-05)

Remove the `is_first` BOM-stripping path from `scan_line` and rely solely on `signal_document_boundary` for BOM stripping at all document prefixes (including stream start).

- [x] In `scan_line` (lines 115-127), remove the `is_first` BOM-strip conditional. The `is_first` parameter may become unused — if so, remove it from the function signature and all call sites. If `is_first` serves other purposes beyond BOM stripping, keep the parameter but remove only the BOM logic.
- [x] Update or remove the existing unit test `bom_stripped_line_offset_starts_after_bom_bytes` at `lines.rs:789-798` — it directly tests the `is_first` BOM path being removed. Update it to test BOM stripping via `signal_document_boundary` instead, or remove it if the new integration tests cover the behavior. Also verify `bom_not_stripped_on_non_boundary_mid_content_line` still holds.
- [x] Verify that `signal_document_boundary` is called for the first document before the first line is consumed. Trace the call path from `LineBuffer::new` through the event iterator's initial state to confirm the first document's BOM is stripped by `signal_document_boundary`.
- [x] Add integration test: `"\u{FEFF}\u{FEFF}key: v\n"` (double BOM at stream start) → produces at least one parse error (the second BOM is not stripped and appears as illegal content)
- [x] Add integration test: `"\u{FEFF}key: v\n"` (single BOM at stream start) → parses correctly (regression guard)
- [x] Add integration test: `"key: v\n...\n\u{FEFF}\u{FEFF}key: b\n"` (double BOM at inter-doc) → still produces error (existing behavior preserved)
- [x] Existing `cargo test -p rlsp-yaml-parser` passes with zero failures
- [x] `cargo clippy --all-targets` passes with zero warnings
- [x] `cargo fmt --check` passes
- [x] Remove double-BOM entry (L1) from `project_followup_plans.md`
- [x] Single commit: `fix(rlsp-yaml-parser): reject double BOM at stream start`

## Decisions

- **Single-site approach (remove `scan_line` BOM strip) over flag approach.** Removing code is simpler than adding flags. One strip site means one place to maintain, one mental model. The flag approach (add `bom_already_stripped: bool`) adds mutable state to `LineBuffer` for a case that never needs to fire.
- **`signal_document_boundary` is the sole BOM-strip site.** It already handles inter-document BOM stripping correctly. Making it the only site for ALL document prefixes (including stream start) unifies behavior.
- **No `yaml-spec-conformance.md` updates (including test citations).** The `[3] c-byte-order-mark` entry's test list will be stale (missing the new stream-start double BOM test). Per established precedent across all 8 prior conformance fixes in this session, individual doc patches are deferred to the holistic doc rewrite.
- **No feature-log entry.** Encoding/BOM handling conformance fix, not user-facing feature.
- **No conformance doc updates.** Holistic rewrite deferred.
- **Performance: zero or slight improvement.** Removes one conditional from the first-line path.

## Non-Goals

- **BOM-less UTF-32 encoding detection.** Already fixed at commit `f16a0cf`.
- **Conformance doc rewrite.** Deferred.
