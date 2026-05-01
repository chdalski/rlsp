---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: Reconciliation
section: §5.2
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §5.2 Character Encodings

A enumerated 8 requirements; B enumerated 10. Subject-matter overlap covers 7 requirements both auditors verdicted identically as `Strict-conformant`. Each auditor independently found one substantive defect that the other did not test:

- **A's defect (Non-conformant):** BOM-less encoding detection table is missing the UTF-32-BE and UTF-32-LE arms. B's REQ-§5.2-5 verdicted `Strict-conformant` because B's test set did not include BOM-less UTF-32 inputs.
- **B's defect (Lenient):** Double BOM at stream start is silently accepted (both BOMs stripped). A's REQ-§5.2-4 sub-case 4 only tested double-BOM at inter-document prefix (correctly rejected); A did not test the stream-start position.

These are not inter-auditor disagreements — they are complementary findings exposing a coverage gap that one auditor caught and the other missed. The lead accepts both findings as final; no `[NEEDS USER REVIEW]` flags.

## Final Verdict Tally

- `Strict-conformant`: 8
- `Stricter-than-spec`: 0
- `Lenient`: 1 (double BOM at stream start)
- `Not-applicable`: 0
- `Non-conformant`: 1 (BOM-less UTF-32 encoding-detection arms missing)
- `Indeterminate`: 0
- **Total: 10 reconciled requirements**

## Reconciled Requirement Table

Both auditors enumerated requirements independently and used different REQ-N numbering. The table below uses subject-matter identifiers so future readers can map between A's and B's numbering:

| Topic | A entry | B entry | Final verdict | Source |
|---|---|---|---|---|
| BOM at stream start accepted as encoding signal | REQ-§5.2-1 | REQ-§5.2-1 | Strict-conformant | Both agree |
| Five required encodings (UTF-8, UTF-16-LE/BE, UTF-32-LE/BE) supported | REQ-§5.2-2 | REQ-§5.2-2 | Strict-conformant | Both agree |
| Encoding is presentation; same content across encodings yields same parse | REQ-§5.2-3 | REQ-§5.2-3 | Strict-conformant | Both agree |
| BOM accepted at document prefix; rejected mid-document and post-`---` | REQ-§5.2-4 | REQ-§5.2-4 | Strict-conformant | Both agree |
| BOM-less encoding detection (per §5.2 detection table) | REQ-§5.2-5 | REQ-§5.2-5 (partial) | **Non-conformant** | A only — B missed |
| Truncated and invalid byte sequences rejected with typed errors | REQ-§5.2-6 | REQ-§5.2-6 | Strict-conformant | Both agree |
| BOM allowed inside quoted scalars (JSON compatibility) | REQ-§5.2-7 | REQ-§5.2-7 | Strict-conformant | Both agree |
| Multi-document streams with per-document BOM via loader API | REQ-§5.2-8 | REQ-§5.2-8 | Strict-conformant | Both agree |
| Double BOM at stream start silently accepted | (not tested) | REQ-§5.2-9 | **Lenient** | B only — A missed |
| Empty and BOM-only streams handled as zero-document streams | (not tested) | REQ-§5.2-10 | Strict-conformant | B only |

## Defect 1: BOM-less UTF-32 encoding detection missing

**Source:** Auditor A, REQ-§5.2-5. Auditor B's REQ-§5.2-5 covered the same spec sentence ("Otherwise, the stream must begin with an ASCII character") but B's test set tested only ASCII first byte and the UTF-16 null-byte heuristic, missing the UTF-32 BOM-less cases.

**Spec position.** YAML 1.2.2 §5.2 (lines 1542–1553 of the local spec, or the equivalent table in the canonical spec) defines a 9-row encoding-detection table. The relevant rows are normative — implementations must classify input matching each row as the indicated encoding:

- BOM rows (UTF-32 BE/LE, UTF-16 BE/LE, UTF-8): all five required.
- BOM-less rows: UTF-32-BE (`x00 x00 x00 any`), UTF-32-LE (`any x00 x00 x00`), UTF-16-BE (`x00 any`), UTF-16-LE (`any x00`), UTF-8 default (any other).

**Implementation gap.** `rlsp-yaml-parser/src/encoding.rs:55-72` (`detect_encoding`) implements 7 of the 9 rows: all 5 BOM rows plus UTF-16-BE and UTF-16-LE BOM-less heuristics. The two BOM-less UTF-32 rows are absent.

