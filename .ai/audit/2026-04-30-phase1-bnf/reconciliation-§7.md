---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: Reconciliation
section: §7
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §7

58 entries reconciled. 51 entries had identical verdicts from Auditor A and Auditor B; 7 entries had disagreements. Three are flagged `[NEEDS USER REVIEW]` because the lead cannot resolve a §7.4.2-vs-§7.3.1 spec-interpretation question from code alone.

## Final Verdict Tally

- `Strict-conformant`: 57 (3 with `[NEEDS USER REVIEW]` tentative verdict)
- `Stricter-than-spec`: 0
- `Lenient`: 0
- `Not-applicable`: 1
- `Non-conformant`: 0
- `Indeterminate`: 0

§7's clean tally reflects that the parser's flow-style implementation is mature and well-tested. The most substantive open question is the implicit-key single-line restriction in flow mappings (`[NEEDS USER REVIEW]` items below).

## Agreed Verdicts (51 entries)

| Production | Verdict |
|---|---|
| [104] c-ns-alias-node | Strict-conformant |
| [105] e-scalar | Strict-conformant |
| [106] e-node | Strict-conformant |
| [108] ns-double-char | Strict-conformant |
| [109] c-double-quoted(n,c) | Strict-conformant |
| [112] s-double-escaped(n) | Strict-conformant |
| [113] s-double-break(n) | Strict-conformant |
| [114] nb-ns-double-in-line | Strict-conformant |
| [115] s-double-next-line(n) | Strict-conformant |
| [116] nb-double-multi-line(n) | Strict-conformant |
| [117] c-quoted-quote | Strict-conformant |
| [118] nb-single-char | Strict-conformant |
| [119] ns-single-char | Strict-conformant |
| [120] c-single-quoted(n,c) | Strict-conformant |
| [122] nb-single-one-line | Strict-conformant |
| [123] nb-ns-single-in-line | Strict-conformant |
| [124] s-single-next-line(n) | Strict-conformant |
| [125] nb-single-multi-line(n) | Strict-conformant |
| [126] ns-plain-first(c) | Strict-conformant |
| [127] ns-plain-safe(c) | Strict-conformant |
| [129] ns-plain-safe-in | Strict-conformant |
| [130] ns-plain-char(c) | Strict-conformant |
| [132] nb-ns-plain-in-line(c) | Strict-conformant |
| [133] ns-plain-one-line(c) | Strict-conformant |
| [134] s-ns-plain-next-line(n,c) | Strict-conformant |
| [135] ns-plain-multi-line(n,c) | Strict-conformant |
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

## Resolved Disagreements

### [107] nb-double-char

**A's verdict:** Strict-conformant
**B's verdict:** Stricter-than-spec — the hex-escape printability gate + bidi-control rejection (`lexer/quoted.rs:580-600`) reject input the spec admits.

**Lead's investigation:** The strictness B identifies lives at sub-productions [59], [60], [61] (the numeric-escape productions), which were already verdicted Stricter-than-spec in §5's reconciliation. [107] correctly composes these sub-productions; the strictness is captured at its source. Marking [107] Stricter-than-spec double-counts the same rejection. Per the §5 reconciliation principle (attribute strictness to the production where the rule is enforced), [107] is Strict-conformant.

**Lead's verdict:** Strict-conformant.

### [110] nb-double-text(n,c) — `[NEEDS USER REVIEW]`

**A's verdict:** Indeterminate — implicit-key single-line restriction not directly enforced by passing context to the lexer; whether every multi-line scalar used as an implicit key is rejected depends on `last_token_line` tracking; without a conformance-suite cross-check, the verdict cannot be reached on code reading alone.
**B's verdict:** Strict-conformant — surrounding parsers reject `:` separators across lines, restricting effective consumption to one line; composition matches the four-context dispatch.

**Lead's investigation:** The single-line restriction is implemented at `event_iter/flow.rs:1128`, which rejects implicit keys spanning multiple lines — but ONLY in flow sequences (`in_sequence` check). The same code, with comment "Flow mappings `{...}` allow multi-line implicit keys — see YAML 1.2 §7.4.2", explicitly does NOT enforce the restriction inside flow mappings. The spec at §7.3.1 says "Double-quoted scalars are restricted to a single line when contained inside an implicit key" — but §7.4.2 (flow mappings) may have a specific exception that overrides §7.3.1. Resolving this requires careful reading of §7.4.2's normative wording against §7.3.1's, which I cannot do conclusively from code.

