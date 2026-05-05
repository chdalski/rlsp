**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-05

## Goal

Rewrite the YAML 1.2.2 conformance documentation to reflect all Phase 1 + Phase 2 audit findings, user design decisions, and the fixes delivered across 15 conformance plans. Replace the single 2119-line `yaml-spec-conformance.md` with a structured `docs/conformance/` subfolder. Add inline `///` doc comments at enforcement sites in the source. Create `rlsp-yaml-parser/CLAUDE.md` with a Conformance Sync section that instructs agents to keep code comments and conformance docs in sync.

## Context

- **Current state:** `docs/yaml-spec-conformance.md` is 2119 lines, single file, uses classifications that the audit found incorrect (11 Phase 1 entries mislabeled as "Conformant"), citation line numbers that have drifted, and no Phase 2 findings.
- **Audit sources:**
  - Phase 1 BNF: `.ai/audit/2026-04-30-phase1-bnf/` (5 reconciliation files + summary, 213 entries)
  - Phase 2 Prose: `.ai/audit/2026-04-30-phase2-prose/` (7 reconciliation files + summary)
  - BNF-trace analyses: `reconciliation-§7.md` entry [110] (§7.3.x vs §7.4.2 flow-key terminology trap)
- **Fixes delivered (all now landed on main):**
  - Phase 1: c-printable [1]/[27]/[34]/[75], directive ns-char [84]/[85], tag prefix+suffix [93]/[94]/[95]/[99], flow-line-prefix [69]
  - Phase 2: signed octal/hex [L9/L10], TAG comment [L4], verbatim admissibility+separator [L5/L6], post-concat URI [L8], error positions [L12-L17], double BOM [L1]
- **User design decisions (2026-05-05):**
  - MAX_DIRECTIVES_PER_DOC=64: keep as-is (DoS protection)
  - Leading-zero decimal rejection: keep (YAML 1.1 octal confusion + LSP diagnostic enablement)
- **Taxonomy:** Strict-conformant, Stricter-than-spec (with rationale), Formally-Accepted-Lenient (none remain — all were fixed), Not-applicable
- **Target structure:**
  ```
  docs/conformance/
  ├── README.md              # Overview, methodology, taxonomy, summary table
  ├── bnf-§5.md             # §5 Character productions
  ├── bnf-§6.md             # §6 Structural
  ├── bnf-§7.md             # §7 Flow/Block styles
  ├── bnf-§8.md             # §8 Block styles
  ├── bnf-§9.md             # §9 Document stream
  ├── prose.md              # Phase 2 normative prose findings
  └── design-decisions.md   # Stricter-than-spec rationales + BNF-traces
  ```
- **Code doc comments:** Add `///` doc comments at key enforcement sites (validation functions, rejection points) citing the spec production and rationale.
- **`rlsp-yaml-parser/CLAUDE.md`:** New file with a Conformance Sync section modeled on root CLAUDE.md's Settings Sync.
- **Performance:** Documentation-only plan. No runtime code changes. Zero performance impact.

## Steps

- [x] Create `docs/conformance/` folder structure with README
- [ ] Populate per-chapter BNF conformance entries
- [ ] Write prose findings and design decisions files
- [ ] Add `///` doc comments at enforcement sites
- [ ] Create `rlsp-yaml-parser/CLAUDE.md` with Conformance Sync section
- [ ] Delete old `docs/yaml-spec-conformance.md`
- [ ] Update root CLAUDE.md component table if needed
- [ ] Verify build passes (doc comments are compiled)
- [ ] Mark plan Completed and commit

## Tasks

### Task 1: Create `docs/conformance/` structure and README

**Completed:** commit `5cf6178` (2026-05-05)

Create the folder and write `README.md` with methodology, taxonomy, and summary table.

- [x] Create `docs/conformance/` directory
- [x] Write `README.md` with:
  - Methodology section (dual-track A/B independent audit, lead reconciliation, symmetric reconciliation principle)
  - Verdict taxonomy table (Strict-conformant, Stricter-than-spec, Not-applicable; note that no Lenient entries remain after fixes)
  - Summary verdict counts (Phase 1: 213 entries; Phase 2: per-section counts)
  - One-line-per-production summary table with verdict and link to detail file
  - References to audit source files (`.ai/audit/`) as historical records
- [x] `cargo fmt --check` passes (no code changes yet)

### Task 2: Populate per-chapter BNF entries (§5-§9)

Write the five per-chapter files with entries for all 213 BNF productions.

- [ ] Write `bnf-§5.md` — §5 character productions. For each entry: production number, BNF, verdict, implementation function name (not line number), test reference, and for non-trivial verdicts a one-paragraph rationale.
- [ ] Write `bnf-§6.md` — §6 structural productions. Include the formerly-Lenient entries ([69], [84], [85], [93]-[95], [99]) with updated verdicts (now Strict-conformant after fixes) and commit references.
- [ ] Write `bnf-§7.md` — §7 flow/block styles. Embed the §7.3.x-vs-§7.4.2 BNF-trace analysis verbatim from `reconciliation-§7.md` entry [110] for the flow-key terminology trap.
- [ ] Write `bnf-§8.md` — §8 block styles.
- [ ] Write `bnf-§9.md` — §9 document stream.
- [ ] Each file uses function-name citations (e.g., `scan_tag()` in `properties.rs`) NOT line numbers
- [ ] `cargo clippy --all-targets` passes (no code changes in this task)

### Task 3: Write prose findings and design decisions

