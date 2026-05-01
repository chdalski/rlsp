---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: Summary
section: all
date: 2026-04-30
produced-by: lead
---

# Phase 2 Conformance Audit Summary

130 normative-prose requirements audited across 7 areas (encoding §5.2, directives §6.8, tag resolution §6.9.1, schemas §10.1 / §10.2 / §10.3, error semantics + limits) using the same dual-track methodology as Phase 1 (independent A and B subagents per area, lead reconciliation). Behavioral methodology — auditors constructed inputs, ran them through the parser, and compared observed output to spec expectation. All 7 reconciliation files plus this summary live in `.ai/audit/2026-04-30-phase2-prose/`.

## Final Verdict Tally

| Verdict | Count | Percentage |
|---|---|---|
| `Strict-conformant` | 107 | 82.3% |
| `Stricter-than-spec` | 4 | 3.1% |
| `Lenient` | 17 | 13.1% |
| `Not-applicable` | 0 | 0.0% |
| `Non-conformant` | 1 | 0.8% |
| `Indeterminate` | 1 | 0.8% |
| **Total** | **130** | **100%** |

Phase 2's higher Lenient rate (13.1%) compared to Phase 1's (5.2%) reflects that normative-prose requirements have more behavioral nuance than BNF productions — many of Phase 2's Lenient findings are about how the parser handles edge cases, not about whether it matches the grammar.

## Per-Area Tally

| Area | SC | ST | Len | NC | Indet | Total |
|---|---|---|---|---|---|---|
| §5.2 Character Encodings | 8 | 0 | 1 | 1 | 0 | 10 |
| §6.8 Directives | 23 | 3 | 3 | 0 | 0 | 29 |
| §6.9.1 Tag Resolution | 23 | 0 | 4 | 0 | 1 | 28 |
| §10.1 Failsafe Schema | 8 | 0 | 0 | 0 | 0 | 8 |
| §10.2 JSON Schema | 13 | 0 | 0 | 0 | 0 | 13 |
| §10.3 Core Schema | 16 | 1 | 2 | 0 | 0 | 19 |
| Error semantics + limits | 16 | 0 | 7 | 0 | 0 | 23 |
| **Total** | **107** | **4** | **17** | **1** | **1** | **130** |

§10.1 and §10.2 are 100% Strict-conformant. §6.8 has the highest Stricter-than-spec count (3), all defensive-conservatism choices. The error-and-limits area has the highest Lenient count (7), driven by the 6-case error-position imprecision cluster.

## Non-conformant Production (1 entry)

This is a **real defect** — input the spec considers valid is misclassified by the parser:

### NC1. BOM-less UTF-32 encoding detection arms missing (§5.2)

**Code location:** `rlsp-yaml-parser/src/encoding.rs:55-72` (`detect_encoding`).

**Spec requirement (§5.2):** the encoding-detection table is normative; implementations must classify input matching each row as the indicated encoding. The 9-row table includes BOM-less UTF-32-BE (`x00 x00 x00 any`) and BOM-less UTF-32-LE (`any x00 x00 x00`) rows.

**Implementation gap:** the `detect_encoding` match table implements 7 of 9 rows. The two BOM-less UTF-32 arms are missing. Behavioral evidence:

- BOM-less UTF-32-BE input `[0x00, 0x00, 0x00, 0x6B, ...]` (encoding of `"k: 1\n"`) is misclassified as UTF-8 and decoded with embedded NUL bytes.
- BOM-less UTF-32-LE input `[0x6B, 0x00, 0x00, 0x00, ...]` is misclassified as UTF-16-LE (the `[a, 0x00, ..]` two-byte heuristic matches before the UTF-32 arm could).

**Fix sketch:** insert two arms before the existing UTF-16 heuristic arms:

```rust
[0x00, 0x00, 0x00, a, ..] if *a != 0 => Encoding::Utf32Be,
[a, 0x00, 0x00, 0x00, ..] if *a != 0 => Encoding::Utf32Le,
```

Verdict `Non-conformant` (this is real wrong-output for spec-valid input). Filed as new follow-up entry (see Follow-up filing below).

## Lenient Productions (17 entries)

Each is a case where the parser accepts input the spec rejects. Findings cluster by code area:

### Encoding (§5.2)

