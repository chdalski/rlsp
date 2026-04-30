---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: Reconciliation
section: §6
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §6

41 entries reconciled. 32 entries had identical verdicts from Auditor A and Auditor B; 9 entries had disagreements that the lead resolved. No entries flagged `[NEEDS USER REVIEW]`.

## Final Verdict Tally

- `Strict-conformant`: 31
- `Stricter-than-spec`: 2
- `Lenient`: 8
- `Not-applicable`: 0
- `Non-conformant`: 0
- `Indeterminate`: 0

The Lenient count (8) is notably higher than §5's (3) and reflects multiple gaps in directive-name validation, tag-prefix validation, and shorthand-tag suffix enforcement that the conformance doc currently mislabels as "Conformant."

## Agreed Verdicts (32 entries)

| Production | Verdict |
|---|---|
| [64] s-indent-less-than(n) | Strict-conformant |
| [65] s-indent-less-or-equal(n) | Strict-conformant |
| [66] s-separate-in-line | Strict-conformant |
| [67] s-line-prefix(n,c) | Strict-conformant |
| [68] s-block-line-prefix(n) | Strict-conformant |
| [70] l-empty(n,c) | Strict-conformant |
| [71] b-l-trimmed(n,c) | Strict-conformant |
| [72] b-as-space | Strict-conformant |
| [73] b-l-folded(n,c) | Strict-conformant |
| [76] b-comment | Strict-conformant |
| [78] l-comment | Strict-conformant |
| [79] s-l-comments | Strict-conformant |
| [80] s-separate(n,c) | Strict-conformant |
| [81] s-separate-lines(n) | Strict-conformant |
| [82] l-directive | Strict-conformant |
| [84] ns-directive-name | Lenient |
| [85] ns-directive-parameter | Lenient |
| [88] ns-tag-directive | Strict-conformant |
| [89] c-tag-handle | Strict-conformant |
| [90] c-primary-tag-handle | Strict-conformant |
| [91] c-secondary-tag-handle | Strict-conformant |
| [92] c-named-tag-handle | Strict-conformant |
| [93] ns-tag-prefix | Lenient |
| [94] c-ns-local-tag-prefix | Lenient |
| [95] ns-global-tag-prefix | Lenient |
| [96] c-ns-properties(n,c) | Strict-conformant |
| [97] c-ns-tag-property | Strict-conformant |
| [98] c-verbatim-tag | Strict-conformant |
| [100] c-non-specific-tag | Strict-conformant |
| [101] c-ns-anchor-property | Strict-conformant |
| [102] ns-anchor-char | Strict-conformant |
| [103] ns-anchor-name | Strict-conformant |

## Resolved Disagreements

### [63] s-indent(n)

**A's verdict:** Stricter-than-spec — the hard rejection of tab-led lines exceeds what the BNF mechanically requires.
**B's verdict:** Strict-conformant — the BNF + §6.1 prose ("tabs must not be used in indentation") together require tab rejection; the implementation matches.

**Lead's investigation:** The BNF defines `s-indent(n)` as `n` space characters. The §6.1 prose normatively requires that tabs MUST NOT be used in indentation. The implementation counts only leading spaces (tabs yield indent=0) and rejects content lines starting with a tab. Both behaviors are spec-mandated; this is not strictness beyond spec but conformance to spec's normative wording.

**Lead's verdict:** Strict-conformant.

### [69] s-flow-line-prefix(n)

**A's verdict:** Strict-conformant — `trim_start_matches([' ', '\t'])` strips both spaces and tabs in one pass, matching `s-indent(n) s-separate-in-line?`.
**B's verdict:** Lenient — the trim does not enforce that the FIRST n bytes are spaces; a continuation line with leading tabs is accepted as if tabs counted toward indentation.

**Lead's investigation:** The BNF requires the prefix to be `s-indent(n)` (n SPACE characters per [63]) followed by optional `s-separate-in-line` (whitespace including tabs). A line beginning with `\t\t` does not match `s-indent(2)` because the first two characters are not spaces; the BNF permits tabs only in the optional separation portion. The implementation's combined trim accepts tabs at any position in the leading whitespace, including positions where the BNF requires spaces. Inputs the spec rejects pass through.

**Lead's verdict:** Lenient.

### [74] s-flow-folded(n)

**A's verdict:** Strict-conformant — the [74]-specific code (trim trailing whitespace, strip leading whitespace, fold to space) matches §6.5's "discard preceding/following spaces, then fold all breaks."
**B's verdict:** Lenient — composes [69] which is Lenient; the leniency propagates.

