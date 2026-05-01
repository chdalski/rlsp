---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: Reconciliation
section: §10.2
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §10.2 JSON Schema

Both auditors enumerated 13 requirements covering the 4 JSON regex tables (null, bool, int, float), quoted-scalar handling, and edge cases. Tally: A had 12 SC + 1 Stricter-than-spec; B had 13 SC. The single disagreement is on the `-0` resolution case — both observed the **same behavior** (`-0` resolves to `!!float`, not `!!int`) but applied different verdict labels reflecting a real spec-internal inconsistency.

## Final Verdict Tally

- `Strict-conformant`: 13
- `Stricter-than-spec`: 0
- `Lenient`: 0
- `Not-applicable`: 0
- `Non-conformant`: 0
- `Indeterminate`: 0
- **Total: 13 reconciled requirements**

## Resolved Disagreement

### `-0` resolves to `!!float` (not `!!int`)

**A's verdict:** Stricter-than-spec. Reasoning: §10.2 spec example at line 6601 shows `-0` resolving to integer `0`; the parser's literal regex implementation rejects `-0` from int and accepts it as float. The spec's worked example contradicts the parser's outcome.

**B's verdict:** Strict-conformant. Reasoning: the literal §10.2 regex table at line 6578 specifies the int production as `0 | -? [1-9] [0-9]*` — the `0` arm carries no sign, so `-0` does not match. The parser correctly applies the regex; the example is an illustrative discrepancy in the spec, not a normative requirement.

**Lead's investigation:** This is a spec-internal inconsistency between the regex (line 6578) and the worked example (line 6601). In YAML spec convention, the BNF/regex is the formal normative rule; examples are illustrative. Per the literal regex:

- Int regex: `0 | -? [1-9] [0-9]*`
  - `-0` → does NOT match. The `0` alternative has no sign; the `-? [1-9] [0-9]*` alternative requires the integer part to begin with `[1-9]`, not `0`.
- Float regex: `-? ( 0 | [1-9] [0-9]* ) ( \. [0-9]* )? ( [eE] [-+]? [0-9]+ )?`
  - `-0` → matches via `-? ( 0 )`. The float regex permits a sign on the `0` arm.

The asymmetric handling of the sign in the int vs float regex is intentional in the spec text. The parser implements this exactly: `is_json_int` rejects `-0` (no sign on the `0` alternative), and `is_json_float` accepts it. The spec's worked example showing `-0` as integer is inconsistent with the spec's own regex.

**Lead's verdict:** Strict-conformant. The parser conforms to the spec's normative regex. The spec's worked example is the inconsistency, not the parser's behavior. A's "Stricter-than-spec" framing is defensible if one treats the example as normative, but the standard reading is that BNF/regex is the formal spec.

**Doc errata note:** the spec's internal inconsistency between the int regex and the worked example is worth surfacing in the doc-rewrite plan as a "Spec ambiguity / errata observed" note. The conformance doc rewrite should explain that the parser follows the regex literally and explicitly notes the divergence from the worked example, so future readers don't re-litigate the question.

## Reconciled Requirement Table

The 13 unified requirements both auditors covered, with final verdicts:

