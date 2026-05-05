**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-05

## Goal

Survey `rlsp-yaml-parser` in depth for testing-methodology gaps — spec tables incompletely exercised, behavioral invariants lacking property tests, and differential testing opportunities against reference implementations. Survey `rlsp-yaml` and `rlsp-fmt` at a lighter depth focused on their specific testing patterns (corpus invariants, formatter idempotency, printer algorithm coverage). Produce a survey document at `.ai/audit/2026-05-05-testing-methodology-gaps/summary.md` with one entry per gap.

## Context

- **Motivation:** Phase 2 §5.2 audit revealed the BOM-less UTF-32 detection gap was missed because no test covered that row of the encoding detection table and no property test asserted encoding invariance. The pattern likely repeats elsewhere.
- **Existing proptest usage:** Two locations in `rlsp-yaml-parser` — `tests/encoding.rs` (encoding-choice invariant) and `src/pos.rs` (column/advance properties). Zero proptest in `rlsp-yaml` or `rlsp-fmt`.
- **Spec-table dispatch points identified:**
  - `encoding.rs` `detect_encoding()` — §5.2 encoding detection (BOM prefixes + null-byte heuristics)
  - `schema.rs` `resolve_scalar()` / `resolve_collection()` — §10 tag resolution (failsafe/json/core)
  - `schema.rs` `is_core_int()` / `is_core_float()` — §10.2/§10.3 integer/float regex tables
  - `chars.rs` character predicates — §5 character set tables (c-printable, ns-char, etc.)
  - Tag resolution rules in `directive_scope.rs` — §6.9.1
- **Existing test infrastructure:**
  - 35 integration tests + 25 inline test modules in `rlsp-yaml-parser`
  - YAML Test Suite conformance (368/368)
  - `corpus_invariants.rs` in `rlsp-yaml` (I1-I10 invariants)
  - Formatter idempotency checks via `formatter_conformance.rs`
- **Output format:** Similar to conformance audit reconciliation files — structured entries with gap description, bug class, and fix recommendation.
- **Scope:** Research/documentation only. No code changes. No new tests written — this plan identifies gaps for future implementation plans to address.
- **Performance:** N/A — no code changes.

## Steps

- [ ] Audit spec-table coverage in rlsp-yaml-parser
- [ ] Identify behavioral invariant candidates for proptest
- [ ] Identify differential testing opportunities
- [ ] Write summary document
- [ ] Commit

## Tasks

### Task 1: Audit spec-table coverage and write summary

Survey all normative spec tables the implementation dispatches on. For each table, check whether every row/case is exercised by at least one positive test. Identify proptest candidates and differential testing opportunities. Write the output document.

- [ ] **§5.2 encoding detection table** — check `tests/encoding.rs` covers all 6 rows of the BOM detection table (UTF-8 no BOM, UTF-8 BOM, UTF-16LE BOM, UTF-16BE BOM, UTF-32LE BOM, UTF-32BE BOM) plus the null-byte heuristic fallbacks. Note which rows lack a fixture.
- [ ] **§5 character-set tables** — check that `chars.rs` predicates (`is_c_printable`, `is_ns_char`, `is_ns_uri_char_single`, `is_ns_tag_char_single`) have boundary tests at codepoint edges (e.g., U+0000 rejected, U+0020 at boundary, U+FFFE rejected, supplementary plane boundary). Note gaps.
- [ ] **§10.2/§10.3 integer/float regex tables** — check `tests/schema_resolution.rs` covers all regex rows: decimal `[1-9][0-9]*`, octal `0o[0-7]+`, hex `0x[0-9a-fA-F]+`, and the float patterns (inf, nan, scientific, fixed). Note edge cases not covered (e.g., `+0`, `-0`, maximum-length values).
- [ ] **§6.9.1 tag resolution rules** — check tag-resolution dispatch (default tag assignment by kind: mapping → `tag:yaml.org,2002:map`, sequence → `tag:yaml.org,2002:seq`, scalar with/without tag). Note uncovered rule branches.
- [ ] **§10.1 default-tag-by-kind** — check Failsafe/JSON/Core schema selection coverage. Note whether all three schemas are tested for all node kinds.
- [ ] **Proptest candidates** — identify behavioral invariants suitable for property-based testing:
  - Round-trip: `parse_events(input) → reconstruct → parse_events(output)` preserves event sequence
  - Character predicate consistency: for any char in U+0000..U+10FFFF, `is_ns_char(c)` implies `is_c_printable(c)` (subset relationships hold)
  - Schema resolution determinism: `resolve_scalar(s, schema)` is pure (same input → same tag)
  - Formatter idempotency: `format(format(input)) == format(input)` (already partially tested, expand to proptest)
  - Encoding detection determinism: encoding detection on any valid YAML byte sequence is idempotent
- [ ] **Differential testing candidates** — identify behaviors where comparing against libyaml/PyYAML/snakeyaml would catch bugs:
  - Full §5.2 encoding detection table against libyaml
  - Schema resolution (Core schema) for scalar values against PyYAML/snakeyaml
  - Event-stream output for YAML Test Suite cases vs libyaml events
- [ ] **rlsp-yaml testing gaps** — survey `corpus_invariants.rs` (I1-I10) for invariant coverage gaps, check formatter round-trip fixture completeness (does `formatter_conformance.rs` cover all YAML Test Suite categories?), check whether `code_action_fixtures` covers all code-action types. Note gaps.
- [ ] **rlsp-fmt testing gaps** — survey `printer.rs` unit tests for algorithm coverage gaps. Check whether edge cases are tested: zero-width groups, deeply nested groups, break-mode vs flat-mode transitions at width boundary, empty documents. Note gaps.
- [ ] Write `.ai/audit/2026-05-05-testing-methodology-gaps/summary.md` with one entry per gap. Each entry: gap ID, gap description, what bug class it would catch, severity (high/medium/low), specific fix recommendation (fixture/property/differential).
- [ ] Summary document exists at `.ai/audit/2026-05-05-testing-methodology-gaps/summary.md` and contains at least one entry per surveyed area (encoding, character predicates, schema resolution, tag rules, proptest candidates, differential candidates, rlsp-yaml gaps, rlsp-fmt gaps)
- [ ] Single commit: `docs(audit): add testing methodology gap survey`

## Decisions

- **Research only, no implementation.** This plan identifies gaps. Fixing them is separate work — each high-severity gap becomes its own implementation plan.
- **Depth varies by crate.** `rlsp-yaml-parser` gets the deepest survey (spec-table dispatch points, character predicates, schema resolution). `rlsp-yaml` and `rlsp-fmt` get targeted surveys of their specific testing patterns (corpus invariants, formatter round-trip, printer algorithm) — they have fewer spec-driven behaviors so the survey is lighter but explicit.
- **Output location:** `.ai/audit/` alongside the conformance audit files — this is an audit artifact, not user-facing documentation.
- **Gap severity:** High = would catch a real bug class (like the BOM-less UTF-32 gap); Medium = improves confidence but no known missed bug; Low = nice-to-have, existing tests likely cover indirectly.

## Non-Goals

- Writing new tests or adding proptest infrastructure (separate implementation plans).
- Auditing test quality (assertion strength, fixture realism) — this is coverage/methodology only.
- Performance testing gaps — performance measurement is the user's job out-of-band.
