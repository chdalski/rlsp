---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: Reconciliation
section: error-and-limits
date: 2026-04-30
produced-by: lead
---

# Reconciliation: Error Semantics and Resource Limits

A enumerated 15 requirements (13 SC + 1 Lenient + 1 split SC/Indeterminate); B enumerated 22 (16 SC + 6 Lenient). Each auditor surfaced findings the other did not test:

- **A's unique Lenient finding (high-impact, security-relevant):** the documented 1 MiB quoted-scalar length cap at `lexer/quoted.rs:606-611, 641-646, 709-714` is only enforced on the **owned path** (after an escape decode). A 100 MiB double-quoted scalar with no `\` escapes bypasses the cap entirely — the cap-check is gated on `if let Some(buf) = owned.as_mut()`. Raw scalars take the borrow path, which has no length check. **DoS-relevant defect.**
- **B's unique Lenient cluster (6 cases on error-position usability):** errors are correctly produced and structured (no panics), but reported positions don't point to the offending byte for several error classes — `%YAML` errors report the `%` position, unterminated single-quoted scalar reports EOF, resolved-tag overflow reports the `---` line, and `LoadError::UndefinedAlias` / `CircularAlias` / `AnchorCountLimitExceeded` / `AliasExpansionLimitExceeded` / `NestingDepthLimitExceeded` carry no `pos` field at all.

Verdict-taxonomy interpretation differs between auditors on the position-precision cases. A treats spec silence on positions as Indeterminate (no requirement to verdict against). B treats the Phase 2 task framing's "positions point to offending byte" expectation as a requirement → Lenient. **Lead sides with B's framing**: the Phase 2 task description explicitly listed error-position accuracy as a required behavior; treating it as a no-requirement Indeterminate would lose the actionable signal. The 6 Lenient cases stay Lenient with rationale that they are usability defects rather than spec-conformance defects.

Plus B's behavioral refinement: **`%YAML 1.0` is accepted** — only major=0 is rejected, not minor=0. Refines Phase 1 [86]'s "major-0 rejection" finding. Recorded for the conformance doc; not a separate verdict.

## Final Verdict Tally

- `Strict-conformant`: 16
- `Stricter-than-spec`: 0
- `Lenient`: 7 (6 on error-position usability + 1 on 1 MiB cap bypass)
- `Not-applicable`: 0
- `Non-conformant`: 0
- `Indeterminate`: 0
- **Total: 23 reconciled requirements**

## Resolved Disagreement

### Error-position imprecision: A's Indeterminate vs B's Lenient (6 entries)

**A's verdict:** split — Strict-conformant on the cases where positions ARE precise (implicit-key 1024 limit; directive count limit), Indeterminate on the cases where positions are imprecise (anchor/tag/comment/handle/resolved-tag/1 MiB overflow positions). Reasoning: the spec is silent on error-position precision, so the implementation's choice to point to start-of-construct rather than offending-byte is not a spec violation.

**B's verdict:** Lenient on 6 distinct cases. Reasoning: the Phase 2 task description listed error-position accuracy as a required behavior; the implementation falls short of that requirement on 6 cases.