**Lead's investigation:** This is the same propagation question as §5's [62]. The §5 reconciliation principle is "attribute strictness/leniency to the production where the rule is enforced; parent productions that correctly compose are Strict-conformant." [74]'s own contribution (trim + fold) is correct; the leniency lives in [69]. Marking [74] Lenient would imply a fix needed at [74] when in fact fixing [69] resolves [74]'s effective behavior.

**Lead's verdict:** Strict-conformant. The reasoning section of [74] (in the agreed/disagreed-resolved record) notes that composition with [69] inherits [69]'s leniency until [69] is fixed.

### [75] c-nb-comment-text

**A's verdict:** Strict-conformant — comment body slice contains everything after `#` up to the line break; matches `c-comment nb-char*` since `LineBuffer` excludes the line terminator.
**B's verdict:** Lenient — `nb-char` excludes BOM (`\u{FEFF}`); the implementation does not strip BOM occurrences from comment bodies. Same root as [27]'s leniency.

**Lead's investigation:** `nb-char` is defined as `c-printable - b-char - c-byte-order-mark`. The implementation's slice excludes line breaks (`b-char`) by construction (LineBuffer splits on them), but does not exclude BOM or non-c-printable bytes. A literal BOM in a comment body is retained verbatim. This is a [75]-specific manifestation of the [1]/[27]/[34] root cause: the c-printable / nb-char predicates are defined but not enforced on slice content.

**Lead's verdict:** Lenient. Same root cause as [1] c-printable; fixing [1]'s enforcement gap will propagate here.

### [77] s-b-comment

**A's verdict:** Strict-conformant — `extract_trailing_comment` enforces preceded-by-whitespace at `plain.rs:523`.
**B's verdict:** Lenient — `lexer.rs:354-381` (`handle_plain_scalar_inline`) and `event_iter/directives.rs:126-133` accept `#` after content with no whitespace separator.

**Lead's investigation:** B's evidence cites `handle_plain_scalar_inline` in `lexer.rs:354-381`, where `residual.starts_with('#')` permits `#` directly. But the residual is what remains AFTER `scan_plain_line_block` has consumed the plain scalar. The plain-scanner's documented behavior — verified by test `hash_without_preceding_space_is_content("a#b", "a#b")` at `plain.rs:566` — treats `#` mid-scalar without preceding whitespace as part of the scalar, not a comment indicator. So `--- key#comment` produces a scalar `"key#comment"` with empty residual, not `key` + `#comment`. B's analysis is incorrect for this code path. The directive-residual case at `directives.rs:126-133` is a directive-line context, where the spec's "comments must be separated from other tokens" applies to content tokens — directives are line-structured and treat the optional trailing `#` correctly. The standalone-comment case at `comment.rs:30-31` does not apply because standalone comment lines have no preceding token on the same line.

**Lead's verdict:** Strict-conformant. A is correct; B's evidence misreads the plain-scanner behavior.

### [83] ns-reserved-directive

**A's verdict:** Lenient — the parser ignores unknown directives without parsing their parameters or validating against `ns-directive-name (s-separate-in-line ns-directive-parameter)*`.
**B's verdict:** Strict-conformant — the spec uses "should ignore … with appropriate warning"; "should" allows the warning to be omitted, and the body of unknown directives is opaque under "ignore."

**Lead's investigation:** A's specific concern is body-shape validation: inputs like `%FOO bad\x00content` would pass without parameter-shape rejection. But this is a manifestation of [1] c-printable's leniency on literal non-printables, not a [83]-specific gap. The [83] production describes the structure of directive bodies for parsing; for unknown directives the parser is licensed by "ignore" to skip body-content interpretation. The strict body-validation A wants is not specified by the spec for unknown directives.

**Lead's verdict:** Strict-conformant. The non-printable-in-body concern is captured at [1]; [83]'s ignore behavior is spec-permitted.

### [86] ns-yaml-directive

**A's verdict:** Strict-conformant — duplicate-rejection per §6.8.1, version-parsing as decimal digits, major-not-1 rejection per spec.
**B's verdict:** Stricter-than-spec — `major != 1` rejects `major == 0` (e.g., `%YAML 0.5`); the spec only mandates rejection for higher major versions.

**Lead's investigation:** Verified at `event_iter/directives.rs:146`: `if major != 1` rejects both `major == 0` and `major >= 2`. The §6.8.1 spec requires rejection of higher major versions but is silent on `major == 0`. Rejecting `major == 0` is reasonable conservatism but stricter than spec.

