# YAML 1.2.2 Conformance Audit — rlsp-yaml-parser

## Methodology

### Scope

This document is a **parser-only, documentation-only** audit of `rlsp-yaml-parser`
against the YAML 1.2.2 specification. Every numbered production in §3–§10 of the spec
is classified using the strict entry format defined below.

**Out of scope:**

- `rlsp-yaml` (language server + formatter) and `rlsp-fmt` (generic pretty-printer).
- Remediation of any finding. Findings are recorded here; remediation is a follow-up
  decision in separate plans.
- Downstream ramifications of hypothetical parser fixes. Those belong in each
  remediation plan's Context, not in this audit.
- Expanding beyond YAML 1.2.2. YAML 1.1 compatibility diagnostics are out of scope.

### Reference Specification

- **URL:** <https://yaml.org/spec/1.2.2/>
- **Cached copy:** `.ai/references/yaml-1.2.2-spec.md`
  (source: `https://raw.githubusercontent.com/yaml/yaml-spec/main/spec/1.2.2/spec.md`,
  fetched 2026-04-21, 211 productions [1]–[211] across §5–§9; §10 uses tables only)

All spec quotes in this document are verbatim from the cached copy.

### Strict Entry Format

Every production, regardless of classification, uses this format:

```
### [NNN] production-name

BNF: <exact BNF from the spec>

- Classification: Conformant | Lenient | Strict | Not Implemented | Not Applicable (descriptive) | Not Applicable (meta-notation)
- Spec (§X.Y): "<verbatim quote of the normative text>"
- Implementation: <crate>/<path>:<line-range>
- Test coverage: <yaml-test-suite case ID(s)> | <project test path> | no direct test
- Discrepancy: <one-sentence gap — Lenient/Strict only; omit for other classes>
```

For `Not Applicable` entries: the Spec quote is still required (it establishes that the
entry is descriptive / meta-notation); the Implementation and Test coverage fields carry
the explicit text `(no implementation obligation)`.

### Classification Decision Rules

| Spec says | Code does | Classification |
|-----------|-----------|----------------|
| requires X | does X | **Conformant** |
| requires X | does X **and also** Y (Y not permitted) | **Lenient** |
| permits X | rejects X | **Strict** |
| requires X | does not implement X | **Not Implemented** |
| entry has no normative obligation on the implementation (purely descriptive) | — | **Not Applicable (descriptive)** |
| entry is meta-notation for the grammar itself | — | **Not Applicable (meta-notation)** |

The classification is the output of applying these rules to the spec quote and the
implementation fact recorded in the entry. A classification that does not follow from
the recorded evidence is a reviewer-rejectable defect.

### Test-Coverage Conventions

- **yaml-test-suite case ID** — four-character identifier (e.g., `6CA3`) when a test
  case exercises the production. Multiple IDs allowed.
- **project test path** — `rlsp-yaml-parser/tests/<file>.rs` plus test function name,
  if the production is exercised by a project test.
- **no direct test** — valid only when neither of the above applies. An explicit
  "no direct test" is itself a data point (coverage gap); silent omission is not
  permitted.

---

## §3

<!-- Task 2: draft §3 + §4 + §5 entries -->

## §4

<!-- Task 2: draft §3 + §4 + §5 entries -->

## §5

<!-- Task 2: draft §3 + §4 + §5 entries -->

## §6

<!-- Task 4: draft §6 entries -->

## §7

<!-- Task 6: draft §7 entries -->

## §8

<!-- Task 8: draft §8 entries -->

## §9

<!-- Task 10: draft §9 entries -->

## §10

<!-- Task 12: draft §10 entries -->

## Summary

<!-- Task 13: append consolidated Summary table of all Lenient and Strict findings -->
