**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-30

## Goal

Audit the `rlsp-yaml-parser` crate for YAML 1.2.2 conformance at the BNF-production level using a dual-track methodology — two independent subagents per spec chapter that the lead reconciles — and produce committed findings in `.ai/audit/` that establish ground truth for which productions are strict-conformant, stricter-than-spec, lenient, or non-conformant. The audit's existing conformance documentation (`rlsp-yaml-parser/docs/yaml-spec-conformance.md`) cannot currently be trusted as ground truth — at least two of its "Conformant" entries (`[1] c-printable` and `[2] nb-json`) describe enforcement gaps in passing while still carrying the conformant label. The output of this plan is the corrected verdict table for every BNF production in §3–§9 of the spec, the rationale behind each verdict, and a summary that surfaces follow-up work (gaps to fix, deviations to formally accept) for the user to decide on.

## Context

- The `rlsp-yaml-parser` crate is the project's authority on valid YAML; downstream `rlsp-yaml/` consumes its events and AST without re-parsing structure.
- The conformance doc at `rlsp-yaml-parser/docs/yaml-spec-conformance.md` is ~2000 lines indexing entries across spec chapters §3, §4, §5, §6, §7, §8, §9, §10. Each entry has Classification, Spec quote, Implementation citation, Test coverage, and (where applicable) Discrepancy + Rationale fields.
- The doc was likely produced by walking the BNF list and labeling "Conformant" wherever the matching predicate existed in `chars.rs` — a methodology that cannot detect enforcement gaps. This plan audits the doc by independent verification rather than trusting its labels.
- Entry counts per chapter (from `awk '/^## §/{...} /^### \[/{...}'` over the doc): §3 (1 meta-notation entry), §4 (1 meta-notation entry), §5 (62 BNF productions), §6 (41 BNF productions), §7 (58 BNF productions), §8 (40 BNF productions), §9 (10 BNF productions), §10 (0 BNF, prose-only). Total 213 entries — 211 BNF productions plus 2 meta-notation entries that the doc currently classifies as Not-applicable.
- §10 (presentation/output) has no BNF entries and is out of scope for a parser audit; relevant to formatter conformance, which is Phase 2 / separate work.
- §3 and §4 each have 1 meta-notation entry and are folded into the §5 audit task to avoid spawning subagents for single-entry sections; these meta entries are expected to verdict as `Not-applicable` but receive their own evidence + reasoning lines per the audit format.
- Local spec reference: `.ai/references/yaml-1.2.2-spec.md` (6725 lines, full YAML 1.2.2 text).
- Online spec reference: `https://yaml.org/spec/1.2.2/`.
- Project convention `One parser, one AST` (root `CLAUDE.md`) means parser correctness is the load-bearing dependency for every higher-level feature; conformance gaps here propagate to all consumers.
- A separate Phase 2 plan, drafted after Phase 1 reconciliation completes, will audit normative-prose requirements that have no BNF anchor: encoding (§5.2), tag resolution (§6.9.1), schema resolution (§10.1–10.3 Failsafe/JSON/Core), error semantics, and limits.

## Steps

- [ ] Create the audit output directory `.ai/audit/2026-04-30-phase1-bnf/`
- [ ] Audit §5 (with §3, §4 folded in): dispatch A + B subagents, reconcile, commit
- [ ] Audit §6: dispatch A + B subagents, reconcile, commit
- [ ] Audit §7: dispatch A + B subagents, reconcile, commit
- [ ] Audit §8: dispatch A + B subagents, reconcile, commit
- [ ] Audit §9: dispatch A + B subagents, reconcile, commit
- [ ] Compose final summary across all chapters; file follow-up entries from summary; commit
- [ ] Mark plan Completed and commit status update

## Tasks

### Task 1: Audit §3, §4, §5 (character productions)

Dispatch two independent subagents to audit all 64 entries under `## §3`, `## §4`, and `## §5` in the conformance doc (1 meta entry from §3, 1 meta entry from §4, 62 BNF productions from §5). Reconcile the two outputs, commit the chapter's audit files.

Subagent A (`audit-a-§5`):
- Inputs: parser source (`rlsp-yaml-parser/src/`) and `.ai/references/yaml-1.2.2-spec.md`.
- Forbidden: must not read `rlsp-yaml-parser/docs/yaml-spec-conformance.md`. Must not fetch the online spec.
- Output: `.ai/audit/2026-04-30-phase1-bnf/audit-a-§5.md`.