| # | Topic | Final verdict | Notes |
|---|---|---|---|
| 1 | Tag set: Failsafe + null + bool + int + float | Strict-conformant | Both agree |
| 2 | Null regex: `null` only (case-sensitive) | Strict-conformant | Both agree; `Null`/`NULL`/`~`/empty all unresolved |
| 3 | Bool regex: `true \| false` only (case-sensitive) | Strict-conformant | Both agree; `True`/`TRUE`/`yes`/`On` all unresolved |
| 4 | Int regex: `0 \| -? [1-9] [0-9]*` | Strict-conformant | Both agree on positive, negative, zero; both agree leading zeros (`007`, `01`) unresolved |
| 5 | Float regex: `-? ( 0 \| [1-9] [0-9]* ) ( \. [0-9]* )? ( [eE] [-+]? [0-9]+ )?` plus `.inf`/`.nan` | Strict-conformant | Both agree; `0.`, `1.`, `3.14`, `1e2`, `-1.5e+10` all match |
| 6 | `+0`, `+42`, `+12.3` (leading `+`) | Strict-conformant | Both agree: unresolved (no `+` sign in JSON regex) |
| 7 | Octal/hex (`0o7`, `0x3A`) | Strict-conformant | Both agree: unresolved (no octal/hex in JSON) |
| 8 | `.inf`, `-.inf`, `.nan` (special floats) | Strict-conformant | Both agree: unresolved under JSON (these are Core-only); JSON regex requires int part `0\|[1-9][0-9]*` |
| 9 | `-0` resolution | Strict-conformant | Lead-resolved disagreement; parser correctly follows literal int regex |
| 10 | Quoted scalars (single + double) → `!!str` | Strict-conformant | Both agree; quoting overrides regex matching |
| 11 | Plain scalars not matching any regex → strict-mode error | Strict-conformant | Both agree; parser returns `UnresolvedScalar` |
| 12 | Empty implicit scalars (`key:`, empty list items) → strict-mode error | Strict-conformant | Both agree; empty plain scalar doesn't match `null` regex; `UnresolvedScalar` |
| 13 | Schema selection per-loader (`Schema::Json`) | Strict-conformant | Both agree |

## Implementation Highlights

- **JSON regex implementation at `schema.rs:240-260`** (approximate; both auditors cite this region):
  - `is_json_int`: rejects `-0`, leading zeros, `+`-sign integers — exact regex match.
  - `is_json_float`: permits `-0`, `0.`, `1.`, exponents with `[-+]?` sign — exact regex match.
  - `is_json_bool`: case-sensitive `true`/`false` only.
  - `is_json_null`: case-sensitive `null` only.
- **`UnresolvedScalar` error path** at `loader.rs` (both auditors cite ~`loader.rs:120-128`): strict-mode rejection of plain scalars that match no regex.
- **Quoted scalar override**: the resolver bypasses regex matching for `ScalarStyle::SingleQuoted` and `ScalarStyle::DoubleQuoted`, always assigning `!!str`.
- **Foreign-tag passthrough**: explicit tags (`!!int`, `!!float`, etc.) bypass schema resolution and pass through unchanged. Same as §10.1 Failsafe behavior.

## Architectural Findings

None new beyond §10.1's. The JSON schema is correctly implemented as a strict regex-match overlay on Failsafe.

## Spec Errata Observed (for doc-rewrite plan)

**§10.2 spec internal inconsistency between int regex and worked example for `-0`:**

- Spec line 6578 (regex): `0 | -? [1-9] [0-9]*` — `-0` does NOT match.
- Spec line 6601 (example): "Resolved: `-0` → integer `0`" — implies `-0` IS an integer.

The conformance doc rewrite should note this and document that the parser follows the regex literally (resolving `-0` → `!!float`), and that this matches the de-facto behavior of mature JSON-schema YAML parsers.

## Disposition for Phase 2 Summary (Task 8)

**No follow-up entries to file from §10.2.** All 13 requirements are conformant after reconciliation.

The spec-errata observation about `-0` is a doc-rewrite concern (preserve the analysis in `yaml-spec-conformance.md` with citation to spec lines 6578 and 6601 and the parser's literal-regex implementation choice).

## Methodology Notes

- §10.2 was substantially more complex than §10.1 (13 requirements vs 8) but reconciled cleanly. The single disagreement was a spec-interpretation question, not a behavioral disagreement — both auditors observed the same parser output.
- Both auditors correctly tested the negative cases (uppercase variants, leading zeros, octal/hex, `+`-sign integers) — these would have been easy to miss but are critical for verifying the strict regex match.
- B's REQ-12 (empty implicit scalars resolve to `UnresolvedScalar`) is a useful behavioral point that A also covered. JSON schema's strict-mode rejection of empty implicit scalars is a real edge case for tooling that emits implicit `key:` mappings.
- Probe cleanup held: both auditors used `/tmp/audit-probe-§10.2-{a,b}/` outside the git tree; final `git status` clean.
- Next: §10.3 Core schema. Core extends JSON with broader recognition (Y/yes/Yes/YES variants, hex `0x`, octal `0o`, special floats). Expect more requirements per area and possibly more disagreements on edge cases.