**Behavioral evidence (A's probe).**

- Input `[0x00, 0x00, 0x00, 0x6B, 0x00, 0x00, 0x00, 0x3A, ...]` (BOM-less UTF-32-BE encoding of `"k: 1\n"`): `detect_encoding` returned `Utf8`; `decode` produced `"\0\0\0k\0\0\0:\0\0\0 \0\0\01\0\0\0\n"` — bytes interpreted as UTF-8 with embedded NULs. Parse downstream fails on NUL bytes.
- Input `[0x6B, 0x00, 0x00, 0x00, 0x3A, 0x00, 0x00, 0x00, ...]` (BOM-less UTF-32-LE): `detect_encoding` returned `Utf16Le` (the `[a, 0x00, ..]` two-byte arm matches before any UTF-32 arm could); `decode` produced `"k\0:\0 \01\0\n\0"` — corrupt string interpreting alternating bytes as UTF-16 code-unit pairs.

**Lead verdict:** Non-conformant. The spec's detection table is normative; missing two of nine rows means BOM-less UTF-32 input is misclassified, and the user's content is unrecoverable through this entry point.

**Fix sketch (from A):** insert two arms before the existing UTF-16 heuristic arms in `encoding.rs:55-72`:

```rust
[0x00, 0x00, 0x00, a, ..] if *a != 0 => Encoding::Utf32Be,
[a, 0x00, 0x00, 0x00, ..] if *a != 0 => Encoding::Utf32Le,
```

The arms must precede the UTF-16 heuristic arms because `[a, 0x00, ..]` is a strict prefix of `[a, 0x00, 0x00, 0x00, ..]`.

**Real-world impact.** BOM-less UTF-32 input is rare in practice (most UTF-32 producers emit a BOM). However, the spec explicitly lists the BOM-less UTF-32 rows; absence is a normative gap regardless of frequency. A user submitting BOM-less UTF-32 — perhaps generated by a tool that signals encoding via filename or HTTP header rather than a BOM — receives a misleading decode error or corrupted parse.

## Defect 2: Double BOM at stream start silently accepted

**Source:** Auditor B, REQ-§5.2-9. Auditor A's REQ-§5.2-4 sub-case 4 tested double-BOM at INTER-DOCUMENT prefix (after `...`) and correctly observed parse-error; A did not test the stream-start position.

**Spec position.** YAML 1.2.2 production [202] `l-document-prefix ::= c-byte-order-mark? l-comment*` — at most one BOM at any document prefix. A second consecutive U+FEFF in document-prefix position is invalid: production [202] permits zero or one, and U+FEFF in any non-prefix position is rejected by the body check.

**Implementation gap.** Two BOM-stripping code paths both run for the first document:

1. `rlsp-yaml-parser/src/lines.rs:115-127` — `scan_line` strips a leading BOM when `is_first=true` (stream-start case).
2. `rlsp-yaml-parser/src/lines.rs:282-305` — `signal_document_boundary` strips a leading BOM at every document-prefix position, including the BetweenDocs → InDocument transition for the FIRST document.

For inter-document transitions (post-`...`), only path 2 runs, so a second consecutive BOM is preserved and then rejected by the body check at `event_iter/step.rs:64-82`. The asymmetry is observable: stream-start strips up to two BOMs silently; inter-doc strips one and rejects a second.

**Behavioral evidence (B's probe).**

- `&str` form `"\u{FEFF}\u{FEFF}key: v\n"`: `has_err = false`; events `StreamStart, DocumentStart{explicit=false}, MappingStart at byte 6, Scalar("key") at 6..9, Scalar("v") at 11..12, MappingEnd, DocumentEnd, StreamEnd`. Both BOMs (6 bytes total) silently removed.
- Contrast inter-doc form `"key: a\n...\n\u{FEFF}\u{FEFF}key: b\n"`: `has_err = true` — correctly rejected.

**Lead verdict:** Lenient. The implementation accepts at stream start what the spec rejects at every other document-prefix position; the inconsistency is a stream-start-specific gap, not an alternative spec interpretation.

**Doc errata.** The conformance doc cites the existing test `parse_events_rejects_double_bom_at_document_prefix` (`tests/encoding.rs:317-325`) at `yaml-spec-conformance.md:149-151` as evidence that double BOMs are rejected. The cited test only exercises the inter-doc form; the stream-start case is untested and the citation overstates uniformity. This is a citation-level erratum that propagates to the final summary's doc-errata section.

**Fix sketch (from B):** either (a) gate the second strip on "no BOM was already stripped at this prefix" — keep a per-prefix `bom_already_stripped` flag — or (b) remove one of the two strip sites for the first-document case, retaining only one.

## Methodology Notes

- The dual-track methodology surfaced two real defects that single-auditor coverage would have missed. A's BOM-less UTF-32 finding required pricer-level testing of the detection table; B's double-BOM-at-stream-start finding required a position-discrimination test A did not run. The defects are independent — they live in different parts of the encoding pipeline (`detect_encoding` for one; the `lines.rs` BOM-strip pair for the other).
- This reinforces the Phase 2 dispatch principle that A and B should not coordinate test design — the auditors' independent test choices catch different gaps. Future Phase 2 dispatches preserve this principle.
- Auditor B left a probe file in the parser tree (`tests/_audit_b_5_2_probe.rs`) at the time A finished — this was visible to A and to rust-analyzer's diagnostic stream. B subsequently deleted the file before its own completion. The "no committed test programs" constraint in the dispatch prompt held: `git status --porcelain` at lead-reconciliation time shows zero new files in `rlsp-yaml-parser/`. For future Phase 2 dispatches, both auditors should be reminded to delete probes immediately after observing output, not as a final-step cleanup, to avoid polluting the working tree during the parallel run.

## Disposition for Phase 2 Summary (Task 8)

Two follow-up entries to file in `.ai/memory/project_followup_plans.md`:

1. **BOM-less UTF-32 encoding-detection arms missing (§5.2)** — `encoding.rs:55-72` `detect_encoding`; gap is two missing match arms; fix sketch in this reconciliation. Verdict `Non-conformant`.
2. **Double BOM at stream start silently accepted (§5.2)** — two BOM-strip paths overlap for the first document at `lines.rs:115-127` + `lines.rs:282-305`; fix is to gate or deduplicate. Verdict `Lenient`.

Both at §5.2; different code locations. Neither deduplicates against any existing entry in `project_followup_plans.md`.

One doc erratum for the doc-rewrite plan (post-Phase-2 step 5):

- `yaml-spec-conformance.md:149-151` cites `parse_events_rejects_double_bom_at_document_prefix` as evidence that double-BOMs are rejected uniformly; the test only exercises the inter-doc case. Citation needs scoping to "inter-document" or supplementing with a stream-start case.