**Lead's tentative verdict:** Strict-conformant — the implementation documents its spec interpretation citing §7.4.2; absent clear evidence of misinterpretation, accepting the implementation's reading.

**`[NEEDS USER REVIEW]`** — please verify whether §7.4.2 permits multi-line implicit keys in flow mappings (or whether §7.3.1's restriction applies regardless of context). If the latter, the implementation is Lenient at flow.rs:1124-1135 and a fix is required.

### [111] nb-double-one-line

**A's verdict:** Strict-conformant
**B's verdict:** Stricter-than-spec — hex-escape printability gate + bidi-control rejection.

**Lead's investigation:** Same root as [107] — strictness is at sub-productions [59]/[60]/[61], not at this composing production.

**Lead's verdict:** Strict-conformant.

### [121] nb-single-text(n,c) — `[NEEDS USER REVIEW]`

**A's verdict:** Indeterminate — same concern as [110] for single-quoted scalars.
**B's verdict:** Strict-conformant — same composed-enforcement reasoning as [110].

**Lead's investigation:** Same §7.4.2-vs-§7.3.1 ambiguity as [110]. Same code path at flow.rs:1128 enforces single-line in flow sequences but not flow mappings.

**Lead's tentative verdict:** Strict-conformant.

**`[NEEDS USER REVIEW]`** — same question as [110]. Resolution should match [110]'s.

### [128] ns-plain-safe-out

**A's verdict:** Lenient — leniency inherited from §5 [34] ns-char.
**B's verdict:** Strict-conformant — implementation correctly composes [34]; leniency is at [34]'s source, not propagated.

**Lead's investigation:** Per the §5 reconciliation principle (attribute leniency to the production where the rule is enforced), [128] is Strict-conformant. [34]'s leniency is captured in §5's reconciliation; propagating it up the tree creates noise without informational value.

**Lead's verdict:** Strict-conformant.

### [131] ns-plain(n,c) — `[NEEDS USER REVIEW]`

**A's verdict:** Indeterminate — same concern as [110] / [121] for plain scalars.
**B's verdict:** Strict-conformant — same composed-enforcement reasoning.

**Lead's investigation:** Same §7.4.2-vs-§7.3.1 ambiguity as [110] and [121]. Plain scalars in flow mappings can span multiple lines under the implementation's interpretation.

**Lead's tentative verdict:** Strict-conformant.

**`[NEEDS USER REVIEW]`** — same question as [110]. Resolution should match [110]'s.

### [136] in-flow(n,c)

**A's verdict:** Strict-conformant — propagation rule correctly observed; flow handler implements context propagation indirectly.
**B's verdict:** Not-applicable — `in-flow` is meta-notational; it only renames the outer context for the entries production and has no observable parser obligation distinct from the productions it forwards to.

**Lead's investigation:** The BNF for `in-flow(n,c)` is purely a context-mapping function: it maps outer-context labels to the inner context the entries production should use. It has no parsing semantics of its own — the parsing happens in the `ns-s-flow-seq-entries` production it forwards to. For an event-driven parser that does not materialize "context tokens" as distinct parse states, in-flow is indeed meta-notational. Consistent with §3 and §4's meta-entries which were verdicted Not-applicable.

**Lead's verdict:** Not-applicable.

## Doc Errata (informational — propagates to final summary)

The conformance doc disagreements surfaced by B that resolved as `Strict-conformant` after reconciliation:

- **[107] nb-double-char** — Doc says `Conformant`. B initially flagged Stricter-than-spec; after reconciliation principle applied, this is `Strict-conformant`. No erratum.
- **[111] nb-double-one-line** — Same as [107]. No erratum.

No doc errata were added by §7.

## Methodology Notes

- The dual-track methodology surfaced inconsistencies in EACH auditor's application of the reconciliation principle. A propagated leniency up ([128] from §5 [34]) but did not propagate strictness up ([107]/[111]). B propagated strictness up ([107]/[111]) but did not propagate leniency up ([128]). The lead applied the principle uniformly, resulting in 6 of 7 disagreements resolving to `Strict-conformant`.
- The implicit-key single-line restriction ([110]/[121]/[131]) is a real spec-interpretation question. Three `[NEEDS USER REVIEW]` flags require user adjudication — they may reveal a Lenient finding in flow-mapping handling that an additional code change would address.
- Disagreement count (7 of 58) is comparable to §6's (9 of 41 ≈ 22%); §7's rate is ~12%. As predicted in §6's methodology notes, disagreement rates decrease as auditors converge on the reconciliation principle, but spec-interpretation calls remain cases where the lead must investigate or escalate.