- **L1. Double BOM at stream start silently accepted.** Two BOM-strip code paths run for the first document (`scan_line` is_first=true at `lines.rs:115-127` AND `signal_document_boundary` at `lines.rs:282-305`); inter-doc transitions correctly run only one path and reject second consecutive BOM. Stream-start asymmetry. Fix: gate the second strip or remove redundancy.

### Directives (§6.8)

- **L2. NUL bytes in directive name pass through.** `%FOO\x00 bad` parses without error. (Phase 1 [84] propagation; deduplicated against existing follow-up.)
- **L3. NUL bytes in directive parameter pass through.** `%FOO foo\x00bar` parses. (Phase 1 [85] propagation; dedup.)
- **L4. `%TAG ! ! # primary` comment-after-prefix absorbed.** Comment text is absorbed into the prefix because the prefix scanner doesn't honor `s-l-comments` after `ns-tag-prefix`.

### Tag resolution (§6.9.1)

- **L5. Verbatim tag admissibility unenforced.** `properties.rs:91-164` validates only `ns-uri-char+`; the spec's "must begin with `!` or be a valid URI" rule is unenforced. Spec Example 6.25 cases (`!<!>`, `!<$:?>`, `!<:foo>`) all parse. Loader bare-`!` shortcut at `loader.rs:1010-1013` further misclassifies `!<!>` as shorthand non-specific tag.
- **L6. Verbatim tag separator missing.** Verbatim arm at `properties.rs:91-164` advances past `>` without `s-separate` check; shorthand path correctly enforces. `!<URI>foo` parses with no whitespace separator. Asymmetric.
- **L7. Empty shorthand suffix accepted.** `!!`, declared `!handle!`, and `%TAG !` shorthand all parse. Phase 1 [99] propagation; deduplicated against existing follow-up.
- **L8. Post-concatenation tag URI validity unchecked.** Handle+suffix concatenation result is not re-validated. Same observable behavior as Phase 1 [93]/[94]/[95] but the §6.9.1 layer's rule is independent of the §6.8.2.2 prefix-scan layer.

### Core schema (§10.3)

- **L9. Signed octal int accepted.** `-0o10`, `+0o10` resolve to `!!int`; spec §10.3 octal regex `0o [0-7]+` is unsigned. Sign-strip at `schema.rs:289-293` is unconditional and applies before per-base validation.
- **L10. Signed hex int accepted.** `-0xFF`, `+0xFF` resolve to `!!int`; spec §10.3 hex regex `0x [0-9a-fA-F]+` is unsigned. Same root cause as L9.

### Error-and-limits

- **L11. 1 MiB quoted-scalar cap bypassed on no-escape borrow path.** `lexer/quoted.rs:606-611, 641-646, 709-714` cap-check is gated on `if let Some(buf) = owned.as_mut()`. Raw double-quoted scalars without escapes take the borrow path which has no length check. **DoS-relevant.**
- **L12-L17. Error-position imprecision (6 cases).** Errors are correctly produced and structured (no panics, typed `Error`/`LoadError`), but the `pos` field doesn't point to the offending byte for: `%YAML` major-0 rejection; `%YAML` u8 digit overflow; unterminated single-quoted scalar; resolved-tag overflow; `MAX_ANCHOR_NAME_BYTES` overflow; five `LoadError` variants without `pos` field.

## Stricter-than-spec Productions (4 entries)

Each is a case where the parser rejects input the spec admits. All are intentional defensive-conservatism or pragmatic implementation choices — preserved as-is unless the user explicitly chooses to relax.

- **S1. `%YAML` major-0 rejection (§6.8).** Spec only mandates rejection of higher major versions; `%YAML 0.5` rejection is conservative (no defined YAML 0.x exists). Phase 1 [86] propagation; behaviorally refined: only major=0 is rejected, `%YAML 1.0` is accepted.
- **S2. `%YAML` u8 digit overflow (§6.8).** `parse::<u8>` bounds digits to [0, 255]; `%YAML 1.300` rejected. Spec regex permits arbitrary digit counts. Phase 1 [87] propagation.
- **S3. `MAX_DIRECTIVES_PER_DOC=64` hardcoded limit (§6.8).** Spec has no per-document directive count limit; the 65th directive is hard-rejected. Defensive but undocumented as a configurable choice. NEW finding from Phase 2 §6.8.
- **S4. Core leading-zero decimal rejection (§10.3).** `007`, `01`, `0123` rejected as `!!str`; spec regex `[-+]? [0-9]+` permits leading zeros. Defensive (prevents user-error confusion with octal). NEW finding from Phase 2 §10.3.