Subagent B (`audit-b-§5`):
- Inputs: parser source (`rlsp-yaml-parser/src/`), the online spec at `https://yaml.org/spec/1.2.2/`, and `rlsp-yaml-parser/docs/yaml-spec-conformance.md`.
- Forbidden: must not read `.ai/references/yaml-1.2.2-spec.md`. Must verify each conformance-doc claim against the actual code rather than accepting it.
- Output: `.ai/audit/2026-04-30-phase1-bnf/audit-b-§5.md`.

Both subagents use the verdict taxonomy:
- `Strict-conformant` — implementation matches spec exactly
- `Stricter-than-spec` — rejects more than spec requires (with rationale)
- `Lenient` — accepts what spec rejects (with location)
- `Non-conformant` — produces wrong output for valid spec input
- `Not-applicable` — production does not apply to a parser implementation
- `Indeterminate` — auditor cannot verdict without further work

Both subagents produce one section per BNF production with this exact shape:

```
### [N] production-name

BNF: <verbatim BNF from spec>
Spec prose: <quote of relevant normative prose>
Verdict: <one taxonomy label>
Evidence: <parser file:line citations>
Reasoning: <one or two paragraphs explaining how the code matches or diverges from the spec, citing both>
```

For the two meta-notation entries from §3 and §4 (which have no numeric production number), use the conformance doc's identifier as the heading: `### [§3] Not Applicable (descriptive)` and `### [§4] Not Applicable (meta-notation)`. The BNF field reads `(no BNF — meta-notation)`.

Both subagent outputs begin with this YAML frontmatter:

```yaml
---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: A | B
section: §5 (with §3, §4)
date: 2026-04-30
---
```

After both subagent outputs are complete, the lead writes `.ai/audit/2026-04-30-phase1-bnf/reconciliation-§5.md` containing:
- A per-production reconciliation row: where A and B agree, a one-line `Agreed: <verdict>`; where they disagree, both verdicts and reasoning, the lead's investigation, and the lead's verdict.
- For any production where the lead cannot resolve a disagreement from the two outputs alone, the lead must research independently (re-read the cited code and spec sections), record their own verdict, and tag the entry `[NEEDS USER REVIEW]` with a one-line reason. These flagged entries propagate into the final summary's escalation list.
- Frontmatter side: `Reconciliation`. produced-by: `lead`.

Acceptance criteria:
- [ ] `.ai/audit/2026-04-30-phase1-bnf/audit-a-§5.md` exists and contains one verdict-bearing entry for every entry under `## §3`, `## §4`, `## §5` in the conformance doc — 64 entries total (1 + 1 + 62).
- [ ] `.ai/audit/2026-04-30-phase1-bnf/audit-b-§5.md` exists with the same 64-entry coverage.
- [ ] Each audit file's frontmatter contains all five required fields: `plan`, `phase`, `side`, `section`, `date`.
- [ ] Every entry in both files carries a verdict from the taxonomy, parser file:line evidence, and reasoning citing spec wording.
- [ ] No entry is missing reasoning or relies on hedge words ("looks correct," "probably conformant," "should be fine").
- [ ] `.ai/audit/2026-04-30-phase1-bnf/reconciliation-§5.md` exists with frontmatter fields `plan`, `phase`, `side: Reconciliation`, `section`, `date`, `produced-by: lead`; covers every production; and resolves every disagreement either with a lead verdict or with `[NEEDS USER REVIEW]`.
- [ ] All three files committed in one commit: `docs(audit): record phase 1 §5 conformance audit`.

### Task 2: Audit §6 (structural productions)

Same shape as Task 1, but for the 41 productions under `## §6` in the conformance doc.

Subagent A (`audit-a-§6`) and Subagent B (`audit-b-§6`) follow the same input partitioning, blindness rules, taxonomy, entry shape, and frontmatter as Task 1. Output paths replace `§5` with `§6`.

