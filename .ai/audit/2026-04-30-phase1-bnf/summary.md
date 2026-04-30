---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: Summary
section: all
date: 2026-04-30
produced-by: lead
---

# Phase 1 Conformance Audit Summary

213 entries audited across YAML 1.2.2 spec chapters §3–§9 using a dual-track methodology (independent A and B subagents per chapter, lead reconciliation). Results are committed in `.ai/audit/2026-04-30-phase1-bnf/`. Corresponding follow-up entries are filed in `.ai/memory/project_followup_plans.md`.

The 3 `[NEEDS USER REVIEW]` flags initially raised on §7's `[110]`, `[121]`, `[131]` were finalized as `Strict-conformant` after a detailed BNF-trace of §7.3.x prose against §7.4.2 grammar (resolved 2026-04-30 — see `reconciliation-§7.md`'s entry for [110]).

## Final Verdict Tally

| Verdict | Count | Percentage |
|---|---|---|
| `Strict-conformant` | 194 | 91.1% |
| `Stricter-than-spec` | 5 | 2.3% |
| `Lenient` | 11 | 5.2% |
| `Not-applicable` | 3 | 1.4% |
| `Non-conformant` | 0 | 0.0% |
| `Indeterminate` | 0 | 0.0% |
| **Total** | **213** | **100%** |

Per-chapter inter-auditor disagreement rates: §5 (3.1%), §6 (22.0%), §7 (12.1%), §8 (2.5%), §9 (0.0%). Most disagreements were granularity calls about strictness/leniency propagation between parent and child productions; the lead applied a symmetric reconciliation principle ("attribute strictness/leniency to the production where the rule is enforced") consistently.

## Verdict Table

### §3 + §4 + §5 (Character Productions, 64 entries)

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
| [19] c-double-quote | Strict-conformant |
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
| [42]–[58] ns-esc-* (named escapes) | Strict-conformant |
| [59] ns-esc-8-bit | Stricter-than-spec |
| [60] ns-esc-16-bit | Stricter-than-spec |
| [61] ns-esc-32-bit | Stricter-than-spec |
| [62] c-ns-esc-char | Strict-conformant |

(Detailed entries [42]–[58] are all Strict-conformant; collapsed for readability. Full per-entry reasoning is in `reconciliation-§5.md`.)

### §6 (Structural Productions, 41 entries)

| Production | Verdict |
|---|---|
| [63] s-indent(n) | Strict-conformant |
| [64] s-indent-less-than(n) | Strict-conformant |
| [65] s-indent-less-or-equal(n) | Strict-conformant |
| [66] s-separate-in-line | Strict-conformant |
| [67] s-line-prefix(n,c) | Strict-conformant |
| [68] s-block-line-prefix(n) | Strict-conformant |
| [69] s-flow-line-prefix(n) | Lenient |
| [70] l-empty(n,c) | Strict-conformant |
| [71] b-l-trimmed(n,c) | Strict-conformant |
| [72] b-as-space | Strict-conformant |
| [73] b-l-folded(n,c) | Strict-conformant |
| [74] s-flow-folded(n) | Strict-conformant |
| [75] c-nb-comment-text | Lenient |
| [76] b-comment | Strict-conformant |
| [77] s-b-comment | Strict-conformant |
| [78] l-comment | Strict-conformant |
| [79] s-l-comments | Strict-conformant |
| [80] s-separate(n,c) | Strict-conformant |
| [81] s-separate-lines(n) | Strict-conformant |
| [82] l-directive | Strict-conformant |
| [83] ns-reserved-directive | Strict-conformant |
| [84] ns-directive-name | Lenient |
| [85] ns-directive-parameter | Lenient |
| [86] ns-yaml-directive | Stricter-than-spec |
| [87] ns-yaml-version | Stricter-than-spec |
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
| [99] c-ns-shorthand-tag | Lenient |
| [100] c-non-specific-tag | Strict-conformant |
| [101] c-ns-anchor-property | Strict-conformant |
| [102] ns-anchor-char | Strict-conformant |
| [103] ns-anchor-name | Strict-conformant |

### §7 (Flow Style Productions, 58 entries)

