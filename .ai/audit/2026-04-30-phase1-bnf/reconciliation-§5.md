---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: Reconciliation
section: §5 (with §3, §4)
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §3, §4, §5

64 entries reconciled. 62 entries have identical verdicts from Auditor A and Auditor B; 2 entries had disagreements that the lead resolved. No entries flagged `[NEEDS USER REVIEW]`.

## Final Verdict Tally

- `Strict-conformant`: 56
- `Stricter-than-spec`: 3
- `Lenient`: 3
- `Not-applicable`: 2
- `Non-conformant`: 0
- `Indeterminate`: 0

## Agreed Verdicts (62 entries)

| Production | Verdict |
|---|---|
| [§3] Not Applicable (descriptive) | Not-applicable |
| [§4] Not Applicable (meta-notation) | Not-applicable |
| [1] c-printable | Lenient |
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
| [20] c-directive | Strict-conformant |
| [21] c-reserved | Strict-conformant |
| [22] c-indicator | Strict-conformant |
| [23] c-flow-indicator | Strict-conformant |
| [24] b-line-feed | Strict-conformant |
| [25] b-carriage-return | Strict-conformant |
| [26] b-char | Strict-conformant |
| [27] nb-char | Lenient |
| [28] b-break | Strict-conformant |
| [29] b-as-line-feed | Strict-conformant |
| [30] b-non-content | Strict-conformant |
| [31] s-space | Strict-conformant |
| [32] s-tab | Strict-conformant |
| [33] s-white | Strict-conformant |
| [34] ns-char | Lenient |
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

## Resolved Disagreements

### [19] c-double-quote

**A's verdict:** Stricter-than-spec
**A's reasoning (summarized):** Beyond recognizing `"` as the indicator, the parser adds three security policies on the resulting double-quoted scalar: rejection of hex escapes that produce non-c-printable characters, rejection of bidi-control characters from numeric escapes, and a 1 MiB scalar length cap. A bundles these into [19] as overall double-quote-scalar strictness.

**B's verdict:** Strict-conformant
**B's reasoning (summarized):** The BNF for [19] is `c-double-quote ::= '"'` — a single character. Recognition of the opening/closing `"` is per spec at `lexer/quoted.rs:178-242`. The escape-related strictness belongs to other productions ([59]/[60]/[61]) and the 1 MiB cap is a non-BNF limit, not part of [19].

**Lead's investigation:** The BNF makes B's interpretation textually correct: production [19] is the indicator character only. Bundling escape-decoder strictness into [19] would double-count rejection that is also captured by [59]/[60]/[61] (numeric-escape printability) and would conflate a non-BNF resource limit (1 MiB scalar cap) with a BNF production. The strict scope of the production is the character recognition.

**Lead's verdict:** Strict-conformant

**Lead's reasoning:** [19] is a single-character indicator production. The parser correctly recognizes `"` at the dispatch site. Behavior on the resulting scalar (escape decoding, length capping, body parsing) belongs to other productions and to non-BNF resource limits. Strictness is attributed to the production where the rule is enforced; double-counting at the parent level obscures the audit. Sub-productions [59], [60], [61] retain their `Stricter-than-spec` verdicts; the 1 MiB scalar cap surfaces in Phase 2's limits audit.

### [62] c-ns-esc-char

**A's verdict:** Stricter-than-spec
**A's reasoning (summarized):** The alternation of all 20 escape sub-productions is implemented exactly. However, the printability and bidi-control gates inherited from [59]/[60]/[61] reject some escapes that the strict spec [62] alternation would admit. A treats the inherited strictness as making the union as a whole stricter than spec.

**B's verdict:** Strict-conformant
**B's reasoning (summarized):** Production [62] is the dispatch union — does the parser correctly implement all 20 alternates? `decode_escape` at `chars.rs:173-199` is the single dispatch point, invoked exclusively from the double-quoted scanner; single-quoted and block-scalar code paths do not invoke it. The alternation structure matches the BNF. The strictness on numeric escapes is captured in [59]/[60]/[61], where the rules actually live.

**Lead's investigation:** This is the same granularity question as [19]. Production [62] is a parent production composing 20 alternates including [59]/[60]/[61]. The parser's dispatch correctly enumerates all 20; the strictness on three of them ([59]/[60]/[61]) is captured in those sub-productions. Marking [62] Stricter-than-spec double-counts strictness already attributed to [59]/[60]/[61]. The principle: attribute strictness to the production where the rule is enforced, mark parent productions as Strict-conformant when they correctly compose strict sub-productions.

**Lead's verdict:** Strict-conformant

**Lead's reasoning:** The dispatch correctly handles all 20 alternates. Sub-productions [59], [60], [61] retain `Stricter-than-spec` and capture the printability/bidi rejections. Marking [62] as Strict-conformant keeps each strictness attributable to its source production. Future readers consulting the audit can locate where the rejection lives without traversing the entire escape-production graph.

## Doc Errata (informational — propagates to final summary)

These are findings where the audit verdict disagrees with the conformance doc's claim. Both auditors agreed on the audit verdict (no inter-auditor disagreement); the disagreement is between the audit and the doc. Listed here for the final summary's doc-correction section.

- **[1] c-printable** — Doc says `Conformant`. Audit says `Lenient`. Doc's Implementation citation (`chars.rs:14-26`) is only the predicate definition; no call site enforces it on literal stream characters (only escape-decoded characters are gated). Already filed as `6f0ec6d` in `project_followup_plans.md`.
- **[27] nb-char** — Doc says `Conformant`. Audit says `Lenient`. The line-splitter half (non-break recognition) is correct, but `nb-char ⊆ c-printable` is unenforced for the same reason as [1] — propagates from [1]'s gap.
- **[34] ns-char** — Doc says `Conformant`. Audit says `Lenient`. Predicate is partially used (plain-scalar starter, unrecognised-line first-char) but not enforced on plain-scalar bodies or quoted-scalar bodies — same root cause as [1].
- **[29] b-as-line-feed** — Doc cites `encoding.rs:179-197` (`normalize_line_breaks`) as the implementation. Audit B's grep finds that function is unused in production. Audit verdict remains `Strict-conformant` because the structural pathway (LineBuffer discards terminator + lexer inserts `'\n'`) achieves the spec MUST, but the doc's citation is incorrect.

## Methodology Notes

- A operated without conformance-doc access and produced 64 entries with verdicts that match B's on 62 entries. The methodology's independence held — A independently flagged the same `[1]`/`[27]`/`[34]` Lenient findings via direct code reading.
- B used the conformance doc as a reference but verified each claim against the code. B's findings include the doc errata above, demonstrating that B did not anchor to doc claims (the explicit anti-bias instruction worked).
- The two disagreements ([19], [62]) are granularity calls, not factual disagreements. The lead's resolution applies a consistent principle (strictness at the production where the rule is enforced) that disambiguates similar cases in later chapters.