Acceptance criteria:
- [ ] `audit-a-§6.md`, `audit-b-§6.md`, and `reconciliation-§6.md` exist with complete coverage of all 41 §6 productions.
- [ ] Each file's frontmatter contains all five required fields (`plan`, `phase`, `side`, `section`, `date`); reconciliation file additionally carries `produced-by: lead`.
- [ ] Every entry has verdict + evidence + reasoning per the format above.
- [ ] All three files committed: `docs(audit): record phase 1 §6 conformance audit`.

### Task 3: Audit §7 (flow style productions)

Same shape as Task 1, but for the 58 productions under `## §7` in the conformance doc.

Acceptance criteria:
- [ ] `audit-a-§7.md`, `audit-b-§7.md`, and `reconciliation-§7.md` exist with complete coverage of all 58 §7 productions.
- [ ] Each file's frontmatter contains all five required fields (`plan`, `phase`, `side`, `section`, `date`); reconciliation file additionally carries `produced-by: lead`.
- [ ] Every entry has verdict + evidence + reasoning per the format above.
- [ ] All three files committed: `docs(audit): record phase 1 §7 conformance audit`.

### Task 4: Audit §8 (block style productions)

Same shape as Task 1, but for the 40 productions under `## §8` in the conformance doc.

Acceptance criteria:
- [ ] `audit-a-§8.md`, `audit-b-§8.md`, and `reconciliation-§8.md` exist with complete coverage of all 40 §8 productions.
- [ ] Each file's frontmatter contains all five required fields (`plan`, `phase`, `side`, `section`, `date`); reconciliation file additionally carries `produced-by: lead`.
- [ ] Every entry has verdict + evidence + reasoning per the format above.
- [ ] All three files committed: `docs(audit): record phase 1 §8 conformance audit`.

### Task 5: Audit §9 (document and stream productions)

Same shape as Task 1, but for the 10 productions under `## §9` in the conformance doc.

Acceptance criteria:
- [ ] `audit-a-§9.md`, `audit-b-§9.md`, and `reconciliation-§9.md` exist with complete coverage of all 10 §9 productions.
- [ ] Each file's frontmatter contains all five required fields (`plan`, `phase`, `side`, `section`, `date`); reconciliation file additionally carries `produced-by: lead`.
- [ ] Every entry has verdict + evidence + reasoning per the format above.
- [ ] All three files committed: `docs(audit): record phase 1 §9 conformance audit`.

### Task 6: Compose summary and file follow-ups

Lead-authored consolidation across all chapters. The summary lives at `.ai/audit/2026-04-30-phase1-bnf/summary.md` with frontmatter `side: Summary` and `produced-by: lead`.

Required content:

