---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: Reconciliation
section: §6.8
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §6.8 Directives

A enumerated 20 requirements (with REQ-20 marked `Indeterminate`); B enumerated 28 (with REQ-5 carrying a split verdict — SC on the "must accept" half, Lenient on the "should adjust" half — that is consolidated below). The audits cover overlapping ground but B explored more behavioral edges (case sensitivity, `MAX_DIRECTIVES_PER_DOC` limit, `%TAG` comment absorption, etc.) while A surfaced one Indeterminate that B did not address (indented `%` line position).

The two audits diverged systematically on **"should warn / should adjust" interpretation**:

- **A's reading:** spec uses "should ... with appropriate warning" three times in §6.8 (1.1 acceptance, 1.3 acceptance, unknown-directive ignore); the parser has no Warning event variant or collector, so silent ignore on these cases is `Lenient`.
- **B's reading:** "should" is non-mandatory per RFC 2119; silent ignore satisfies the spec literally; verdict `Strict-conformant`.

**Phase 1's `[83] ns-reserved-directive` precedent already settled this question** in B's favor: "should is non-mandatory; silent acceptance is permitted." For consistency, the lead reconciles all "should warn / should adjust" cases as `Strict-conformant` per the literal spec language. **A's underlying observation is preserved** as a separate Architectural Findings section below — the absence of a Warning channel is a real design constraint that affects multiple requirements, but it is not a per-requirement Lenient.

The lead also resolves A's `Indeterminate` (REQ-20: indented `%` line position) by reading the parser code: `is_directive_line` and `try_consume_directive_line` at `lexer.rs:150-174` both call `line.content.starts_with('%')`, so an indented `%FOO` is not recognized as a directive — it falls through to content-line handling. This matches §6.8 ("Each directive is specified on a separate non-empty line starting with the `%` character"). Verdict: `Strict-conformant`.

## Final Verdict Tally