**Lead's verdict:** Stricter-than-spec. Rationale: defensive rejection of `major == 0` (no defined YAML 0.x exists), conservatism rather than conformance gap.

### [87] ns-yaml-version

**A's verdict:** Stricter-than-spec — `parse::<u8>` bounds digit values to [0, 255]; the BNF `ns-dec-digit+` admits arbitrary-length digit sequences.
**B's verdict:** Strict-conformant — split on `.`, parse each side as `u8`, correctly enforces "one or more decimal digits."

**Lead's investigation:** The BNF `ns-dec-digit+` admits any number of decimal digits. The implementation rejects values exceeding 255 because of the `u8` parse. A version like `%YAML 1.300` would fail the minor-version parse. This is stricter than the BNF requires, even though no realistic YAML version exceeds u8 range. B's "practically no version exceeds 255.999" rationale notes the practical case but does not rebut the BNF-strictness analysis.

**Lead's verdict:** Stricter-than-spec. Rationale: the digit-count limit comes from `u8::from_str` rather than spec; pragmatic, not a conformance gap.

### [99] c-ns-shorthand-tag

**A's verdict:** Strict-conformant — the suffix scanner enforces `ns-tag-char` exclusions; empty-suffix cases fall through to other branches.
**B's verdict:** Lenient — `properties.rs:170-176` explicitly comments "`!!` alone with no suffix is valid (empty suffix shorthand)"; named-handle branch accepts `!handle!` with zero-byte suffix.

**Lead's investigation:** The BNF `c-ns-shorthand-tag ::= c-tag-handle ns-tag-char+` requires a NON-EMPTY suffix (the `+` is one-or-more). The implementation's primary-handle branch documents acceptance of empty suffix, and the named-handle branch's `end = i + 1; end += scan_tag_suffix(...)` returns the bare `!handle!` form when the suffix scan returns zero bytes. The spec considers `!!` and `!handle!` (without suffix) malformed shorthand tags. The implementation accepts spec-rejected input.

**Lead's verdict:** Lenient.

## Doc Errata (informational — propagates to final summary)

These are findings where the audit's reconciled verdict disagrees with the conformance doc's claim. Listed for the final summary's doc-correction section.

- **[69] s-flow-line-prefix(n)** — Doc says `Conformant`. Audit says `Lenient`. Trim accepts tabs in the indent portion; spec requires spaces in `s-indent(n)`.
- **[75] c-nb-comment-text** — Doc says `Conformant`. Audit says `Lenient`. Same root as [1]: nb-char/c-printable predicates not enforced on slice content.
- **[84] ns-directive-name** — Doc says `Conformant`. Both auditors say `Lenient`. Directive name not validated against `ns-char+`.
- **[85] ns-directive-parameter** — Doc says `Conformant`. Both auditors say `Lenient`. Parameter blob not validated against `ns-char+`.
- **[93] ns-tag-prefix** — Doc says `Conformant`. Both auditors say `Lenient`. Prefix validation rejects only ASCII control chars + DEL, not the full `ns-uri-char` constraints.
- **[94] c-ns-local-tag-prefix** — Doc says `Conformant`. Both auditors say `Lenient`. Same root as [93].
- **[95] ns-global-tag-prefix** — Doc says `Conformant`. Both auditors say `Lenient`. Same root as [93]; doc itself notes the leniency in adjacent prose yet labels "Conformant."
- **[99] c-ns-shorthand-tag** — Doc says `Conformant`. Audit says `Lenient`. Implementation explicitly accepts empty suffixes (`!!` and `!handle!`).

## Methodology Notes

- A operated without conformance-doc access and produced 41 verdicts; B operated with the doc and produced verdicts that diverged from A on 9 entries. The divergences split: 2 were B-correct (B caught issues A missed: [86] major-0 rejection, [99] empty-suffix); 4 were A-correct (A caught issues B missed: [63] tab-rejection-is-conformant, [74] propagation principle, [77] plain-scanner correctness, [87] u8-digit-limit-is-stricter, [83] body-validation belongs to [1]); 3 had both auditors partially right with the lead applying the §5 reconciliation principle ([69], [75], related [74]).
- The dual-track methodology continues to work: B's doc-bias was mitigated by the anti-bias instruction (B independently flagged [86] which the doc labels Strict; B did not anchor to the doc's claim).
- Disagreement count (9 of 41) is higher than §5's (2 of 64). The §6 chapter has more behavioral nuance (indentation, comments, directives, tags) than §5's character productions; future chapters with similar shape (especially §7 flow style, which has 58 entries with significant interaction) may exhibit similar disagreement rates.