## Indeterminate Production (1 entry)

- **I1. Unresolved tags partial representation (§6.9.1, REQ-23 cross-§10).** B's audit deferred to §10 schema audit. After §10.1/§10.2/§10.3 audits, the schemas don't define partial-representation behavior — they all resolve unresolved nodes to specific tags (`!str` under permissive Core, error under strict JSON, `!str` under Failsafe). Spec's "may compose a partial representation" is non-mandatory; the implementation chooses to fully resolve, which is permissible. Re-verdicted post-§10 audits as **`Strict-conformant`**. (One Indeterminate stays in the tally because the re-verdict is post-summary; the user may adjudicate.)

## Doc Errata (informational — for the doc-rewrite plan)

Phase 2 surfaced several places where the conformance doc or the spec itself has errata:

### Conformance doc errata

- **§5.2 BOM rejection citation overstates uniformity.** `yaml-spec-conformance.md:149-151` cites `parse_events_rejects_double_bom_at_document_prefix` as evidence of uniform double-BOM rejection. The cited test only exercises the inter-doc case; the stream-start case (defect L1) is masked. Citation needs scoping or supplementing.
- **§6.8 directive name/parameter validation labels.** Doc marks [84]/[85]/[93]/[94]/[95]/[99]/[88] as "Conformant"; behavioral evidence contradicts (Lenient on [84]/[85]/[93]/[94]/[95]/[99]; the [88] inconsistency was noted in audit B).
- **§10.3 integer rows.** Doc marks the integer rows as "Conformant" without documenting the leading-zero rejection (S4) or the signed-octal/hex laxity (L9/L10).
- **Position-precision contract not documented.** The conformance doc does not describe which error classes report precise positions vs start-of-construct. Add a per-error-class table.
- **`%YAML 1.0` accepted refinement.** Phase 1 [86] said "major-0 rejected"; Phase 2 found this means `%YAML 0.x` is rejected but `%YAML 1.0` is accepted (only major=0 is gated, not minor=0). Document the refined behavior.
- **1 MiB quoted-scalar cap actual coverage.** Doc should describe that the cap covers only escape-bearing scalars (current behavior, defect L11) — or remove the cap entirely from the borrow path so coverage is universal.

### Spec errata observed

- **§10.2 `-0` worked example contradicts int regex.** Spec line 6601 shows `-0` resolving to integer 0; spec line 6578 (int regex `0 | -? [1-9] [0-9]*`) excludes `-0` from int. The parser correctly follows the regex (`-0` → `!!float`). Document the parser's literal-regex implementation choice and note the spec's internal inconsistency.

## Architectural Findings

These are cross-cutting design observations. Not per-requirement defects; recorded for the doc-rewrite plan and for the post-Phase-2 design-decisions batch.

- **A1. No Warning event variant or collector** (surfaced in §6.8). The parser's event stream is success-or-error; there is no `Event::Warning` variant. Spec uses "should ... with appropriate warning" three times in §6.8 alone (1.1 acceptance, 1.3 acceptance, reserved-directive ignore); all three are reconciled `Strict-conformant` per "should is non-mandatory," but the architectural absence prevents the parser from honoring the spec's "appropriate warning" suggestion at any site. Future-design candidate.
- **A2. Loader's bare-`!` shortcut conflates verbatim and shorthand sources** (surfaced in §6.9.1). `loader.rs:1010-1013` treats any tag value of `!` as non-specific regardless of whether it arrived from verbatim `!<!>` or shorthand `!`. The proper fix is at the verbatim admissibility check (defect L5), not the loader.
- **A3. Verbatim/shorthand asymmetry on s-separate** (surfaced in §6.9.1). The verbatim arm doesn't enforce `s-separate` between `>` and content; shorthand arm correctly does. Defect L6.
- **A4. Position-precision design contract is implicit** (surfaced in error-and-limits). Most error positions point to start-of-construct rather than offending-byte. The implicit-key 1024 limit demonstrates a feasible precise-byte design that the other paths do not match. Defect cluster L12-L17 is the consequence; the doc-rewrite should make the contract explicit.
- **A5. Foreign tags pass through under any schema** (surfaced in §10.1, §10.3). Explicit non-schema tags (`!!int` under Failsafe, `!foo` anywhere) preserve as-is. Spec-consistent — schemas govern resolution, not source rejection. Document the design choice.