- `Strict-conformant`: 23
- `Stricter-than-spec`: 3 (major-0 rejection; minor digit overflow; `MAX_DIRECTIVES_PER_DOC=64`)
- `Lenient`: 3 (NUL in directive name; NUL in directive parameter; `%TAG` comment-after-prefix absorption)
- `Not-applicable`: 0
- `Non-conformant`: 0
- `Indeterminate`: 0
- **Total: 29 reconciled requirements** (28 from B + 1 unique from A's REQ-20 — resolved by lead code-reading)

## Reconciled Requirement Table

Both auditors numbered independently. The table uses B's broader enumeration as canonical and adds A's unique REQ-20. "Source" columns indicate which audit primarily contributed each requirement.

| # | Topic | Final verdict | A entry | B entry | Notes |
|---|---|---|---|---|---|
| 1 | `%YAML 1.2` accepted | Strict-conformant | REQ-1 SC | REQ-1 SC | Both agree |
| 2 | No `%YAML` directive accepted | Strict-conformant | REQ-2 SC | REQ-2 SC | Both agree |
| 3 | Higher major (`%YAML 2.0`) rejected | Strict-conformant | REQ-5 SC | REQ-3 SC | Both agree |
| 4 | Higher minor (`%YAML 1.3`) processed (with warning per spec) | Strict-conformant | REQ-4 Lenient | REQ-4 SC | Lead sides with B; "should is non-mandatory" |
| 5 | Lower minor (`%YAML 1.1`) processed (with adjustment per spec) | Strict-conformant | REQ-3 Lenient | REQ-5 SC (must-half) + Lenient (should-half) | Lead sides with B's must-half SC and consolidates the should-adjust half as SC per "should is non-mandatory" |
| 6 | Major version 0 rejected | Stricter-than-spec | REQ-6 ST | REQ-6 ST | Both agree; rationale: defensive (no defined YAML 0.x) |
| 7 | Minor digit overflow (256+) rejected | Stricter-than-spec | REQ-7 ST | REQ-7 ST | Both agree; rationale: `parse::<u8>` cap (Phase 1 [87] propagation) |
| 8 | Duplicate `%YAML` directive rejected | Strict-conformant | REQ-8 SC | REQ-8 SC | Both agree |
| 9 | Per-document `%YAML` scope (no carry across `---`/`...`) | Strict-conformant | (not explicitly tested) | REQ-9 SC | B unique |
| 10 | `%TAG` primary handle (`!`) | Strict-conformant | REQ-9 SC | REQ-10 SC | Both agree |
| 11 | `%TAG` secondary handle (`!!`) defaults and overrides | Strict-conformant | REQ-10 SC | REQ-11 SC | Both agree |
| 12 | `%TAG` named handle (`!handle!`) requires declaration | Strict-conformant | REQ-11 SC | REQ-12 SC | Both agree |
| 13 | `%TAG` per-document scope | Strict-conformant | REQ-13 SC | REQ-13 SC | Both agree |
| 14 | Duplicate `%TAG` handle rejected | Strict-conformant | REQ-12 SC | REQ-14 SC | Both agree |
| 15 | Reserved/unknown directive ignored (with warning per spec) | Strict-conformant | REQ-14 Lenient | REQ-15 SC | Lead sides with B; "should is non-mandatory" |
| 16 | `MAX_DIRECTIVES_PER_DOC=64` limit | Stricter-than-spec | (not tested) | REQ-16 ST | B unique; spec has no per-document directive count limit |
| 17 | Lowercase directive names treated as reserved (case-sensitive `YAML`/`TAG`) | Strict-conformant | (not tested) | REQ-17 SC | B unique |
| 18 | NUL bytes in directive name pass through | Lenient | REQ-15 Lenient | REQ-18 Lenient | Both agree; Phase 1 [84] propagation |
| 19 | NUL bytes in directive parameter pass through | Lenient | (subsumed in REQ-15) | REQ-19 Lenient | Phase 1 [85] propagation |
| 20 | Tab vs space separator before parameters | Strict-conformant | (not tested) | REQ-20 SC | B unique |
| 21 | Trailing comment after `%YAML` accepted | Strict-conformant | (not tested) | REQ-21 SC | B unique |
| 22 | Trailing junk after `%YAML 1.2` rejected | Strict-conformant | REQ-16 SC (digit shape) | REQ-22 SC | Both agree |
| 23 | `%TAG ! ! # primary` comment-after-prefix absorbed into prefix | Lenient | (not tested) | REQ-23 Lenient | B unique |
| 24 | `%TAG !foo` missing trailing `!` rejected | Strict-conformant | REQ-17 SC (parameter shape) | REQ-24 SC | Both agree |
| 25 | `%TAG` named handle with underscore rejected | Strict-conformant | (not tested) | REQ-25 SC | B unique |
| 26 | `%TAG` named handle with hyphen accepted | Strict-conformant | (not tested) | REQ-26 SC | B unique |
| 27 | `%TAG` missing prefix rejected | Strict-conformant | (not tested) | REQ-27 SC | B unique |
| 28 | `%YAML` directive without `---` rejected | Strict-conformant | REQ-19 SC | REQ-28 SC | Both agree |
| 29 | Indented `%` line not recognized as directive | Strict-conformant | REQ-20 Indeterminate (resolved) | (not tested) | A unique; lead resolved by code reading |

## Resolved Disagreements

### Should-warn / should-adjust cases (Items 4, 5, 15)

**A's verdict on items 4/5/15:** `Lenient`. Reasoning: spec uses "should ... with appropriate warning" or "with appropriate adjustment"; the parser silently accepts/ignores without surfacing any indicator.

**B's verdict on items 4/5/15:** `Strict-conformant`. Reasoning: "should" is non-mandatory per RFC 2119; silent ignore is permitted. The implementation surfaces the parsed `version: Some((major, minor))` field in `DocumentStart`, allowing consumer-side handling of any version differences.

**Lead's investigation:** Phase 1 already resolved this interpretation question for `[83] ns-reserved-directive` (`reconciliation-§6.md`) in B's favor — silent acceptance is conformant when the spec uses "should." For the three §6.8 cases:

- **Item 4 (1.3 processed with warning):** the parser stores `version: (1, 3)` in `DocumentStart` and parses with 1.2 rules. Spec says "should be processed with an appropriate warning." Since the version is surfaced to the consumer, the consumer can emit a warning at its layer. The parser's silent acceptance is permissible per "should."
- **Item 5 (1.1 processed with adjustment):** the parser stores `version: (1, 1)` and parses with 1.2 rules (no 1.1-specific lexical adjustments). Spec says "should be processed with appropriate adjustment." Since the version is surfaced, the consumer can apply 1.1 adjustments; the parser's uniform 1.2 rule application is permissible per "should."
- **Item 15 (unknown directive ignored with warning):** the parser silently increments `directive_count`; no warning. Spec says "should ignore unknown directives with an appropriate warning." Phase 1 [83] precedent applied directly: "should is non-mandatory."

**Lead's verdict on items 4, 5, 15:** `Strict-conformant`.

A's underlying architectural observation is preserved in the Architectural Findings section below.

## Architectural Findings (cross-cutting)

These findings span multiple requirements and represent design constraints rather than per-requirement defects. They are surfaced here for the doc-rewrite plan and for the post-Phase-2 design-decisions batch — the user may choose to pursue them as enhancements.

### A1. No Warning event variant or collector

**Observation (Auditor A's discovery, applied across multiple §6.8 requirements):** the parser's event stream has only success/error semantics — `Result<(Event, Span), Error>` from `parse_events()`. There is no `Event::Warning(...)` variant, no warning collector, no diagnostic side-channel.

**Effect on §6.8 conformance:** the spec uses "should ... with appropriate warning" three times in §6.8 (1.1 adjustment, 1.3 acceptance, reserved-directive ignore). All three are reconciled `Strict-conformant` because "should" is non-mandatory, but the parser cannot honor the spec's "appropriate warning" suggestion at all. A consumer that wants to surface warnings must derive them from observable state (e.g., reading `DocumentStart.version` and emitting its own warning when `version != Some((1, 2))`) rather than receiving them from the parser.

**Effect beyond §6.8:** the same pattern likely applies to §5.1 (`is_c_printable` warnings), §5.2 BOM-strip behaviors, and other places where the spec uses "should." Phase 2's later areas (especially §6.9.1 tag resolution) may surface additional cases.

**Disposition for post-Phase-2 batch:** this is a **design-enhancement candidate**, not a Lenient finding. The user may decide whether to add a Warning channel as a future feature (would benefit LSP-style consumers in particular) or formally accept the absence with a documented rationale.

### A2. Strict-conformant items with thin rationale (`MAX_DIRECTIVES_PER_DOC=64`)

**Observation (B's REQ-16):** the parser hard-rejects when `directive_count >= MAX_DIRECTIVES_PER_DOC` at `event_iter/directives.rs:75-83`. The spec has no per-document directive count limit; this is implementation-defined defensive behavior similar to Phase 1's `[86]` (`major == 0` rejection) and `[87]` (u8 digit cap).

**Disposition for post-Phase-2 batch:** Stricter-than-spec verdict is correct per the audit; rationale ("DoS protection / sanity limit") is reasonable but should be documented in the conformance doc with the specific value (64) and the rationale. The user may decide whether to make the limit configurable via `LoaderOptions` or keep it hardcoded.

## Disposition for Phase 2 Summary (Task 8)

**New follow-up entries to file** (not yet in `project_followup_plans.md`):

1. **`%TAG` comment-after-prefix absorption (§6.8.2, item 23)** — `%TAG ! ! # primary` absorbs `# primary` into the prefix because the prefix scanner does not honor `s-l-comments` after `ns-tag-prefix`. `event_iter/directives.rs` (tag-directive parsing); fix is to consume optional whitespace + comment-or-EOL after the prefix. Verdict `Lenient`.

2. **`MAX_DIRECTIVES_PER_DOC=64` hardcoded limit (§6.8, item 16)** — the parser hard-errors on the 65th directive in a single document. Spec has no such limit; this is defensive but undocumented. `event_iter/directives.rs:75-83`. Verdict `Stricter-than-spec` — preserve the limit but document the choice (or make configurable).

**Items deduplicated against existing follow-ups** (will be cited in summary's Deduplicated subsection):

3. **NUL bytes in directive name (item 18)** and **NUL bytes in directive parameter (item 19)** — both behavioral confirmations of Phase 1 [84] / [85] Lenient findings. Already filed at `775020c` ("Directive name + parameter validation"). Same spec section + same code location → dedup.
4. **Major-0 rejection (item 6)** and **Minor digit overflow (item 7)** — both behavioral confirmations of Phase 1 [86] / [87] Stricter-than-spec findings. These do not generate follow-up entries (Stricter-than-spec entries don't auto-file per the dedup rule).

**Architectural observation to surface in summary** (not a Lenient/Non-conformant fix entry, but worth user awareness):

- **No Warning channel.** Three §6.8 "should ... with appropriate warning" requirements pass `Strict-conformant` via "should is non-mandatory," but the parser fundamentally cannot emit warnings. Future-design discussion item.

## Methodology Notes

- The §5.2 lesson about probe cleanup carried successfully into §6.8: both auditors confirmed clean `git status` at completion. Auditor A used a standalone `/tmp/audit-probe-§6.8/` Cargo project; auditor B used `/tmp/audit-probe-6.8-b/` (note the slightly different naming — both outside the git tree). Neither left files in `rlsp-yaml-parser/`.
- B's broader enumeration (28 requirements vs A's 20) reflects a more aggressive decomposition of the spec text. Notable: B tested case sensitivity, `MAX_DIRECTIVES_PER_DOC`, comment-after-prefix, and several malformed-handle variants that A did not. A surfaced one item (indented `%`) that B missed. Cross-coverage is the value of the dual-track design.
- Phase 1 [83] precedent on "should is non-mandatory" continues to hold and should be repeated explicitly in future Phase 2 dispatch prompts so auditors don't re-litigate the question independently.