All 58 entries are `Strict-conformant` except `[136] in-flow(n,c)` (`Not-applicable` — meta-notational context-mapping rule). Three entries (`[110] nb-double-text(n,c)`, `[121] nb-single-text(n,c)`, `[131] ns-plain(n,c)`) were initially flagged `[NEEDS USER REVIEW]` for a §7.3.x-vs-§7.4.2 spec-interpretation question; the question was resolved on 2026-04-30 by tracing the BNF context propagation in detail (see `reconciliation-§7.md`'s [110] entry). All three are now final `Strict-conformant`.

(Full per-entry reasoning in `reconciliation-§7.md`.)

### §8 (Block Style Productions, 40 entries)

All 40 entries (`[162]`–`[201]`) are `Strict-conformant`. §8 was the cleanest chapter: 1 inter-auditor disagreement at `[169] l-trail-comments(n)`, resolved as `Strict-conformant` after lead investigation. (Full per-entry reasoning in `reconciliation-§8.md`.)

### §9 (Document Stream Productions, 10 entries)

All 10 entries (`[202]`–`[211]`) are `Strict-conformant`. Both auditors agreed on every verdict; no disagreements. (Full per-entry reasoning in `reconciliation-§9.md`.)

## Lenient Productions (11 entries)

These productions accept input the spec rejects. Each generates a follow-up entry unless deduplicated against an existing one.

### Character predicate enforcement gaps (root cause: predicates defined but not applied to literal stream characters)

- **[1] c-printable (§5.1)** — Predicate at `chars.rs:14-26` is defined but only enforced at numeric-escape decoder (`lexer/quoted.rs:580`); literal non-printables pass through silently in plain/quoted/block scalars and comments. **Fix summary:** add c-printable enforcement to scalar/comment lexers OR document the leniency in the conformance doc as an intentional implementation choice. *Already filed at commit `6f0ec6d`.*

- **[27] nb-char (§5.4)** — `nb-char ⊆ c-printable` is unenforced for the same root-cause reason as [1]. **Fix summary:** propagates from [1] — enforcing c-printable at content slicing fixes this.

- **[34] ns-char (§5.5)** — Predicate is partially used (plain-scalar starter, unrecognized-line first-char) but not enforced on plain-scalar bodies or quoted-scalar bodies. **Fix summary:** apply ns-char predicate to scalar body bytes during lexing, or document leniency.

- **[75] c-nb-comment-text (§6.6)** — Comment body slice does not exclude BOM (carries forward from [27]'s gap). **Fix summary:** strip BOM from comment body slices, or document leniency. Propagates from [27].

### Directive validation gaps (§6.8, root cause: directive name/parameter blobs not validated)

- **[84] ns-directive-name + [85] ns-directive-parameter (§6.8)** — Name/parameter blobs are not validated against `ns-char+`. The parser accepts `%FOO bad\x00content` because the body is opaque. **Fix summary:** validate directive name and parameter against `ns-char+`, OR retain current laxity for unknown directives (which the spec licenses with "should ignore"). Decision required.

### Tag prefix validation gaps (§6.9.1, root cause: prefix validation rejects only ASCII control + DEL)

- **[93] ns-tag-prefix + [94] c-ns-local-tag-prefix + [95] ns-global-tag-prefix (§6.9.1)** — Prefix validation rejects only ASCII control characters and DEL, not the full `ns-uri-char` / `ns-tag-char` constraints. The conformance doc itself notes the leniency at one citation but labels these "Conformant" inconsistently. **Fix summary:** validate prefix bytes against `ns-uri-char` per the spec.

### Structural enforcement gaps

- **[69] s-flow-line-prefix(n) (§6.3)** — Trim-based prefix stripping does not enforce the n-space indent portion separately from the optional separation-in-line tail. A continuation line with leading tabs is accepted as if tabs counted toward indent. **Fix summary:** distinguish indent (n spaces required) from separation (whitespace allowed) in the continuation prefix logic.

- **[99] c-ns-shorthand-tag (§6.9.1)** — Implementation explicitly accepts empty suffixes (`!!` and `!handle!`), violating `ns-tag-char+`. The code includes a comment documenting this acceptance. **Fix summary:** require non-empty suffix per the spec, or document the deviation as an intentional implementation choice.

## Stricter-than-spec Productions (5 entries)

These productions reject input the spec admits. All are intentional security-hardening choices, preserved as-is — no follow-up filing.

- **[59] ns-esc-8-bit, [60] ns-esc-16-bit, [61] ns-esc-32-bit (§5.7)** — Numeric escape sequences (`\x##`, `\u####`, `\U########`) are rejected when their decoded character is non-c-printable, and additionally rejected when in the bidi-control range. *Rationale:* security hardening against obfuscation via numeric escapes (Trojan Source mitigation). The spec permits these escapes; the parser does not.

- **[86] ns-yaml-directive (§6.8.1)** — Rejects `major == 0` (e.g., `%YAML 0.5`) in addition to the spec-mandated `major >= 2` rejection. *Rationale:* defensive conservatism (no defined YAML 0.x version exists).

- **[87] ns-yaml-version (§6.8.1)** — `parse::<u8>` bounds digit values to [0, 255], rejecting arbitrary-length sequences the BNF `ns-dec-digit+` admits. *Rationale:* practical limit (no realistic YAML version exceeds 255.999); not a conformance gap.

## Non-conformant Productions

None. The audit found no productions that produce wrong output for valid spec input.

## `[NEEDS USER REVIEW]` Items — Resolved 2026-04-30

The 3 entries originally flagged for user adjudication (`[110] nb-double-text(n,c)`, `[121] nb-single-text(n,c)`, `[131] ns-plain(n,c)`) hinged on whether YAML 1.2.2 §7.4.2 permits multi-line implicit keys in flow mappings (the implementation accepts them) or whether §7.3.x's "single line" restriction applies regardless of context (which would make the implementation Lenient).

A detailed BNF-trace resolved the question in favor of the implementation's reading. **Final verdicts: all three are `Strict-conformant`.** No fix required.

### Resolution summary (full reasoning in `reconciliation-§7.md`'s [110] entry)

The §7.3.x prose ("scalars are restricted to a single line when contained inside an implicit key") is informal; the precise meaning is encoded in the BNF context labels. `BLOCK-KEY` and `FLOW-KEY` are the formal "implicit key" contexts; `FLOW-IN` is "inside a flow collection but not formally an implicit key."

Three places where implicit keys appear in the spec, and what context each forces:

1. **Block mapping implicit keys** (§8.2.2) — hardcoded `BLOCK-KEY` → one-line.
2. **Flow sequence single-pair compact form** `[ a: b ]` (§7.4.1, [152]) — hardcoded `FLOW-KEY` → one-line, regardless of outer context.
3. **Flow mapping entry keys** `{ a: b }` (§7.4.2, [145]) — uses **parent context `c`**, not hardcoded. At top level the parent is `FLOW-IN` (after `in-flow(c)` mapping), so the key parses with `ns-plain(n,FLOW-IN) ::= ns-plain-multi-line(n,FLOW-IN)` — multi-line allowed.

When a flow mapping is itself nested inside a block-key or flow-key, the outer one-line constraint cascades: the entire `{ ... }` must fit on one line, so inner keys are naturally one-line by the outer constraint.

The asymmetry between flow-sequence-pair and flow-mapping-entry is deliberate in the spec. The implementation's comment "Flow mappings `{...}` allow multi-line implicit keys — see YAML 1.2 §7.4.2" is accurate. De-facto behavior of mature parsers (libyaml, PyYAML, snakeyaml) follows the BNF — multi-line implicit keys in top-level flow mappings are accepted.

### Concrete examples

Spec-conformant (multi-line implicit key in top-level flow mapping accepted):
```yaml
{
  long
  key: value
}
```

Spec-conformant (one-line enforced in flow-sequence pair):
```yaml
[ key: value ]                    # OK
[
  key
  : value                         # REJECTED — flow-sequence pair uses hardcoded FLOW-KEY
]
```

Spec-conformant (one-line enforced when flow mapping is itself a block key):
```yaml
{ a: b }: outer-value             # OK
{
  a: b
}: outer-value                    # REJECTED at outer scope — block-key must be one line
```

### Doc-rewrite note

This BNF-trace analysis must propagate into the conformance doc rewrite verbatim — it documents a prose-vs-BNF terminology trap that is otherwise difficult to recover. The in-code comment at `flow.rs:1124-1135` could be tightened to reference the BNF terminology more precisely, but since the analysis lives here in the audit record, code-comment changes are optional.

## Doc Errata (informational — for future doc rewrite plan)

The conformance doc at `rlsp-yaml-parser/docs/yaml-spec-conformance.md` was found to mislabel multiple entries as `Conformant` when the audit determined them `Lenient`. These need correction in a follow-up doc-rewrite plan.

| Entry | Doc says | Audit says |
|---|---|---|
| [1] c-printable | Conformant | Lenient |
| [27] nb-char | Conformant | Lenient |
| [34] ns-char | Conformant | Lenient |
| [69] s-flow-line-prefix(n) | Conformant | Lenient |
| [75] c-nb-comment-text | Conformant | Lenient |
| [84] ns-directive-name | Conformant | Lenient |
| [85] ns-directive-parameter | Conformant | Lenient |
| [93] ns-tag-prefix | Conformant | Lenient |
| [94] c-ns-local-tag-prefix | Conformant | Lenient |
| [95] ns-global-tag-prefix | Conformant | Lenient |
| [99] c-ns-shorthand-tag | Conformant | Lenient |

Additionally:
- **[29] b-as-line-feed** — Doc cites `encoding.rs:179-197` (`normalize_line_breaks`); B's grep finds that function is unused in production. Audit verdict remains `Strict-conformant` because the structural pathway achieves the spec MUST, but the doc's citation is incorrect.
- **[105] c-ns-flow-pair-json-key-entry** (§7) — B noted citation drift: doc cites `flow.rs:502-507` for the `}` empty-key fallback; actual code is at `flow.rs:511-513`.

## Follow-up Filing

Per the dedup rule (spec section AND code location), the following Lenient productions warrant new follow-up entries (the existing `[1]` entry covers its own spec section + code location only):

| Entry | Spec section | Code location | Filed? |
|---|---|---|---|
| [27] nb-char | §5.4 | line splitter + nb-char enforcement gap | New |
| [34] ns-char | §5.5 | predicate at `chars.rs:67-74` not applied to scalar bodies | New |
| [69] s-flow-line-prefix(n) | §6.3 | `lexer/quoted.rs:107`, `:276` | New |
| [75] c-nb-comment-text | §6.6 | `lexer/comment.rs:50-51` | New |
| [84] + [85] directives | §6.8 | `event_iter/directives.rs` (combined entry) | New |
| [93] + [94] + [95] tag prefix | §6.9.1 | tag prefix validation (combined entry) | New |
| [99] c-ns-shorthand-tag | §6.9.1 | `event_iter/properties.rs:170-216` | New |

Productions with verdict `Strict-conformant`, `Stricter-than-spec`, `Not-applicable` do not generate follow-up entries.

The follow-up entries are appended to `.ai/memory/project_followup_plans.md` in a separate commit (`chore(memory): file phase 1 conformance audit follow-ups`).

## Cumulative Methodology Observations

- The dual-track methodology with strict input partitioning surfaced the c-printable enforcement gap (root cause of 4 of 11 Lenient findings) and 8 Lenient productions in §6 that the conformance doc had labeled as `Conformant`. Without the audit, these would have remained as silent leniency.
- Inter-auditor disagreement rates trended down across chapters as the reconciliation principle ("attribute strictness/leniency to the production where the rule is enforced") propagated through dispatch prompts: §5 (3.1%) → §6 (22.0%) → §7 (12.1%) → §8 (2.5%) → §9 (0.0%). §6 was the high water mark; the principle was added to dispatches starting in §7 and the rate fell.
- The 3 originally `[NEEDS USER REVIEW]` items in §7 were resolved on 2026-04-30 by detailed BNF-tracing (see `reconciliation-§7.md`'s [110] entry). All three finalized as `Strict-conformant`. The §7.3.x-vs-§7.4.2 prose-vs-BNF terminology analysis is preserved in the audit record for the doc rewrite.
- The conformance doc's "Conformant" label is unreliable for at least 11 entries (the Doc Errata listing). A doc-rewrite follow-up plan should correct the labels, update citations, and embed the BNF-trace analyses captured here.
- §10 (presentation/output) was out of scope (no BNF productions in the doc). Phase 2 (drafted at `.ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md`, pending user approval) covers normative-prose conformance: encoding (§5.2), directives (§6.8 behavioral), tag resolution (§6.9.1 behavioral), schemas (§10.1–10.3), error semantics, and limits.