## Follow-up Filing

Per the dedup rule (spec section AND code location, not topic), the following NEW entries are appended to `.ai/memory/project_followup_plans.md`:

### New entries

1. **BOM-less UTF-32 encoding detection arms missing (§5.2)** — Non-conformant. `encoding.rs:55-72`. Fix: insert two missing match arms before UTF-16 heuristic. Defect NC1.
2. **Double BOM at stream start silently accepted (§5.2)** — Lenient. `lines.rs:115-127` + `lines.rs:282-305`. Fix: gate or deduplicate the two BOM-strip paths. Defect L1.
3. **`%TAG` comment-after-prefix absorbed (§6.8.2)** — Lenient. Tag-directive prefix scanner. Fix: consume optional whitespace + `s-l-comments` after `ns-tag-prefix`. Defect L4.
4. **`MAX_DIRECTIVES_PER_DOC=64` hardcoded limit (§6.8)** — Stricter-than-spec. `event_iter/directives.rs:75-83`. Document the choice; consider making configurable via `LoaderOptions`. Defect S3.
5. **Verbatim tag admissibility unenforced (§6.9.1)** — Lenient. `properties.rs:91-164`. Fix: add prose-level admissibility check (must begin with `!` or be valid URI). Defect L5.
6. **Verbatim tag separator missing (§6.9.1)** — Lenient. `properties.rs:91-164`. Fix: enforce `s-separate(n,c)` between `>` and content. Defect L6.
7. **Post-concatenation tag URI validity (§6.9.1)** — Lenient. Resolution-time concatenation check. Could be deduplicated with Phase 1 [93]/[94]/[95] follow-up at user discretion (single fix may resolve both). Defect L8.
8. **Signed octal/hex int (§10.3) — combined entry covering L9 + L10** — Lenient. `schema.rs:289-293` unconditional sign-strip. Fix: gate sign strip to decimal-shaped bodies only. Defects L9, L10 share the same fix point.
9. **Core leading-zero decimal rejection (§10.3)** — Stricter-than-spec. `schema.rs:307-309`. Document choice (defensive against octal-confusion); user decides post-Phase-2 whether to relax. Defect S4.
10. **1 MiB quoted-scalar cap bypassed on no-escape path (error-and-limits)** — Lenient, DoS-relevant. `lexer/quoted.rs:606-611, 641-646, 709-714`. Fix: add unconditional length check on the borrow path. Defect L11.
11. **Error-position imprecision across 6 error classes (error-and-limits)** — Lenient cluster. Multiple sites; fix shape uniform (capture offending-byte at construction site). Filed as one consolidated entry covering L12-L17.

### Deduplicated against existing entries

12. **NUL bytes in directive name (§6.8, defect L2)** — deduplicates against Phase 1 [84] follow-up at commit `775020c` ("Directive name + parameter validation"). Same spec section + same code location.
13. **NUL bytes in directive parameter (§6.8, defect L3)** — deduplicates against Phase 1 [85] / [84] consolidated follow-up at `775020c`. Same spec section + same code location.
14. **Empty shorthand suffix (§6.9.1, defect L7)** — deduplicates against Phase 1 [99] follow-up at `775020c` ("Empty suffix in shorthand tags"). Same spec section + same code location.
15. **`%YAML` major-0 rejection (§6.8, S1)** — Stricter-than-spec; doesn't auto-file per the dedup rule. Already represented in Phase 1 [86].
16. **`%YAML` u8 digit overflow (§6.8, S2)** — Stricter-than-spec; doesn't auto-file. Already represented in Phase 1 [87].

### `Indeterminate` requirements (no auto-file)

- **I1. Unresolved tags partial representation (§6.9.1)** — re-verdicted post-§10 audits as Strict-conformant; user may adjudicate.

## Cumulative Phase 1 + Phase 2 Picture

Combining Phase 1's 213 entries and Phase 2's 130 entries: **343 conformance points audited**. Cumulative findings:

