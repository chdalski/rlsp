# YAML 1.2.2 Conformance — rlsp-yaml-parser

This directory documents the conformance status of `rlsp-yaml-parser` against the
[YAML 1.2.2 specification](https://yaml.org/spec/1.2.2/). It replaces the previous
single-file `docs/yaml-spec-conformance.md` with a structured presentation that
reflects all Phase 1 (BNF) and Phase 2 (normative prose) audit findings, user design
decisions, and the fixes delivered across the conformance implementation plans.

## Contents

| File | Covers |
|------|--------|
| `README.md` (this file) | Methodology, taxonomy, summary counts, production index |
| `bnf-§5.md` | §5 Character productions [1]–[62] |
| `bnf-§6.md` | §6 Structural productions [63]–[103] |
| `bnf-§7.md` | §7 Flow style productions [104]–[161] |
| `bnf-§8.md` | §8 Block style productions [162]–[201] |
| `bnf-§9.md` | §9 Document stream productions [202]–[211] |
| `prose.md` | Phase 2 normative prose findings (§5.2, §6.8, §6.9.1, §10.1–§10.3, error semantics) |
| `design-decisions.md` | Stricter-than-spec rationales and BNF-trace analyses |

---

## Methodology

### Audit Scope

This conformance record covers `rlsp-yaml-parser` — the parser crate only. It is a
parser-only, documentation-only audit. Out of scope: `rlsp-yaml` (language server and
formatter), `rlsp-fmt` (generic pretty-printer), YAML 1.1 compatibility, and
remediation design.

Two phases were conducted:

| Phase | Coverage | Method | Entries |
|-------|----------|--------|---------|
| Phase 1 — BNF | §3–§9 numbered productions | Grammar-level: match each BNF production against the implementation's lexer/parser logic | 213 |
| Phase 2 — Prose | §5.2, §6.8, §6.9.1, §10.1–§10.3, error semantics | Behavioral: construct inputs, run through parser, compare observed output to spec expectation | 130 |

### Dual-Track Methodology

Each chapter or area was audited independently by two subagents (A and B) with separate
dispatch prompts and no access to each other's work during the audit. The lead then
reconciled disagreements. This design surfaces findings that a single-pass audit misses:
Phase 1's c-printable enforcement gap (root cause of 4 of 11 original Lenient findings)
and 8 Lenient productions in §6 that the prior conformance doc labeled "Conformant" were
both found through cross-coverage.

### Symmetric Reconciliation Principle

Disagreements about where to attribute strictness or leniency — whether to the parent
production or the child that enforces the rule — were resolved by the principle:
*attribute the verdict to the production where the rule is enforced*. Applied
consistently from §7 onward, inter-auditor disagreement rates fell from 22% (§6) to
2.5% (§8) and 0% (§9).

### Reference Specification

- **Canonical:** <https://yaml.org/spec/1.2.2/>
- **Cached copy:** `.ai/references/yaml-1.2.2-spec.md`
  (fetched 2026-04-21; 211 productions [1]–[211] across §5–§9; §10 uses prose tables)

---

## Verdict Taxonomy

| Verdict | Meaning |
|---------|---------|
| `Strict-conformant` | The parser's behavior matches what the spec requires or permits. |
| `Stricter-than-spec` | The parser rejects input the spec admits. The deviation is intentional and documented with a rationale. |
| `Not-applicable` | The entry has no normative obligation on the implementation (descriptive prose or meta-notation for the grammar itself). |

### No Lenient Entries Remain

The audit initially identified 28 Lenient findings (11 Phase 1 BNF + 17 Phase 2 prose).
"Lenient" means the parser accepts input the spec rejects. All 28 were addressed:

- **Phase 1 fixed (11):** [1] c-printable, [27] nb-char, [34] ns-char,
  [69] s-flow-line-prefix(n), [75] c-nb-comment-text, [84] ns-directive-name,
  [85] ns-directive-parameter, [93] ns-tag-prefix, [94] c-ns-local-tag-prefix,
  [95] ns-global-tag-prefix, [99] c-ns-shorthand-tag.
- **Phase 2 fixed (7):** L1 (double BOM), L4 (%TAG comment-after-prefix),
  L5 (verbatim tag admissibility), L6 (verbatim tag separator),
  L8 (post-concatenation tag URI), L9 (signed octal int), L10 (signed hex int).
- **Phase 2 remaining as Lenient or Non-conformant:** NC1 (BOM-less UTF-32 detection),
  L2/L3 (NUL in directives, deduped), L7 (empty shorthand suffix, deduped),
  L11 (1 MiB cap bypass), L12–L17 (error-position imprecision) — these were not in scope
  for the current fix batch and remain open in the follow-up queue.

The `Lenient` label is documented here for historical reference; no entries in the
production tables below carry it.

### Non-conformant Finding

One real defect exists (not covered by the current fix batch):

- **NC1 — BOM-less UTF-32 detection arms missing (§5.2):** `detect_encoding()` in
  `encoding.rs` implements 7 of the spec's 9 encoding-detection table rows. The two
  BOM-less UTF-32 arms are missing; BOM-less UTF-32-BE input is misclassified as UTF-8.
  Tracked in the follow-up queue.

---

## Summary Verdict Counts

### Phase 1 — BNF (213 entries)

After applying fixes, the final tally:

| Verdict | Count |
|---------|-------|
| Strict-conformant | 205 |
| Stricter-than-spec | 5 |
| Not-applicable | 3 |
| **Total** | **213** |

Notes:
- The 11 originally-Lenient productions are now Strict-conformant after fixes.
- §3 and §4 contribute 2 Not-applicable entries (descriptive prose and meta-notation).
- [136] `in-flow(n,c)` is Not-applicable (meta-notational context-mapping rule).

### Phase 2 — Prose (130 entries)

After applying fixes to L1, L4, L5, L6, L8, L9, L10:

| Verdict | Count |
|---------|-------|
| Strict-conformant | 114 |
| Stricter-than-spec | 4 |
| Lenient (unfixed) | 10 |
| Non-conformant (unfixed) | 1 |
| Indeterminate (re-verdicted SC) | 1 |
| **Total** | **130** |

Per-area breakdown:

| Area | SC | ST | Len | NC | Indet | Total |
|------|----|----|-----|----|-------|-------|
| §5.2 Character Encodings | 9 | 0 | 0 | 1 | 0 | 10 |
| §6.8 Directives | 24 | 3 | 2 | 0 | 0 | 29 |
| §6.9.1 Tag Resolution | 26 | 0 | 1 | 0 | 1 | 28 |
| §10.1 Failsafe Schema | 8 | 0 | 0 | 0 | 0 | 8 |
| §10.2 JSON Schema | 13 | 0 | 0 | 0 | 0 | 13 |
| §10.3 Core Schema | 18 | 1 | 0 | 0 | 0 | 19 |
| Error semantics + limits | 16 | 0 | 7 | 0 | 0 | 23 |
| **Total** | **114** | **4** | **10** | **1** | **1** | **130** |

---

## BNF Production Index

One line per production. Verdict reflects post-fix status. Click the section file links
for full per-entry reasoning, spec quotes, implementation citations, and test coverage.

### §3 and §4 (Descriptive Prose)

| Production | Verdict | Detail |
|------------|---------|--------|
| [§3] Processes and Models (descriptive) | Not-applicable | [bnf-§5.md](bnf-§5.md) |
| [§4] Meta-notation | Not-applicable | [bnf-§5.md](bnf-§5.md) |

### §5 Character Productions ([1]–[62]) → [bnf-§5.md](bnf-§5.md)

| Production | Verdict |
|------------|---------|
| [1] c-printable | Strict-conformant |
| [2] nb-json | Strict-conformant |
| [3] c-byte-order-mark | Strict-conformant |
| [4] c-sequence-entry | Strict-conformant |
| [5] c-mapping-key | Strict-conformant |
| [6] c-mapping-value | Strict-conformant |
| [7] c-collect-entry | Strict-conformant |
| [8] c-sequence-start | Strict-conformant |
| [9] c-sequence-end | Strict-conformant |
| [10] c-mapping-start | Strict-conformant |
| [11] c-mapping-end | Strict-conformant |
| [12] c-comment | Strict-conformant |
| [13] c-anchor | Strict-conformant |
| [14] c-alias | Strict-conformant |
| [15] c-tag | Strict-conformant |
| [16] c-literal | Strict-conformant |
| [17] c-folded | Strict-conformant |
| [18] c-single-quote | Strict-conformant |
| [19] c-double-quote | Strict-conformant |
| [20] c-directive | Strict-conformant |
| [21] c-reserved | Strict-conformant |
| [22] c-indicator | Strict-conformant |
| [23] c-flow-indicator | Strict-conformant |
| [24] b-line-feed | Strict-conformant |
| [25] b-carriage-return | Strict-conformant |
| [26] b-char | Strict-conformant |
| [27] nb-char | Strict-conformant |
| [28] b-break | Strict-conformant |
| [29] b-as-line-feed | Strict-conformant |
| [30] b-non-content | Strict-conformant |
| [31] s-space | Strict-conformant |
| [32] s-tab | Strict-conformant |
| [33] s-white | Strict-conformant |
| [34] ns-char | Strict-conformant |
| [35] ns-dec-digit | Strict-conformant |
| [36] ns-hex-digit | Strict-conformant |
| [37] ns-ascii-letter | Strict-conformant |
| [38] ns-word-char | Strict-conformant |
| [39] ns-uri-char | Strict-conformant |
| [40] ns-tag-char | Strict-conformant |
| [41] c-escape | Strict-conformant |
| [42] ns-esc-null | Strict-conformant |
| [43] ns-esc-bell | Strict-conformant |
| [44] ns-esc-backspace | Strict-conformant |
| [45] ns-esc-horizontal-tab | Strict-conformant |
| [46] ns-esc-line-feed | Strict-conformant |
| [47] ns-esc-vertical-tab | Strict-conformant |
| [48] ns-esc-form-feed | Strict-conformant |
| [49] ns-esc-carriage-return | Strict-conformant |
| [50] ns-esc-escape | Strict-conformant |
| [51] ns-esc-space | Strict-conformant |
| [52] ns-esc-double-quote | Strict-conformant |
| [53] ns-esc-slash | Strict-conformant |
| [54] ns-esc-backslash | Strict-conformant |
| [55] ns-esc-next-line | Strict-conformant |
| [56] ns-esc-non-breaking-space | Strict-conformant |
| [57] ns-esc-line-separator | Strict-conformant |
| [58] ns-esc-paragraph-separator | Strict-conformant |
| [59] ns-esc-8-bit | Stricter-than-spec |
| [60] ns-esc-16-bit | Stricter-than-spec |
| [61] ns-esc-32-bit | Stricter-than-spec |
| [62] c-ns-esc-char | Strict-conformant |

### §6 Structural Productions ([63]–[103]) → [bnf-§6.md](bnf-§6.md)

| Production | Verdict |
|------------|---------|
| [63] s-indent(n) | Strict-conformant |
| [64] s-indent-less-than(n) | Strict-conformant |
| [65] s-indent-less-or-equal(n) | Strict-conformant |
| [66] s-separate-in-line | Strict-conformant |
| [67] s-line-prefix(n,c) | Strict-conformant |
| [68] s-block-line-prefix(n) | Strict-conformant |
| [69] s-flow-line-prefix(n) | Strict-conformant |
| [70] l-empty(n,c) | Strict-conformant |
| [71] b-l-trimmed(n,c) | Strict-conformant |
| [72] b-as-space | Strict-conformant |
| [73] b-l-folded(n,c) | Strict-conformant |
| [74] s-flow-folded(n) | Strict-conformant |
| [75] c-nb-comment-text | Strict-conformant |
| [76] b-comment | Strict-conformant |
| [77] s-b-comment | Strict-conformant |
| [78] l-comment | Strict-conformant |
| [79] s-l-comments | Strict-conformant |
| [80] s-separate(n,c) | Strict-conformant |
| [81] s-separate-lines(n) | Strict-conformant |
| [82] l-directive | Strict-conformant |
| [83] ns-reserved-directive | Strict-conformant |
| [84] ns-directive-name | Strict-conformant |
| [85] ns-directive-parameter | Strict-conformant |
| [86] ns-yaml-directive | Stricter-than-spec |
| [87] ns-yaml-version | Stricter-than-spec |
| [88] ns-tag-directive | Strict-conformant |
| [89] c-tag-handle | Strict-conformant |
| [90] c-primary-tag-handle | Strict-conformant |
| [91] c-secondary-tag-handle | Strict-conformant |
| [92] c-named-tag-handle | Strict-conformant |
| [93] ns-tag-prefix | Strict-conformant |
| [94] c-ns-local-tag-prefix | Strict-conformant |
| [95] ns-global-tag-prefix | Strict-conformant |
| [96] c-ns-properties(n,c) | Strict-conformant |
| [97] c-ns-tag-property | Strict-conformant |
| [98] c-verbatim-tag | Strict-conformant |
| [99] c-ns-shorthand-tag | Strict-conformant |
| [100] c-non-specific-tag | Strict-conformant |
| [101] c-ns-anchor-property | Strict-conformant |
| [102] ns-anchor-char | Strict-conformant |
| [103] ns-anchor-name | Strict-conformant |

### §7 Flow Style Productions ([104]–[161]) → [bnf-§7.md](bnf-§7.md)

| Production | Verdict |
|------------|---------|
| [104] c-ns-alias-node | Strict-conformant |
| [105] e-scalar | Strict-conformant |
| [106] e-node | Strict-conformant |
| [107] nb-double-char | Strict-conformant |
| [108] ns-double-char | Strict-conformant |
| [109] c-double-quoted(n,c) | Strict-conformant |
| [110] nb-double-text(n,c) | Strict-conformant |
| [111] nb-double-one-line | Strict-conformant |
| [112] s-double-escaped(n) | Strict-conformant |
| [113] s-double-break(n) | Strict-conformant |
| [114] nb-ns-double-in-line | Strict-conformant |
| [115] s-double-next-line(n) | Strict-conformant |
| [116] nb-double-multi-line(n) | Strict-conformant |
| [117] c-quoted-quote | Strict-conformant |
| [118] nb-single-char | Strict-conformant |
| [119] ns-single-char | Strict-conformant |
| [120] c-single-quoted(n,c) | Strict-conformant |
| [121] nb-single-text(n,c) | Strict-conformant |
| [122] nb-single-one-line | Strict-conformant |
| [123] nb-ns-single-in-line | Strict-conformant |
| [124] s-single-next-line(n) | Strict-conformant |
| [125] nb-single-multi-line(n) | Strict-conformant |
| [126] ns-plain-first(c) | Strict-conformant |
| [127] ns-plain-safe(c) | Strict-conformant |
| [128] ns-plain-safe-out | Strict-conformant |
| [129] ns-plain-safe-in | Strict-conformant |
| [130] ns-plain-char(c) | Strict-conformant |
| [131] ns-plain(n,c) | Strict-conformant |
| [132] nb-ns-plain-in-line(c) | Strict-conformant |
| [133] ns-plain-one-line(c) | Strict-conformant |
| [134] s-ns-plain-next-line(n,c) | Strict-conformant |
| [135] ns-plain-multi-line(n,c) | Strict-conformant |
| [136] in-flow(n,c) | Not-applicable |
| [137] c-flow-sequence(n,c) | Strict-conformant |
| [138] ns-s-flow-seq-entries(n,c) | Strict-conformant |
| [139] ns-flow-seq-entry(n,c) | Strict-conformant |
| [140] c-flow-mapping(n,c) | Strict-conformant |
| [141] ns-s-flow-map-entries(n,c) | Strict-conformant |
| [142] ns-flow-map-entry(n,c) | Strict-conformant |
| [143] ns-flow-map-explicit-entry(n,c) | Strict-conformant |
| [144] ns-flow-map-implicit-entry(n,c) | Strict-conformant |
| [145] ns-flow-map-yaml-key-entry(n,c) | Strict-conformant |
| [146] c-ns-flow-map-empty-key-entry(n,c) | Strict-conformant |
| [147] c-ns-flow-map-separate-value(n,c) | Strict-conformant |
| [148] c-ns-flow-map-json-key-entry(n,c) | Strict-conformant |
| [149] c-ns-flow-map-adjacent-value(n,c) | Strict-conformant |
| [150] ns-flow-pair(n,c) | Strict-conformant |
| [151] ns-flow-pair-entry(n,c) | Strict-conformant |
| [152] ns-flow-pair-yaml-key-entry(n,c) | Strict-conformant |
| [153] c-ns-flow-pair-json-key-entry(n,c) | Strict-conformant |
| [154] ns-s-implicit-yaml-key(c) | Strict-conformant |
| [155] c-s-implicit-json-key(c) | Strict-conformant |
| [156] ns-flow-yaml-content(n,c) | Strict-conformant |
| [157] c-flow-json-content(n,c) | Strict-conformant |
| [158] ns-flow-content(n,c) | Strict-conformant |
| [159] ns-flow-yaml-node(n,c) | Strict-conformant |
| [160] c-flow-json-node(n,c) | Strict-conformant |
| [161] ns-flow-node(n,c) | Strict-conformant |

### §8 Block Style Productions ([162]–[201]) → [bnf-§8.md](bnf-§8.md)

All 40 entries are `Strict-conformant`. See [bnf-§8.md](bnf-§8.md) for per-entry detail.

| Production | Verdict |
|------------|---------|
| [162] c-b-block-header(t) | Strict-conformant |
| [163] c-indentation-indicator | Strict-conformant |
| [164] c-chomping-indicator(t) | Strict-conformant |
| [165] b-chomped-last(t) | Strict-conformant |
| [166] l-chomped-empty(n,t) | Strict-conformant |
| [167] l-strip-empty(n) | Strict-conformant |
| [168] l-keep-empty(n) | Strict-conformant |
| [169] l-trail-comments(n) | Strict-conformant |
| [170] c-l+literal(n) | Strict-conformant |
| [171] l-nb-literal-text(n) | Strict-conformant |
| [172] b-nb-literal-next(n) | Strict-conformant |
| [173] l-literal-content(n,t) | Strict-conformant |
| [174] c-l+folded(n) | Strict-conformant |
| [175] s-nb-folded-text(n) | Strict-conformant |
| [176] l-nb-folded-lines(n) | Strict-conformant |
| [177] s-nb-spaced-text(n) | Strict-conformant |
| [178] b-l-spaced(n) | Strict-conformant |
| [179] l-nb-spaced-lines(n) | Strict-conformant |
| [180] l-nb-same-lines(n) | Strict-conformant |
| [181] l-nb-diff-lines(n) | Strict-conformant |
| [182] l-folded-content(n,t) | Strict-conformant |
| [183] l+block-sequence(n) | Strict-conformant |
| [184] c-l-block-seq-entry(n) | Strict-conformant |
| [185] s-l+block-indented(n,c) | Strict-conformant |
| [186] ns-l-compact-sequence(n) | Strict-conformant |
| [187] l+block-mapping(n) | Strict-conformant |
| [188] ns-l-block-map-entry(n) | Strict-conformant |
| [189] c-l-block-map-explicit-entry(n) | Strict-conformant |
| [190] c-l-block-map-explicit-key(n) | Strict-conformant |
| [191] l-block-map-explicit-value(n) | Strict-conformant |
| [192] ns-l-block-map-implicit-entry(n) | Strict-conformant |
| [193] ns-s-block-map-implicit-key | Strict-conformant |
| [194] c-l-block-map-implicit-value(n) | Strict-conformant |
| [195] ns-l-compact-mapping(n) | Strict-conformant |
| [196] s-l+block-node(n,c) | Strict-conformant |
| [197] s-l+flow-in-block(n) | Strict-conformant |
| [198] s-l+block-in-block(n,c) | Strict-conformant |
| [199] s-l+block-scalar(n,c) | Strict-conformant |
| [200] s-l+block-collection(n,c) | Strict-conformant |
| [201] seq-space(n,c) | Strict-conformant |

### §9 Document Stream Productions ([202]–[211]) → [bnf-§9.md](bnf-§9.md)

All 10 entries are `Strict-conformant`. Both auditors agreed on every verdict; no
disagreements. See [bnf-§9.md](bnf-§9.md) for per-entry detail.

| Production | Verdict |
|------------|---------|
| [202] l-document-prefix | Strict-conformant |
| [203] c-directives-end | Strict-conformant |
| [204] c-document-end | Strict-conformant |
| [205] l-document-suffix | Strict-conformant |
| [206] c-forbidden | Strict-conformant |
| [207] l-bare-document | Strict-conformant |
| [208] l-explicit-document | Strict-conformant |
| [209] l-directive-document | Strict-conformant |
| [210] l-any-document | Strict-conformant |
| [211] l-yaml-stream | Strict-conformant |

---

## Phase 2 Summary

Phase 2 covered 130 normative-prose requirements across 7 areas using the same
dual-track methodology (independent A and B subagents, behavioral testing, lead
reconciliation). Full per-requirement detail is in [prose.md](prose.md).

### Quick reference: Stricter-than-spec (Phase 2)

| ID | Area | Description |
|----|------|-------------|
| S1 | §6.8 | `%YAML` major-0 rejection (propagated from Phase 1 [86]; refined: only `major == 0` rejected) |
| S2 | §6.8 | `%YAML` u8 digit overflow — `parse::<u8>` limits digits to [0, 255] |
| S3 | §6.8 | `MAX_DIRECTIVES_PER_DOC = 64` hardcoded limit (spec has no limit) |
| S4 | §10.3 | Core leading-zero decimal rejection (`007`, `01` rejected; spec regex permits) |

### Quick reference: Open findings (not in current fix batch)

| ID | Area | Description | Severity |
|----|------|-------------|----------|
| NC1 | §5.2 | BOM-less UTF-32 detection arms missing | Non-conformant |
| L11 | Error limits | 1 MiB quoted-scalar cap bypassed on no-escape borrow path | Lenient, DoS-relevant |
| L12–L17 | Error limits | Error-position imprecision (6 cases) | Lenient |

---

## References

### Audit Source Files (Historical Records)

The audit files below are immutable historical records. Do not edit them to fix citation
drift — they document what the auditors found at audit time.

**Phase 1 BNF audit** (`.ai/audit/2026-04-30-phase1-bnf/`):

- `summary.md` — Phase 1 final verdict tally and verdict table
- `reconciliation-§5.md` — §5 reconciliation (64 entries)
- `reconciliation-§6.md` — §6 reconciliation (41 entries)
- `reconciliation-§7.md` — §7 reconciliation (58 entries); includes full BNF-trace for
  [110]/[121]/[131] multi-line implicit key resolution
- `reconciliation-§8.md` — §8 reconciliation (40 entries)
- `reconciliation-§9.md` — §9 reconciliation (10 entries)

**Phase 2 Prose audit** (`.ai/audit/2026-04-30-phase2-prose/`):

- `summary.md` — Phase 2 final verdict tally, per-area counts, and follow-up filing
- `reconciliation-§5.2.md` — Character encoding (10 entries)
- `reconciliation-§6.8.md` — Directives (29 entries)
- `reconciliation-§6.9.1.md` — Tag resolution (28 entries)
- `reconciliation-§10.1.md` — Failsafe schema (8 entries)
- `reconciliation-§10.2.md` — JSON schema (13 entries)
- `reconciliation-§10.3.md` — Core schema (19 entries)
- `reconciliation-error-and-limits.md` — Error semantics and resource limits (23 entries)