Write `prose.md` (Phase 2 normative prose) and `design-decisions.md` (Stricter-than-spec rationales).

- [ ] Write `prose.md` covering Phase 2 audit areas: §5.2 encoding, §6.8 directives, §6.9.1 tags, §10.1-§10.3 schema, error-and-limits. For each requirement: the spec quote, the implementation verdict (now Strict-conformant after fixes), the fix commit reference.
- [ ] Write `design-decisions.md` with:
  - MAX_DIRECTIVES_PER_DOC=64 — rationale (DoS protection), user decision (keep), enforcement site
  - Leading-zero decimal rejection — rationale (YAML 1.1 octal confusion + LSP diagnostic), user decision (keep), enforcement site
  - Phase 1 Stricter entries: [59]/[60]/[61] (Trojan Source mitigation), [86] (major-0 rejection), [87] (u8 digit limit) — existing rationales from Phase 1 audit
  - The "should is non-mandatory" precedent ([83] ns-reserved-directive)
- [ ] Embed the §7 BNF-trace analysis in `design-decisions.md` (or cross-reference `bnf-§7.md` if already embedded there)
- [ ] Note the spec errata: §10.2 `-0` worked example contradicts int regex; parser follows regex
- [ ] `cargo fmt --check` passes

### Task 4: Add `///` doc comments at enforcement sites and create CLAUDE.md

Add concise `///` doc comments at key validation enforcement functions, and create `rlsp-yaml-parser/CLAUDE.md`.

- [ ] Add `///` doc comments at these enforcement sites (one-line spec citation + rationale):
  - `chars.rs`: `is_ns_char`, `is_c_printable`, `is_ns_uri_char_single`, `is_ns_tag_char_single`
  - `directives.rs`: ns-char pre-validation block, `validate_tag_prefix()`, tag-comment trailing-content check
  - `properties.rs`: verbatim admissibility check, empty-suffix rejection, `scan_tag_suffix()`
  - `directive_scope.rs`: `validate_resolved_tag()`, `resolve_tag()` post-concatenation check
  - `schema.rs`: `is_core_int()` sign-gate and leading-zero rejection
  - `lexer/quoted.rs`: `s-indent(n)` enforcement in continuation loops
  - `lines.rs`: `signal_document_boundary()` BOM-strip rationale
- [ ] Create `rlsp-yaml-parser/CLAUDE.md` with:
  - Brief crate description (streaming YAML 1.2.2 parser)
  - Conformance Sync section with table (source of truth, consumers, sync-when trigger)
  - Reference to `docs/conformance/README.md` for full conformance status
- [ ] Delete old `docs/yaml-spec-conformance.md` (replaced by `docs/conformance/`)
- [ ] Update `rlsp-yaml-parser/README.md` Documentation section: change link from `docs/yaml-spec-conformance.md` to `docs/conformance/README.md`
- [ ] Update `rlsp-yaml-parser/docs/feature-log.md` hex-escape security-hardening entry: change path reference from `docs/yaml-spec-conformance.md entries [59]–[61]` to `docs/conformance/bnf-§5.md entries [59]–[61]`
- [ ] Update root `CLAUDE.md` Components table path if needed (`docs/conformance/` replaces `docs/yaml-spec-conformance.md`)
- [ ] `cargo build -p rlsp-yaml-parser` passes (doc comments are compiled)
- [ ] `cargo clippy --all-targets` passes
- [ ] `cargo fmt --check` passes
- [ ] Remove the "Conformance doc rewrite to reflect audit findings" bullet from `project_followup_plans.md`; trim the "Post-Phase-2 conformance workflow" orchestration entry's step 5 (now satisfied) and note step 6 (public conformance declaration) remains as a future follow-up
- [ ] Single commit: `docs(rlsp-yaml-parser): rewrite conformance documentation`

## Decisions

- **Replace, don't patch.** The old doc has too many structural problems (wrong classifications, drifted citations, missing Phase 2) to patch incrementally. A clean rewrite from audit sources is faster and more reliable.
- **Function-name citations, not line numbers.** Line numbers drift on every change. Function names are stable identifiers that `grep` can resolve. Example: "`validate_tag_prefix()` in `directives.rs`" not "`directives.rs:268`".
- **BNF-trace analyses embedded in detail files, not README.** The README is a summary/index. Detailed analyses live in the per-chapter or design-decisions files where the context is.
- **Single commit for the full rewrite.** The conformance doc subfolder is one coherent artifact — splitting it across commits adds partial states that are individually incomplete. One commit: delete old, write new, update references.
- **`rlsp-yaml-parser/CLAUDE.md` is minimal.** It has the sync table and a crate description. It does NOT duplicate the root CLAUDE.md's build commands or workspace conventions — those inherit via workspace CLAUDE.md.
- **No `Lenient` or `Formally-Accepted-Lenient` entries remain.** All 18 Lenient findings (11 Phase 1 + 7 Phase 2) were fixed. The taxonomy section documents the category for historical reference, but no entries use it.
- **Performance: N/A.** Documentation-only plan.

## Non-Goals

- **Expanding conformance beyond §3-§10.** The YAML spec has informational appendices and examples — those are not in scope for the conformance audit.
- **Fixing citation drift in the audit files.** The `.ai/audit/` files are immutable historical records. Citation drift ([29], [105]) is recorded as errata in the new doc but the audit files themselves are not edited.
- **Updating `feature-log.md`.** The conformance doc rewrite is not a user-facing feature — it's internal documentation.