| Verdict | Phase 1 | Phase 2 | Total |
|---|---|---|---|
| Strict-conformant | 194 | 107 | 301 (87.8%) |
| Stricter-than-spec | 5 | 4 | 9 (2.6%) |
| Lenient | 11 | 17 | 28 (8.2%) |
| Not-applicable | 3 | 0 | 3 (0.9%) |
| Non-conformant | 0 | 1 | 1 (0.3%) |
| Indeterminate | 0 | 1 | 1 (0.3%) |

**Lenient findings cluster across the two phases:**

- **c-printable family** (Phase 1 [1]/[27]/[34]/[75], Phase 2 §6.9.1 verbatim laxity, error-position imprecision) — predicate-defined-but-not-enforced root cause; many fixes share infrastructure.
- **Tag-prefix family** (Phase 1 [93]/[94]/[95]/[99], Phase 2 §6.9.1 admissibility/separator/post-concat) — tag-handling laxity; 4-5 fixes share code area.
- **Directive validation family** (Phase 1 [84]/[85], Phase 2 §6.8 directive-name NUL pass-through, %TAG comment absorption) — directive-parsing laxity.
- **Encoding** (Phase 2 §5.2 NC1 + L1) — separate from above; 2 distinct fixes.
- **Resource limits** (Phase 2 error-and-limits L11) — 1 MiB cap bypass on no-escape path; standalone DoS-relevant fix.
- **Error-position imprecision** (Phase 2 error-and-limits L12-L17) — 6-case usability cluster; uniform fix shape.
- **Schema regex laxity** (Phase 2 §10.3 L9/L10) — sign-strip overreach; single code-point fix.

**Stricter-than-spec findings** (8 of 9 are pragmatic/defensive):

- 5 from Phase 1 numeric-escape security hardening + version checks
- 3 from Phase 2 directive count limit + leading-zero decimal + Core's leading-zero rejection

These are intentional choices the user may relax during the post-Phase-2 design-decisions batch.

## Disposition for Post-Phase-2 Workflow

The post-Phase-2 orchestration entry in `.ai/memory/project_followup_plans.md` (added at commit `a6037ee`) describes the 6-step workflow that the audit feeds into:

1. ✅ **Reconcile Phase 2 findings** — done by this summary.
2. **Bundle conformance design decisions** — for each Lenient and Stricter-than-spec finding (28 + 9 = 37 entries minus pre-existing dedups), the user chooses FIX vs FORMALLY ACCEPT.
3. **Stricter-than-spec rationale review** — Phase 2's S3 (`MAX_DIRECTIVES_PER_DOC`) and S4 (Core leading-zero) join Phase 1's [86]/[87] and [59]/[60]/[61] for review.
4. **Implementation plans for FIX decisions** — clusters share fix areas; consider one plan per cluster.
5. **Conformance doc rewrite plan** — embed all BNF-trace and behavioral analyses verbatim; document the verdict taxonomy; correct mislabeled entries; document architectural findings and Stricter-than-spec rationales; address spec errata observations.
6. **Conformance status declaration** — once the doc reflects audited reality.

## Methodology Notes

- The dual-track methodology continued to surface complementary findings each auditor missed alone. Phase 2's biggest examples: A's 1 MiB cap bypass (B didn't test); B's broader §6.8 enumeration (28 vs A's 20). Cross-coverage is the value of the design.
- The "should is non-mandatory" precedent (Phase 1 [83]) was applied uniformly across §6.8 / §10 — three "should warn" cases reconciled as Strict-conformant, with A's underlying architectural observation about the absent Warning channel preserved as A1.
- The verdict-taxonomy stretch on error-position imprecision (Lenient as "fails implementation contract" rather than "accepts what spec rejects") is preserved with rationale. Future audits may benefit from a separate verdict label, but introducing taxonomy mid-audit creates inconsistency.
- Probe cleanup discipline held across all 7 areas: the §5.2 lesson (B left a probe visible to A during parallel run, then deleted) propagated into all subsequent dispatch prompts as "delete probes IMMEDIATELY after observing output." Final `git status` is clean across all 7 commits.
- Phase 2 inter-auditor disagreement rates by area: §5.2 (low — distinct findings, not disagreements), §6.8 (moderate — "should warn" interpretation), §6.9.1 (low — A cross-attributed correctly), §10.1 (zero), §10.2 (one — `-0` interpretation), §10.3 (high — B mis-read spec table on signed octal/hex), error-and-limits (moderate — taxonomy stretch on position imprecision). Lead resolution discipline held throughout.
