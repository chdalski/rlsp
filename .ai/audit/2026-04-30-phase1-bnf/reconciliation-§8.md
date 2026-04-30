---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: Reconciliation
section: §8
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §8

40 entries reconciled. 39 entries had identical verdicts from Auditor A and Auditor B; 1 entry had a disagreement that the lead resolved.

## Final Verdict Tally

- `Strict-conformant`: 40
- `Stricter-than-spec`: 0
- `Lenient`: 0
- `Not-applicable`: 0
- `Non-conformant`: 0
- `Indeterminate`: 0

§8 is the cleanest chapter audited so far. The block-style implementation is consistently disciplined — both auditors independently noted the comprehensive header rejection rules, the §8.1.1.1 over-indented-leading-blank-line enforcement on both literal and folded paths, the Unicode-correct 1024-char implicit-key limit, and the BLOCK-IN/BLOCK-OUT split for `seq-space`.

## Agreed Verdicts (39 entries)

| Production | Verdict |
|---|---|
| [162] c-b-block-header(t) | Strict-conformant |
| [163] c-indentation-indicator | Strict-conformant |
| [164] c-chomping-indicator(t) | Strict-conformant |
| [165] b-chomped-last(t) | Strict-conformant |
| [166] l-chomped-empty(n,t) | Strict-conformant |
| [167] l-strip-empty(n) | Strict-conformant |
| [168] l-keep-empty(n) | Strict-conformant |
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

## Resolved Disagreements

### [169] l-trail-comments(n)

**A's verdict:** Lenient — the parser does not enforce that the first trailing comment line must be at indent < n; comment lines at indent >= content_indent simply terminate the block scalar and are left for the outer parser, which is more permissive than the BNF specifies.

**B's verdict:** Strict-conformant — the block-scalar loop hands control back to the document-level dispatcher when a less-indented non-blank line appears; the BNF's `s-indent-less-than(n)` is enforced by the dedent-terminator branch firing only when `next.indent < content_indent`.

**Lead's investigation:** A's reasoning misreads the BNF. The production `l-trail-comments(n) ::= s-indent-less-than(n) c-nb-comment-text b-comment l-comment*` defines WHAT counts as a trailing-comment block, not WHAT MUST BE REJECTED. A line that does not satisfy `s-indent-less-than(n)` is simply not a trail-comment — it's something else (scalar content if at indent >= content_indent, or another structural element).

The implementation's behavior:
- A `#`-prefixed line at indent >= content_indent is part of the block scalar's content (correctly, since `#` mid-scalar without preceding whitespace is scalar content per §6.6 and the comment indicator binds to whitespace-preceded `#`).
- A `#`-prefixed line at indent < content_indent dedents the block scalar; control returns to the document-level dispatcher; the comment scanner processes it.

Both cases are correct per spec. The first trailing comment, when it exists, is at indent < content_indent by construction (the dedent-terminator only fires when indent < content_indent). The "less indented" requirement IS the boundary the loop uses to exit. A's concern about "indented comment line at indent >= content_indent rejected as trail-comment" is misplaced — the spec doesn't require rejection of such lines; they're simply scalar content.

**Lead's verdict:** Strict-conformant.

## Doc Errata (informational — propagates to final summary)

None. B explicitly noted that no §8 production's enforcement disagrees with the conformance doc's claim; A's only disagreement was an internal interpretation issue, not a doc erratum.

## Methodology Notes

- §8 is the cleanest chapter so far (1 disagreement out of 40, 2.5%). Block-style productions have well-defined enforcement boundaries and the parser's implementation is consistently disciplined.
- The reconciliation principle was applied symmetrically by both auditors here — neither propagated leniency or strictness up the production tree. This suggests the principle, repeated in dispatch prompts since §6, has been internalized.
- The cumulative chapter audits show a clear pattern: the parser's grammar conformance is high (most productions Strict-conformant); the gaps are concentrated in (a) literal-non-printable enforcement at character productions [1]/[27]/[34] and propagated content slicing at [75], and (b) directive-name and tag-prefix validation at [84]/[85]/[93]/[94]/[95]/[99] in §6, plus the 3 `[NEEDS USER REVIEW]` items in §7.