**Lead's investigation:** The Phase 2 plan's Task 7 description explicitly states: "Error position accuracy: reported error positions point to the actual offending byte for each error class." This is a Phase 2 audit requirement, not a spec requirement. Treating spec silence as overriding the audit requirement (A's framing) would lose the actionable signal that 6 error-position usability defects exist. B's framing preserves the signal.

The "Lenient" verdict is a stretch of the formal taxonomy (Lenient typically means "accepts what spec rejects"; here the implementation produces an error correctly but with imprecise position). Pragmatically, "Lenient on usability" captures the intent. The methodology note below documents this taxonomic stretch.

**Lead's verdict:** Lenient on 6 entries (siding with B), with explicit rationale that this is a usability defect rather than a spec-conformance defect. Items contribute to the final tally as Lenient.

## Reconciled Requirement Table

The 23 unified requirements with final verdicts:

| # | Topic | Final verdict | A | B |
|---|---|---|---|---|
| 1 | Errors are produced for malformed input | Strict-conformant | REQ-1 | REQ-1 |
| 2 | Errors are structured (`Error` / `LoadError`, not panic) | Strict-conformant | REQ-5 | REQ-2 |
| 3 | No panics in production code paths (27+ adversarial probes) | Strict-conformant | REQ-5 (combined) | REQ-3 |
| 4 | Error recovery: stop-at-first behavior documented and observed | Strict-conformant | REQ-2 | REQ-4 |
| 5 | Error position present (line/column/byte_offset structure) | Strict-conformant | REQ-3 (combined) | REQ-5 |
| 6 | Error position points to offending byte — implicit-key 1024 limit | Strict-conformant | REQ-3 SC half | REQ-6 |
| 7 | Error position points to offending byte — directive count limit | Strict-conformant | REQ-3 SC half | REQ-7 |
| 8 | Error position — Phase 1 [59]/[60]/[61] numeric escape rejections | Strict-conformant | REQ-3 SC half | REQ-8 |
| 9 | Error position — `%YAML` major-0 rejection (Phase 1 [86]) | **Lenient** | REQ-3 Indeterminate | REQ-9 Lenient |
| 10 | Error position — u8 digit-overflow (Phase 1 [87]) | **Lenient** | REQ-3 Indeterminate | REQ-10 Lenient |
| 11 | Error position — unterminated single-quoted scalar | **Lenient** | (not split) | REQ-11 Lenient |
| 12 | Error position — resolved-tag overflow | **Lenient** | (not split) | REQ-12 Lenient |
| 13 | Error position — `MAX_ANCHOR_NAME_BYTES` overflow | **Lenient** | (not split) | REQ-13 Lenient |
| 14 | Error position — `LoadError` variants carry no pos field | **Lenient** | (not split) | REQ-14 Lenient |
| 15 | `MAX_COLLECTION_DEPTH=512` — limit enforced | Strict-conformant | (covered) | REQ-15 |
| 16 | `MAX_ANCHOR_NAME_BYTES=1024` — limit enforced (multi-byte verified) | Strict-conformant | REQ-7 | REQ-16 |
| 17 | `MAX_TAG_LEN=4096` — limit enforced | Strict-conformant | REQ-8 | REQ-17 |
| 18 | `MAX_COMMENT_LEN=4096` — limit enforced | Strict-conformant | REQ-9 | REQ-18 |
| 19 | `MAX_DIRECTIVES_PER_DOC=64` — limit enforced (Phase 2 §6.8 ST) | Strict-conformant on enforcement | REQ-10 | REQ-19 |
| 20 | `MAX_TAG_HANDLE_BYTES=256` — limit enforced | Strict-conformant | REQ-11 | REQ-20 |
| 21 | `MAX_RESOLVED_TAG_LEN=4096` — limit enforced | Strict-conformant | REQ-12 | REQ-21 |
| 22 | Loader limits (`max_nesting_depth`, `max_anchors`, `max_expanded_nodes`) enforced | Strict-conformant | REQ-13 | REQ-22 |
| 23 | **1 MiB quoted-scalar cap bypassed on no-escape borrow path** | **Lenient** | REQ-15 Lenient | (not tested) |

## Defect Detail

### Defect 1: Error-position imprecision (items 9-14, 6 cases) — Lenient

**Implementation gap:** errors are correctly produced and structured (no panics, typed `Error`/`LoadError`), but the `pos` field on the error doesn't point to the offending byte for several error classes:

| Error class | Reported position | Should point to |
|---|---|---|
| `%YAML` major-0 rejection | `%` (column 0) | The major digit |
| `%YAML` u8 digit overflow | `%` (column 0) | The first digit beyond the limit |
| Unterminated single-quoted scalar | EOF | The opening `'` |
| Resolved-tag overflow | `---` line position | The offending `!handle!` token |
| `MAX_ANCHOR_NAME_BYTES` overflow | `&` start-of-anchor | The first byte beyond the limit |
| `LoadError::UndefinedAlias` / `CircularAlias` / `AnchorCountLimitExceeded` / `AliasExpansionLimitExceeded` / `NestingDepthLimitExceeded` | (no `pos` field) | The offending node |

**Fix sketch:** for each error class, capture the byte position at the precise overflow/error point and pass it through to the `Error` / `LoadError` construction. The implicit-key 1024 limit (item 6) demonstrates a feasible precise-byte design that the other paths could mirror.

**Rationale for keeping in scope:** The Phase 2 task description explicitly listed error-position accuracy as a required behavior. The spec is silent on positions, but consumers (LSP servers, IDEs) depend on precise positions for diagnostics. The conformance doc rewrite should document the position-precision contract explicitly: which error classes point to offending byte, which point to start-of-construct, and which carry no position. The user may decide post-Phase-2 whether to invest in fixing the imprecise classes.

### Defect 2 (item 23): 1 MiB quoted-scalar cap bypass on no-escape path — Lenient (security-relevant)

**Implementation gap:** the documented 1 MiB scalar length cap at `lexer/quoted.rs:606-611, 641-646, 709-714` is gated on `if let Some(buf) = owned.as_mut()`. The owned path is taken only after a `\` escape triggers the decode-and-buffer routine. A double-quoted scalar with no escapes — no `\` characters anywhere — takes the BORROW path, which has no length check.

**Behavioral evidence (A's REQ-15):** a 100 MiB double-quoted scalar of plain ASCII content (no escapes) parses without error. The error message "scalar exceeds maximum allowed length" suggests universal coverage; the implementation's conditional gating on `owned.as_mut()` contradicts the documented limit.

**DoS implication:** an adversarial input can bypass the documented memory limit by avoiding escape characters. A 100 MiB scalar consumes 100 MiB of memory in the LSP server process (the scalar is borrowed from the source `&str`, but the source `&str` itself must be that large to begin with — so the attack requires an attacker who can submit a 100+ MiB document). For LSP/library consumers that accept bounded input, the cap is effective; for consumers that don't bound the input, the cap is the documented backstop, and it doesn't fire.

**Fix sketch:** add the 1 MiB length check to the borrow path as well — track the length scanned in the borrow body and bail out at `>1 MiB` regardless of whether `owned` has been allocated. Or remove the cap from the owned path and replace with a single check at the lexer-completion site that runs unconditionally.

**Conformance doc note:** the doc should accurately describe which scalars the 1 MiB cap covers (currently: only those containing escape sequences) and decide whether to fix the gap or formally accept it with rationale.

## Architectural Findings

### Position-precision design contract (Defect 1 elaboration)

The parser has a clear precision asymmetry: most error positions point to the start-of-construct (where parsing the construct began) rather than the offending-byte (where the violation actually occurred). The implicit-key 1024 limit is the exception — it correctly captures the offending-byte. This is a reasonable default (start-of-construct is cheaper to record than tracking offending-byte through the parse) but produces poor diagnostics for limit-violation cases where the user wants to know which byte tipped them over.

**Disposition:** the user decides post-Phase-2 whether to invest in offending-byte precision globally. Document the current contract in the conformance doc with a per-error-class table.

### `%YAML 1.0` behavioral refinement

B's REQ-9 testing surfaced that `%YAML 1.0` is accepted (only major=0 is rejected, not minor=0). This refines Phase 1's `[86]` "major-0 rejection" finding — the rejection is on the major component only; minor component bounds (255 max via `parse::<u8>`) catch the digit-overflow case but not minor=0. Document the refined behavior in the conformance doc.

### No-panic property

Both auditors confirmed via 25+ adversarial probes (unterminated quotes, raw control bytes, deep nesting, lone indicators, malformed directives) that all error paths produce structured errors. Production `unreachable!` calls are caller-side invariant guards (preconditions on private functions), not user-reachable. This is a real implementation strength worth documenting in the conformance doc.

## Disposition for Phase 2 Summary (Task 8)

**New follow-up entries to file:**

1. **1 MiB quoted-scalar cap bypassed on no-escape borrow path (item 23)** — `lexer/quoted.rs:606-611, 641-646, 709-714` cap-check is gated on owned.as_mut(); raw scalars without escapes bypass the cap. DoS-relevant. Verdict `Lenient`. Fix: add unconditional length check on the borrow path.

2. **Error-position imprecision (items 9-14, 6 sub-cases)** — six error classes report start-of-construct positions or carry no position field. May be filed as one consolidated entry ("error-position usability defects across 6 error classes") or split into per-class entries. Recommend consolidated entry with internal sub-list, since the fix shape (capture offending-byte at construction site) is uniform across all 6.

**Items deduplicated against existing follow-ups:**

- `MAX_DIRECTIVES_PER_DOC=64` (item 19 enforcement) — Phase 2 §6.8 reconciliation already filed `MAX_DIRECTIVES_PER_DOC` as Stricter-than-spec. The enforcement itself is SC; the Stricter-than-spec verdict on the limit value is in §6.8.

**Items not generating follow-ups:**

- All 16 SC entries.

**Doc errata observations:**

- The existing conformance doc does not describe the position-precision contract; the doc rewrite should add a per-error-class table.
- The `%YAML 1.0`-accepted behavioral refinement (only major=0 rejected) should be documented.
- The 1 MiB cap's actual coverage (escapes-only) should be documented if the user chooses to formally accept the gap rather than fix it.

## Methodology Notes

- This task had the largest cross-coverage gap of Phase 2 (A enumerated 15, B enumerated 22, with significant non-overlap). Each auditor's distinct testing revealed distinct findings: A's borrow-path bypass would have been missed by B; B's per-error-class position survey would have been collapsed into Indeterminate by A. Both surfaces are valuable.
- The verdict-taxonomy stretch on error-position imprecision (Lenient as "fails implementation contract" rather than the usual "accepts what spec rejects") is documented above. Future Phase 2-style audits may benefit from a separate verdict label like "Quality-defect" or "Implementation-contract-shortfall" — but introducing taxonomy mid-audit creates inconsistency, so the stretched-Lenient is preserved here.
- Probe cleanup held: both auditors used `/tmp/audit-probe-error-and-limits-{a,b}/` outside the git tree; final `git status` clean.
- This is the last per-area Phase 2 reconciliation. All 7 area reconciliations are complete. Task 8 (Phase 2 summary + follow-up filings) is next.