1. **Verdict table** — one row per BNF production [1] through the highest audited number, columns: number, name, chapter, final verdict, evidence pointer (link to the chapter's reconciliation file).
2. **Lenient productions** — list of all productions whose final verdict is `Lenient`, with the nature of the gap and a one-line summary of what would be required to make it conformant.
3. **Stricter-than-spec productions** — list of all productions whose final verdict is `Stricter-than-spec`, with the rationale (typically security hardening) preserved as the rationale for not loosening.
4. **Non-conformant productions** — list of all productions whose final verdict is `Non-conformant`, with one-line description of the deviation.
5. **`[NEEDS USER REVIEW]` items** — every production flagged across reconciliation files, gathered into one list with the disagreement summary and the lead's tentative verdict. The user reads this list and adjudicates before the summary is treated as ground truth.
6. **Follow-up filing** — for every production whose final verdict is `Lenient` or `Non-conformant`, append a corresponding entry to `.ai/memory/project_followup_plans.md` under `Open: rlsp-yaml-parser`, except where an existing entry in `project_followup_plans.md` already names the same spec section AND the same code location (file path + line range). Deduplication is by spec section and code location, not by topic — multiple productions whose gaps share a topic but differ in spec section or code location each get their own entry. Productions with verdicts `Strict-conformant`, `Stricter-than-spec`, `Not-applicable`, or `Indeterminate` do not generate follow-up entries from this rule. The c-printable §5.1 entry committed in `6f0ec6d` is the only known existing dedupe target at plan-creation time. New entries follow the existing followup format (bold title, prose body, references to spec section and code locations).

Acceptance criteria:
- [ ] `.ai/audit/2026-04-30-phase1-bnf/summary.md` exists with frontmatter fields `plan`, `phase`, `side: Summary`, `section: all`, `date`, `produced-by: lead`.
- [ ] All six required sections (Verdict table, Lenient productions, Stricter-than-spec productions, Non-conformant productions, `[NEEDS USER REVIEW]` items, Follow-up filing) are present in the summary.
- [ ] Verdict table covers every entry audited across Tasks 1–5; row count equals exactly 213 (the sum of per-chapter entry counts: 64 + 41 + 58 + 40 + 10).
- [ ] Every `Lenient`, `Stricter-than-spec`, and `Non-conformant` entry has at least one supporting evidence pointer to a reconciliation file.
- [ ] Every `[NEEDS USER REVIEW]` item is enumerated with the lead's tentative verdict.
- [ ] For every production with final verdict `Lenient` or `Non-conformant`, either (a) a corresponding entry exists in `project_followup_plans.md` after this task, or (b) the summary explicitly cites the existing `project_followup_plans.md` entry that already names the same spec section AND the same code location.
- [ ] Two commits land in this task: `docs(audit): record phase 1 conformance audit summary` for the summary file, and `chore(memory): file phase 1 conformance audit follow-ups` for the memory update. Plan status update to `Completed (2026-04-30)` lands in a final commit `docs(rlsp-yaml-parser): mark phase 1 audit plan complete`.

## Decisions

- **Dual-track methodology with strict input partitioning.** Subagent A reads parser source + `.ai/references/yaml-1.2.2-spec.md` only; subagent B reads parser source + `https://yaml.org/spec/1.2.2/` + `rlsp-yaml-parser/docs/yaml-spec-conformance.md` only. Each dispatch's prompt names the forbidden inputs explicitly. Subagent B's prompt additionally warns against accepting conformance-doc claims without independent code verification — anchoring bias is the residual risk after partitioning.
- **Per-chapter task decomposition.** One task per spec chapter (§5, §6, §7, §8, §9), with §3 and §4's single entries folded into §5. Productions within a chapter cross-reference each other; per-chapter scope keeps that context intact for each subagent.
- **BNF productions only for Phase 1.** Normative-prose conformance (encoding support, schema resolution, tag resolution, error semantics, limits) is deferred to a Phase 2 plan drafted after Phase 1 reconciliation. This keeps Phase 1 mechanical and bounded; Phase 2 needs different methodology (behavioral tests rather than predicate verification) and its scope can be informed by Phase 1 findings.
- **§10 out of scope.** §10 has no BNF productions in the conformance doc and concerns presentation/serialization, which is the formatter's responsibility, not the parser's. Phase 2 (or a separate formatter conformance plan) addresses §10.
- **Lead investigates unresolvable discrepancies.** When reconciliation surfaces a disagreement that the two audit outputs cannot resolve, the lead reads the cited code and spec independently, records their own verdict, and tags the entry `[NEEDS USER REVIEW]`. These tagged entries are enumerated in the summary's escalation section so the user can adjudicate before the summary becomes ground truth.
- **No fixes during the audit.** The audit produces verdicts and follow-up entries only. Closing gaps (e.g., adding c-printable enforcement) happens in separate plans the user prioritizes after reviewing the summary.
- **Output committed as findings land.** Each chapter's audit files commit in one commit per task; the summary and follow-up additions commit as Task 6 completes. Committing ensures findings survive across sessions and become the basis for downstream plans.
- **Subagent identity stamped in frontmatter.** Each output file's frontmatter records the dispatched subagent's identity (or `lead` for reconciliation/summary files), making provenance auditable later.

## Non-Goals

- Implementing fixes for any conformance gap surfaced by the audit. Fixes are separate plans the user prioritizes after reviewing the summary.
- Auditing the formatter (`rlsp-yaml/`) or any code outside `rlsp-yaml-parser/`. The parser is the conformance authority; the formatter audit is separate work.
- Auditing normative-prose requirements without a BNF anchor (encoding, schema resolution, tag resolution, error semantics, limits). Phase 2 plan covers these.
- Auditing §10 (presentation/output). Out of scope for a parser audit; relevant to the formatter.
- Re-writing the existing conformance doc during Phase 1. The audit produces verdicts; rewriting the doc to reflect them is a follow-up after the user adjudicates `[NEEDS USER REVIEW]` items.
- Adding tests for any production. Test gaps surfaced during the audit get filed as follow-up entries; test additions are separate plans.
