**Repository:** root
**Status:** InProgress
**Created:** 2026-04-30

## Goal

Audit the `rlsp-yaml-parser` crate for YAML 1.2.2 conformance against normative-prose requirements that have no BNF anchor — encoding (§5.2), directive semantics (§6.8), tag resolution (§6.9.1), schema resolution (§10.1 Failsafe / §10.2 JSON / §10.3 Core), error semantics, and limits. These requirements describe parser *behavior* — how the parser handles input that has been syntactically accepted, what types it resolves scalars to, what errors it produces, what limits it enforces — and cannot be verified by predicate inspection alone. The output of this plan is a committed verdict for each normative-prose requirement, with behavioral evidence (small inputs and observed outputs) supporting each verdict, follow-up entries in `.ai/memory/project_followup_plans.md` for every `Lenient` or `Non-conformant` requirement (except where an existing entry already names the same spec section AND code location — every dedup decision is recorded explicitly in the summary's Deduplicated follow-ups subsection), and an enumerated list in the summary of every `Indeterminate` requirement (auditor could not verdict) so they are visible to the user without auto-filing — `Indeterminate` items are gaps of unknown direction, not gaps that warrant a fix-tracking entry. Phase 1 (BNF productions) is the predecessor; Phase 1 verdicts must be reconciled before Phase 2 begins, because Phase 1 findings may broaden Phase 2's scope (e.g., if Phase 1 surfaces Lenient encoding-detection BNF productions, Phase 2's encoding behavioral audit picks up at the boundary).

## Context

- The `rlsp-yaml-parser` crate is the project's authority on valid YAML; downstream `rlsp-yaml/` consumes its events and AST.
- The predecessor plan audits BNF productions across YAML 1.2.2 §3–§9 using a dual-track methodology with two independent subagents per spec chapter and a lead-authored reconciliation. Its output is a verdict for every grammar production. This plan picks up where the predecessor leaves off, auditing the normative-prose requirements that grammar productions do not cover. The predecessor's dispatch shape, taxonomy, and reconciliation procedure are restated below for self-containment; methodology cohesion across the two phases is intentional.
- The conformance doc at `rlsp-yaml-parser/docs/yaml-spec-conformance.md` is BNF-anchored. It does not systematically cover the normative-prose items in this plan's scope, though some appear inside the prose of individual BNF entries (e.g., `[3] c-byte-order-mark` discusses encoding handling). Phase 2 must not infer that absence from the doc means absence in code — the dual-track methodology applies here too.
- Local spec reference: `.ai/references/yaml-1.2.2-spec.md` (6725 lines, full YAML 1.2.2 text).
- Online spec reference: `https://yaml.org/spec/1.2.2/`.
- Project convention `One parser, one AST` (root `CLAUDE.md`) means parser correctness is load-bearing; behavioral conformance gaps in encoding/schema/tag resolution propagate as wrong-typed values to every downstream consumer.
- **Behavioral methodology.** Phase 2 audits cannot be performed by reading code alone — many normative requirements are about input/output relationships. Both subagents must design small inputs, run them through the parser (via the crate's public API: `parse_events()` and `load()`), and compare actual output to spec expectation. Subagents may write throwaway test programs in the parser's existing test infrastructure, but **must not commit those programs** — scratch tools are not deliverables (per `.ai/memory/feedback_scratch_tools_not_deliverables.md`). Audit output records what was tested, what was observed, and the verdict.
- Phase 1 findings that focus Phase 2's behavioral scope per area are recorded in the "Phase 1 Findings" section below.

## Phase 1 Findings

Phase 1 (BNF audit, summary at `.ai/audit/2026-04-30-phase1-bnf/summary.md`) surfaced findings that focus Phase 2's behavioral scope per area:

**Affecting Task 1 (§5.2 Character Encodings):**
- Phase 1 verdict on [3] c-byte-order-mark is `Strict-conformant` at the BNF level — BOM detection works at stream start (`encoding.rs:88-96`) and via document-boundary signaling (`lines.rs:292-303`). Tested only at the predicate level. Phase 2 Task 1 must verify end-to-end: UTF-8 / UTF-16-BE / UTF-16-LE / UTF-32-BE / UTF-32-LE inputs parse identically; encoding detection without BOM via the spec's first-character heuristic (§5.2); BOM at document-prefix vs mid-stream rejection.

**Affecting Task 2 (§6.8 Directives):**
- Phase 1 found `[84] ns-directive-name` and `[85] ns-directive-parameter` Lenient — body shapes not validated against `ns-char+`. Phase 2 Task 2 must verify behavioral observable: parser accepts `%FOO bad\x00content` end-to-end and what it produces (silent-ignore per spec "should ignore unknown directives").
- Phase 1 found `[86] ns-yaml-directive` Stricter-than-spec — rejects `major == 0`. Phase 2 Task 2 must test: `%YAML 0.5` rejected; `%YAML 1.1` / `%YAML 1.2` accepted; `%YAML 1.3` behavior (spec says "should be processed with appropriate warning"); `%YAML 2.0` rejected.
- Phase 1 found `[87] ns-yaml-version` Stricter-than-spec — `parse::<u8>` bounds digits to [0, 255]. Phase 2 Task 2 must test: `%YAML 1.300` rejected (minor > 255); arbitrary-digit count rejected.
- Phase 1's `[83] ns-reserved-directive` resolved Strict-conformant via "should is non-mandatory" reading with thin rationale. Phase 2 Task 2 must re-verify behaviorally: confirm silent-ignore matches spec intent end-to-end.

**Affecting Task 3 (§6.9.1 Tag Resolution):**
- Phase 1 found `[93]`/`[94]`/`[95]` tag prefix validation Lenient — rejects only ASCII control + DEL, not full `ns-uri-char`. Phase 2 Task 3 must verify: does the parser accept malformed prefixes like `%TAG !x! tag:!badprefix!` end-to-end and what output does it produce? Complements Phase 1's grammar-level finding with behavioral evidence.
- Phase 1 found `[99] c-ns-shorthand-tag` Lenient — accepts empty suffixes (`!!` and `!handle!`). Phase 2 Task 3 must verify: does `!! plain` resolve to a node with the bare secondary tag? What about `!handle! plain`? End-to-end behavior with the resolution algorithm.

**No additions from Phase 1 affecting Tasks 4-6 (Failsafe / JSON / Core schemas).** Schemas are pure normative prose; Phase 1 BNF audit had no schema-related findings. Phase 2 schema tasks proceed with no Phase 1 prior context.

**Affecting Task 7 (Error semantics and limits):**
- Phase 1's 5 Stricter-than-spec entries reject input at specific positions: `[59]`/`[60]`/`[61]` numeric escape rejections; `[86]` major-0 rejection; `[87]` u8 digit limit. Phase 2 Task 7 error-position checks must verify the reported positions point to the actual offending byte for each.
- Phase 1's `[192] ns-l-block-map-implicit-entry` audit confirmed the 1024-Unicode-char limit on implicit block keys uses `chars().count()` (Unicode-correct). Phase 2 Task 7 must verify the same correctness for the 1024-char implicit flow-key limit at `event_iter/flow.rs:1136-1161`, with multi-byte test content.

**Methodology carryover from Phase 1:**
- The symmetric reconciliation principle ("attribute strictness/leniency to the production where the rule is enforced; parent productions that correctly compose are conformant") must be repeated in every Phase 2 dispatch prompt to keep verdicts consistent.
- Phase 1 surfaced one prose-vs-BNF terminology trap (§7.3.x vs §7.4.2 implicit-key restriction; resolved in `reconciliation-§7.md` entry [110]). Phase 2 may surface similar traps in normative prose; the BNF-trace resolution procedure used there is the template — when a `[NEEDS USER REVIEW]` flag is raised, the lead investigates by tracing BNF context propagation and concrete examples, recording the resolution durably in the audit record.

## Steps

- [x] Wait for Phase 1 plan completion (`Status: Completed`); read its summary and reconciliation files
- [x] Populate the "Phase 1 Findings" section of this plan with scope additions
- [ ] Audit area 1 (encoding §5.2): dispatch A + B subagents, reconcile, commit
- [ ] Audit area 2 (directives §6.8): dispatch A + B subagents, reconcile, commit
- [ ] Audit area 3 (tag resolution §6.9.1): dispatch A + B subagents, reconcile, commit
- [ ] Audit area 4 (Failsafe schema §10.1): dispatch A + B subagents, reconcile, commit
- [ ] Audit area 5 (JSON schema §10.2): dispatch A + B subagents, reconcile, commit
- [ ] Audit area 6 (Core schema §10.3): dispatch A + B subagents, reconcile, commit
- [ ] Audit area 7 (error semantics + limits): dispatch A + B subagents, reconcile, commit
- [ ] Compose final summary across all areas; file follow-up entries; commit
- [ ] Mark plan Completed and commit status update

## Tasks

### Task 1: Audit §5.2 Character Encodings

Audit the parser's encoding-detection behavior. The spec normative requirements (§5.2):
- "On input, a YAML processor must accept the byte order mark."
- "Implementations should support UTF-8, UTF-16 (with both byte orders) and UTF-32 (with both byte orders)."
- "The character encoding is a presentation detail and must not be used to convey content information."
- BOM rules at stream start, document start, mid-stream behavior.

Dispatch two independent subagents to verify these behaviors via small inputs and observed parser output.

**Subagent A** (`audit-a-§5.2`):
- Inputs: parser source (`rlsp-yaml-parser/src/`), `.ai/references/yaml-1.2.2-spec.md`, the parser's existing test corpus at `rlsp-yaml-parser/tests/`. May write small test programs into the parser's test directory to observe behavior, but MUST NOT commit them.
- Forbidden: `rlsp-yaml-parser/docs/yaml-spec-conformance.md`. Online spec (no WebFetch).
- Output: `.ai/audit/2026-04-30-phase2-prose/audit-a-§5.2.md`.

**Subagent B** (`audit-b-§5.2`):
- Inputs: parser source, online spec at `https://yaml.org/spec/1.2.2/`, `rlsp-yaml-parser/docs/yaml-spec-conformance.md`, parser test corpus. May write small test programs but MUST NOT commit them.
- Forbidden: `.ai/references/yaml-1.2.2-spec.md`. Anti-bias: do not accept conformance-doc claims without behavioral verification.
- Output: `.ai/audit/2026-04-30-phase2-prose/audit-b-§5.2.md`.

**Verdict taxonomy (both subagents — same as Phase 1):**
- `Strict-conformant` — implementation matches spec exactly
- `Stricter-than-spec` — rejects more than spec requires (with rationale)
- `Lenient` — accepts what spec rejects (with location)
- `Non-conformant` — produces wrong output for valid spec input
- `Not-applicable` — requirement does not apply to a parser implementation
- `Indeterminate` — auditor cannot verdict without further work

**Per-requirement entry shape (both subagents):**

```
### REQ-§5.2-N: <one-line requirement summary>

Spec requirement: <verbatim quote of the normative sentence with section citation>
Test method: <description of the input(s) used to exercise this requirement>
Test input: <the actual input bytes/text used>
Observed output: <what the parser produced — events, errors, or loaded value>
Spec expectation: <what the spec says should happen>
Verdict: <one taxonomy label>
Evidence: <parser file:line citations for the code path exercised, plus reference to the test program if one was written>
Reasoning: <one or two paragraphs explaining how the observed behavior matches or diverges from the spec; if conformance doc disagrees, note it>
```

Requirement IDs are sequential within an area (`REQ-§5.2-1`, `REQ-§5.2-2`, ...). Both subagents must enumerate the same set of requirements; reconciliation matches by requirement ID.

**Frontmatter (both subagents):**

```yaml
---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: A | B
section: §5.2
date: 2026-04-30
---
```

**Reconciliation (lead-authored):** `.ai/audit/2026-04-30-phase2-prose/reconciliation-§5.2.md` covers every requirement; resolves disagreements with lead verdict or `[NEEDS USER REVIEW]`. Frontmatter `side: Reconciliation`, `produced-by: lead`.

**Acceptance criteria:**
- [ ] `audit-a-§5.2.md` and `audit-b-§5.2.md` exist.
- [ ] Each audit file's frontmatter contains all five required fields: `plan`, `phase`, `side` (`A` or `B`), `section`, `date`.
- [ ] Each audit file enumerates the same set of normative-prose requirements from §5.2 and assigns sequential REQ IDs (`REQ-§5.2-1`, `REQ-§5.2-2`, …).
- [ ] Every requirement entry includes all eight required fields: Spec requirement, Test method, Test input, Observed output, Spec expectation, Verdict, Evidence, Reasoning.
- [ ] Every requirement entry's Verdict is one of the six taxonomy labels (Strict-conformant / Stricter-than-spec / Lenient / Non-conformant / Not-applicable / Indeterminate).
- [ ] No throwaway test programs left committed in `rlsp-yaml-parser/tests/` or anywhere else after the task — verified by `git status` showing no new test files after the task commit.
- [ ] No hedge words anywhere in the audit files.
- [ ] `reconciliation-§5.2.md` exists with frontmatter fields `plan`, `phase`, `side: Reconciliation`, `section`, `date`, `produced-by: lead`; covers every requirement; resolves every disagreement with a lead verdict or `[NEEDS USER REVIEW]`.
- [ ] All three files committed in one commit: `docs(audit): record phase 2 §5.2 conformance audit`.

### Task 2: Audit §6.8 Directives

Audit the parser's handling of `%YAML` and `%TAG` directives. Spec normative requirements include:
- `%YAML` version compatibility — what the parser does with `%YAML 1.1`, `%YAML 1.2`, `%YAML 1.3`, malformed versions.
- `%TAG` directive semantics — primary handle, secondary handle, named handles, prefix resolution, scope (per-document).
- Reserved/unknown directives — handling per spec ("YAML processors should ignore them with an appropriate warning").
- Directive duplication and ordering rules.

Same subagent shape, taxonomy, entry format (Spec requirement, Test method, Test input, Observed output, Spec expectation, Verdict, Evidence, Reasoning), and reconciliation procedure as Task 1, with `§5.2` replaced by `§6.8` in output paths and frontmatter `section` field.

**Acceptance criteria:**
- [ ] `audit-a-§6.8.md`, `audit-b-§6.8.md`, and `reconciliation-§6.8.md` exist.
- [ ] Each audit file's frontmatter contains all five required fields: `plan`, `phase`, `side` (`A` or `B`), `section`, `date`.
- [ ] Reconciliation file's frontmatter contains all six required fields: `plan`, `phase`, `side: Reconciliation`, `section`, `date`, `produced-by: lead`.
- [ ] Every entry includes Spec requirement, Test method, Test input, Observed output, Spec expectation, Verdict, Evidence, Reasoning.
- [ ] Every requirement entry produces a verdict from the taxonomy (Strict-conformant / Stricter-than-spec / Lenient / Non-conformant / Not-applicable / Indeterminate).
- [ ] No hedge words anywhere.
- [ ] No throwaway test programs left committed (verified by `git status` showing no new test files after the task commit).
- [ ] All three files committed in one commit: `docs(audit): record phase 2 §6.8 conformance audit`.

### Task 3: Audit §6.9.1 Tag Resolution

Audit the parser's tag resolution algorithm. Spec normative requirements include:
- Verbatim tags (`!<tag:yaml.org,2002:str>`) preserved as-is.
- Primary handle resolution (`!foo` → application-specific or default `!`).
- Secondary handle resolution (`!!foo` → `tag:yaml.org,2002:foo` by default).
- Named handle resolution per `%TAG` directive.
- Default tag assignment for unresolved nodes (scalar `?`, sequence `!seq`, mapping `!map` per active schema).

Same subagent shape, taxonomy, entry format, and reconciliation procedure as Task 1, with `§5.2` replaced by `§6.9.1`.

**Acceptance criteria:**
- [ ] `audit-a-§6.9.1.md`, `audit-b-§6.9.1.md`, and `reconciliation-§6.9.1.md` exist.
- [ ] Each audit file's frontmatter contains all five required fields: `plan`, `phase`, `side`, `section`, `date`.
- [ ] Reconciliation file's frontmatter contains all six required fields: `plan`, `phase`, `side: Reconciliation`, `section`, `date`, `produced-by: lead`.
- [ ] Every entry includes Spec requirement, Test method, Test input, Observed output, Spec expectation, Verdict, Evidence, Reasoning.
- [ ] Every requirement entry produces a verdict from the taxonomy.
- [ ] No hedge words anywhere.
- [ ] No throwaway test programs left committed.
- [ ] All three files committed: `docs(audit): record phase 2 §6.9.1 conformance audit`.

### Task 4: Audit §10.1 Failsafe Schema

Audit the parser's node resolution under the Failsafe schema. Spec normative requirements:
- Failsafe defines three tags: `tag:yaml.org,2002:str`, `tag:yaml.org,2002:seq`, `tag:yaml.org,2002:map`.
- All scalars resolve to `!str` regardless of content.
- Sequences resolve to `!seq`; mappings resolve to `!map`.

Same subagent shape, taxonomy, entry format, and reconciliation procedure as Task 1, with `§5.2` replaced by `§10.1`.

**Acceptance criteria:**
- [ ] `audit-a-§10.1.md`, `audit-b-§10.1.md`, and `reconciliation-§10.1.md` exist.
- [ ] Each audit file's frontmatter contains all five required fields: `plan`, `phase`, `side`, `section`, `date`.
- [ ] Reconciliation file's frontmatter contains all six required fields: `plan`, `phase`, `side: Reconciliation`, `section`, `date`, `produced-by: lead`.
- [ ] Every entry includes Spec requirement, Test method, Test input, Observed output, Spec expectation, Verdict, Evidence, Reasoning.
- [ ] Every requirement entry produces a verdict from the taxonomy.
- [ ] No hedge words anywhere.
- [ ] No throwaway test programs left committed.
- [ ] All three files committed: `docs(audit): record phase 2 §10.1 conformance audit`.

### Task 5: Audit §10.2 JSON Schema

Audit the parser's resolution of plain scalars under the JSON schema. Spec normative requirements:
- JSON schema adds `tag:yaml.org,2002:null`, `tag:yaml.org,2002:bool`, `tag:yaml.org,2002:int`, `tag:yaml.org,2002:float`.
- Resolution per JSON-style regular expressions (e.g., `null`, `true`, `false`; integer regex; float regex).
- Plain scalars not matching any tag fall back to `!str`.

Same subagent shape, taxonomy, entry format, and reconciliation procedure as Task 1, with `§5.2` replaced by `§10.2`.

**Acceptance criteria:**
- [ ] `audit-a-§10.2.md`, `audit-b-§10.2.md`, and `reconciliation-§10.2.md` exist.
- [ ] Each audit file's frontmatter contains all five required fields: `plan`, `phase`, `side`, `section`, `date`.
- [ ] Reconciliation file's frontmatter contains all six required fields: `plan`, `phase`, `side: Reconciliation`, `section`, `date`, `produced-by: lead`.
- [ ] Every entry includes Spec requirement, Test method, Test input, Observed output, Spec expectation, Verdict, Evidence, Reasoning.
- [ ] Every requirement entry produces a verdict from the taxonomy.
- [ ] Tests cover all five named requirement categories from the spec: null (`null`), bool (`true`, `false`), integer (decimal, hex, octal forms per the spec table), float (with and without exponent, plus infinity and NaN per the spec table), and `!str` fallback (at least one negative case where a plain scalar matches no JSON regex).
- [ ] No hedge words anywhere.
- [ ] No throwaway test programs left committed.
- [ ] All three files committed: `docs(audit): record phase 2 §10.2 conformance audit`.

### Task 6: Audit §10.3 Core Schema

Audit the parser's resolution of plain scalars under the Core schema. Spec normative requirements:
- Core schema extends JSON schema with broader recognition: `null`/`Null`/`NULL`/`~`/empty for null; `true`/`True`/`TRUE`/`false`/`False`/`FALSE` for bool; broader integer and float forms.
- Loose recognition is part of the schema's spec definition, not a parser leniency.

Same subagent shape, taxonomy, entry format, and reconciliation procedure as Task 1, with `§5.2` replaced by `§10.3`.

**Acceptance criteria:**
- [ ] `audit-a-§10.3.md`, `audit-b-§10.3.md`, and `reconciliation-§10.3.md` exist.
- [ ] Each audit file's frontmatter contains all five required fields: `plan`, `phase`, `side`, `section`, `date`.
- [ ] Reconciliation file's frontmatter contains all six required fields: `plan`, `phase`, `side: Reconciliation`, `section`, `date`, `produced-by: lead`.
- [ ] Every entry includes Spec requirement, Test method, Test input, Observed output, Spec expectation, Verdict, Evidence, Reasoning.
- [ ] Every requirement entry produces a verdict from the taxonomy.
- [ ] Tests cover the full set of recognized forms per the §10.3 spec tables — every null form, every bool form, every integer form, every float form — plus at least one negative case per type (a form that looks similar but should resolve to `!str`).
- [ ] No hedge words anywhere.
- [ ] No throwaway test programs left committed.
- [ ] All three files committed: `docs(audit): record phase 2 §10.3 conformance audit`.

### Task 7: Audit error semantics and limits

Audit the parser's error reporting and resource limits. The spec leaves much of this implementation-defined but requires *some* documented choice. Coverage:
- Error position accuracy: reported error positions point to the actual offending byte, line, and column on at least one malformed input per error class (lex error, parser state error, limit violation).
- Error recovery: whether the parser stops at first error or continues and reports multiple is documented in `LoaderOptions` / `LoaderBuilder` or module-level docs, AND the documented choice matches observed behavior on a multi-error input.
- Default limits: every category exposed via `LoaderOptions` / `LoaderBuilder` and every internal limit constant in the source has a value, and limit violations produce structured `Error` / `LoadError` results — not panics — verified by feeding inputs that breach each limit and observing the error path.
- Limit categories audited: every category exposed in `LoaderOptions` / `LoaderBuilder` (enumerate from the source at audit time) plus error-position accuracy. The audit must list every category found and verdict each one.

Same subagent shape as Task 1, with `§5.2` replaced by `error-and-limits`.

**Acceptance criteria:**
- [ ] `audit-a-error-and-limits.md`, `audit-b-error-and-limits.md`, and `reconciliation-error-and-limits.md` exist.
- [ ] Each audit file's frontmatter contains all five required fields: `plan`, `phase`, `side` (`A` or `B`), `section`, `date`.
- [ ] Reconciliation file's frontmatter contains all six required fields: `plan`, `phase`, `side: Reconciliation`, `section`, `date`, `produced-by: lead`.
- [ ] Every entry includes Spec requirement, Test method, Test input, Observed output, Spec expectation, Verdict, Evidence, Reasoning.
- [ ] Every limit category exposed via `LoaderOptions` / `LoaderBuilder` produces a verdict from the taxonomy. The audit may not skip a category by reporting "observed" without a verdict.
- [ ] Error-position accuracy receives a verdict for at least one malformed input per error class (lex error, parser state error, limit violation).
- [ ] Limit-violation behavior verdicts confirm structured errors, not panics, for every limit category.
- [ ] No hedge words anywhere.
- [ ] No committed throwaway test programs (verified by `git status` showing no new files in the parser test directory after the task commit).
- [ ] All three files committed: `docs(audit): record phase 2 error semantics and limits conformance audit`.

### Task 8: Compose summary and file follow-ups

Lead-authored consolidation across all areas. Summary at `.ai/audit/2026-04-30-phase2-prose/summary.md` with frontmatter `side: Summary`, `section: all`, `produced-by: lead`.

**Required content:**

1. **Verdict table** — one row per requirement across all areas, columns: requirement ID, area, requirement summary, final verdict, evidence pointer (link to the area's reconciliation file).
2. **Lenient requirements** — list of all requirements with verdict `Lenient`, each with the gap nature and a one-line summary of what would be required to make it conformant.
3. **Stricter-than-spec requirements** — list with the rationale (typically security hardening) preserved as the rationale for not loosening.
4. **Non-conformant requirements** — list with a one-line description of the deviation.
5. **`[NEEDS USER REVIEW]` items** — every flagged requirement, gathered with the disagreement summary (or unresolvable case detail) and the lead's tentative verdict. The user reads this list and adjudicates before the summary becomes ground truth.
6. **`Indeterminate` requirements** — list of requirements the auditors could not verdict, each with a one-line description of what additional information would resolve it. These are conformance gaps of unknown direction; surfacing them in the summary ensures they are visible to the user even though they do not generate auto-filed follow-ups.
7. **Deduplicated follow-ups** — list of every `Lenient` or `Non-conformant` requirement that was NOT filed to `project_followup_plans.md`. Each row names the requirement, the spec section, the code location, and the existing `project_followup_plans.md` entry it deduplicates against. This subsection makes every dedup decision auditable rather than relying on the agent's unrecorded judgment.
8. **Follow-up filing** — for every requirement with verdict `Lenient` or `Non-conformant`, append entry to `.ai/memory/project_followup_plans.md` under `Open: rlsp-yaml-parser`, except where an existing entry already names the same spec section AND the same code location (file path + line range). Every dedup decision must appear in section 7 (Deduplicated follow-ups) above. Deduplication is by spec section and code location, not by topic. Requirements with verdicts `Strict-conformant`, `Stricter-than-spec`, `Not-applicable`, or `Indeterminate` do not generate follow-up entries from this rule (Indeterminate requirements are surfaced in section 6 instead).

**Acceptance criteria:**
- [ ] `summary.md` exists with frontmatter fields `plan`, `phase`, `side: Summary`, `section: all`, `date`, `produced-by: lead`.
- [ ] All eight required sections are present (Verdict table, Lenient, Stricter-than-spec, Non-conformant, `[NEEDS USER REVIEW]`, Indeterminate, Deduplicated follow-ups, Follow-up filing).
- [ ] Verdict table row count equals the sum of per-task requirement counts; no requirement audited across Tasks 1–7 is missing from the table.
- [ ] Lenient list includes gap nature and one-line fix summary for each entry.
- [ ] Stricter-than-spec list includes rationale for each entry.
- [ ] Non-conformant list includes one-line deviation description for each entry.
- [ ] `[NEEDS USER REVIEW]` list enumerates every flagged item with the lead's tentative verdict.
- [ ] Indeterminate list enumerates every Indeterminate requirement with a one-line description of what would resolve it.
- [ ] Deduplicated follow-ups list explicitly names every `Lenient` or `Non-conformant` requirement that was not filed, with the existing `project_followup_plans.md` entry it deduplicates against.
- [ ] For every `Lenient` or `Non-conformant` requirement, either (a) a new entry exists in `project_followup_plans.md` after this task, or (b) it appears in the Deduplicated follow-ups list with a cited existing entry. There is no third option.
- [ ] Two commits land: `docs(audit): record phase 2 conformance audit summary` and `chore(memory): file phase 2 conformance audit follow-ups`. Plan status update lands in `docs(rlsp-yaml-parser): mark phase 2 audit plan complete`.

## Decisions

- **Behavioral methodology, not predicate verification.** Phase 2 audits run small inputs through the parser and compare actual output to spec expectation. Subagents may write throwaway test programs, but must not commit them — committed audit output records what was tested, what was observed, and the verdict.
- **Dual-track methodology preserved from Phase 1.** Independent A and B subagents per area, with strict input partitioning. Subagent B is warned against accepting conformance-doc claims without behavioral verification, mirroring Phase 1.
- **Per-area task decomposition.** One task per normative-prose area (§5.2, §6.8, §6.9.1, §10.1, §10.2, §10.3, error-and-limits). Each area is small enough that A and B can run in parallel and reconcile in one cycle.
- **§6.8 directive semantics included.** The user's verbatim Phase 2 scope listed encoding, tag resolution, schemas, error semantics, and limits — `%YAML`/`%TAG` directive handling was implicit (it sits between encoding and tag resolution in the spec, and tag-handle resolution under `%TAG` directly gates Task 3's §6.9.1 audit). Including it explicitly avoids a follow-up "we missed directives" plan and the audit's coverage matches the user's intent for "thorough."
- **Phase 1 findings feed Phase 2 scope.** The "Phase 1 Findings" section is populated after Phase 1 reconciliation completes. If findings broaden scope (e.g., a Lenient §5.2-related BNF production surfaces an encoding-behavior gap), the additions land in that section and may add tasks.
- **Schema-resolution areas split across three tasks.** Failsafe (§10.1), JSON (§10.2), and Core (§10.3) each get their own task. They share methodology but cover disjoint type tables; bundling would inflate one task to a size that complicates reconciliation.
- **Error semantics and limits combined.** Both are implementation-defined-with-documented-choice areas; combining keeps the task count reasonable and the reconciliation focused on consistency-of-choice rather than per-error correctness.
- **§10.4–10.5 (output presentation) out of scope.** They apply to the formatter (`rlsp-yaml/`), not the parser. A separate formatter conformance plan would address these if needed.
- **Lead investigates unresolvable discrepancies.** Same as Phase 1 — lead reads the cited code and re-runs the test independently, records own verdict, tags `[NEEDS USER REVIEW]` for items requiring user adjudication.
- **No fixes during the audit.** Verdicts and follow-up entries only. Closing gaps happens in separate plans the user prioritizes.
- **`Indeterminate` requirements do not generate follow-up entries.** `Indeterminate` means the auditor could not verdict; the gap direction (lenient vs strict vs non-conformant) is unknown, so a fix-shaped follow-up entry would be premature. Indeterminate items are surfaced in the summary's dedicated list (section 6) for user awareness; the user can convert any of them into a follow-up if they want further investigation. This matches the Goal: the Goal promises follow-up entries for `Lenient` and `Non-conformant` only, plus visibility for `Indeterminate`.
- **Output committed as findings land.** Per-area commits during Tasks 1–7; summary + follow-ups commit during Task 8.
- **Audit output files use YAML frontmatter; this is an intentional exception to plan-format.md.** The plan-format guide forbids YAML frontmatter on plan files because plans are runtime artifacts that other agents read in markdown form. Audit output files have a different role — they are findings records that may be consumed by future tooling (e.g., a doc-rewrite plan ingesting verdict tables) — and machine-readable provenance metadata (plan path, phase, side, section, date) makes that consumption tractable. The exception is scoped to audit files only; this plan itself follows the no-frontmatter rule.
- **Cross-phase follow-up consolidation is deferred to a separate user-directed step.** Phase 1 and Phase 2 each append entries to `project_followup_plans.md` independently with per-plan deduplication. After both phases land, the combined list may have entries from two independent sessions that share a code area but differ in spec section. Reviewing the combined list for grouping, supersession, or prioritization is a user-directed cleanup step — not part of either phase's task scope. The user can ask for a consolidation pass after both summaries are committed; until then, both phase summaries remain authoritative on their own scope.

## Non-Goals

- Implementing fixes for any conformance gap surfaced by the audit. Fixes are separate plans the user prioritizes.
- Auditing the formatter (`rlsp-yaml/`) or any code outside `rlsp-yaml-parser/`.
- Auditing BNF productions (Phase 1 covers these).
- Auditing §10.4–10.5 (output presentation — formatter, not parser).
- Auditing test infrastructure or test coverage. Test gaps surfaced during the audit get filed as follow-up entries; test additions are separate plans.
- Updating the existing `yaml-spec-conformance.md` to add normative-prose entries. The doc rewrite (consolidating Phase 1 + Phase 2 findings) is a separate follow-up plan after both phases are user-adjudicated.
- Adding behavioral test fixtures to the parser's permanent test corpus. Throwaway tests written during the audit are deleted; permanent test additions are separate plans.
