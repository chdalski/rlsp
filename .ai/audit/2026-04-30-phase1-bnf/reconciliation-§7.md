---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: Reconciliation
section: §7
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §7

58 entries reconciled. 51 entries had identical verdicts from Auditor A and Auditor B; 7 entries had disagreements. All 7 disagreements were resolved by lead investigation; the 3 `[NEEDS USER REVIEW]` flags initially raised on `[110]`, `[121]`, `[131]` were finalized as `Strict-conformant` after a detailed BNF-trace of §7.3.x prose against §7.4.2 grammar (resolved 2026-04-30 — see entry [110]'s reasoning).

## Final Verdict Tally

- `Strict-conformant`: 57
- `Stricter-than-spec`: 0
- `Lenient`: 0
- `Not-applicable`: 1
- `Non-conformant`: 0
- `Indeterminate`: 0

§7's clean tally reflects that the parser's flow-style implementation is mature and well-tested. The §7.3.x-vs-§7.4.2 BNF analysis (formerly the open question) is captured in [110]'s entry below and should propagate verbatim into the conformance doc rewrite.

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

### [110] nb-double-text(n,c)

**A's verdict:** Indeterminate — implicit-key single-line restriction not directly enforced by passing context to the lexer.
**B's verdict:** Strict-conformant — surrounding parsers reject `:` separators across lines.

**Lead's investigation (resolved 2026-04-30):** Initially flagged `[NEEDS USER REVIEW]` because the BNF interaction between §7.3.1's prose ("scalars are restricted to a single line when contained inside an implicit key") and §7.4.2's flow-mapping productions was non-obvious. Resolved by tracing the BNF context propagation in detail.

**The §7.3.x prose-vs-BNF terminology trap.** The §7.3.1, §7.3.2, §7.3.3 prose informally says "scalars are restricted to a single line when contained inside an implicit key." The precise meaning is encoded in the BNF context labels — `BLOCK-KEY` and `FLOW-KEY` are the formal "implicit key" contexts; `FLOW-IN` is "inside a flow collection but not formally an implicit key." The terms differ.

**Three places where implicit keys appear in the spec:**

1. **Block mapping implicit keys** (§8.2.2, [193] `ns-s-block-map-implicit-key`):
   ```
   ns-s-block-map-implicit-key ::=
       c-s-implicit-json-key(BLOCK-KEY)
     | ns-s-implicit-yaml-key(BLOCK-KEY)
   ```
   Hardcoded `BLOCK-KEY` → `nb-double-text(n,BLOCK-KEY) ::= nb-double-one-line` → **one-line**.

2. **Flow sequence single-pair compact form** (§7.4.1, [152] `ns-flow-pair-yaml-key-entry`):
   ```
   ns-flow-pair-yaml-key-entry(n,c) ::=
     ns-s-implicit-yaml-key(FLOW-KEY)   # hardcoded FLOW-KEY, NOT parent c
     c-ns-flow-map-separate-value(n,c)
   ```
   Hardcoded `FLOW-KEY` → **one-line**, regardless of outer context.

3. **Flow mapping entry keys** (§7.4.2, [145] `ns-flow-map-yaml-key-entry`):
   ```
   ns-flow-map-yaml-key-entry(n,c) ::=
     ns-flow-yaml-node(n,c)             # uses PARENT context c, NOT hardcoded FLOW-KEY
     ...
   ```
   Uses parent context `c` flowing in from `c-flow-mapping(n,c)`:
   ```
   c-flow-mapping(n,c) ::=
     c-mapping-start
     s-separate(n,c)?
     ns-s-flow-map-entries(n,in-flow(c))?    # in-flow(c) maps the context
     c-mapping-end
   ```
   And `in-flow(c)` (§7.4):
   ```
   in-flow(n,FLOW-OUT)  ::= ns-s-flow-seq-entries(n,FLOW-IN)
   in-flow(n,FLOW-IN)   ::= ns-s-flow-seq-entries(n,FLOW-IN)
   in-flow(n,BLOCK-KEY) ::= ns-s-flow-seq-entries(n,FLOW-KEY)
   in-flow(n,FLOW-KEY)  ::= ns-s-flow-seq-entries(n,FLOW-KEY)
   ```

   So inside `{ key: value }`:
   - At top level (outer `c=FLOW-OUT` / `FLOW-IN`): entries get `FLOW-IN` context → key parses as `nb-double-text(n,FLOW-IN) ::= nb-double-multi-line(n)`. **Multi-line allowed.**
   - Inside a block-key or flow-key context (outer `c=BLOCK-KEY` / `FLOW-KEY`): entries get `FLOW-KEY` → key parses as `nb-double-text(n,FLOW-KEY) ::= nb-double-one-line`. **One-line.**

**Why the asymmetry is deliberate.** Flow-sequence-pair compact form uses the named `ns-s-implicit-yaml-key(FLOW-KEY)` production — a formal "implicit key" with hardcoded one-line constraint. Flow-mapping entry keys use `ns-flow-yaml-node(n,c)` — a regular flow node with parent context, NOT a formal "implicit key." The colloquial reading of "the key in `{ a: b }` is an implicit key" is correct in everyday language but not in spec-grammar terminology.

**Why nested cases still work.** A flow mapping nested inside a block-mapping implicit key (`{ a: b }: value`) is constrained at the OUTER level by `ns-s-implicit-yaml-key(BLOCK-KEY)`'s "At most 1024 characters altogether" + single-line rule. The entire `{ a: b }` must fit on one line; inner keys are naturally one-line by the outer constraint.

**Concrete examples illustrating the verdict.**

Spec-conformant (multi-line implicit key in top-level flow mapping accepted):
```yaml
{
  long
  key: value
}
```
At top level, c=FLOW-OUT → entries=FLOW-IN → key=`ns-plain(n,FLOW-IN)`=multi-line. Per spec, valid.

Spec-conformant (one-line enforced in flow-sequence pair):
```yaml
[ key: value ]                    # OK — one line
[
  key
  : value                         # REJECTED — flow-sequence pair uses hardcoded FLOW-KEY
]
```

Spec-conformant (one-line enforced when flow mapping is itself an implicit block key):
```yaml
{ a: b }: outer-value             # OK — fits on one line
{
  a: b
}: outer-value                    # REJECTED at outer scope — block-key must be one line
```

**Implementation evidence.** `event_iter/flow.rs:1124-1135` enforces single-line for flow sequences (`in_sequence` check); the comment "Flow mappings `{...}` allow multi-line implicit keys — see YAML 1.2 §7.4.2" matches the BNF analysis above. The de-facto behavior of mature parsers (libyaml, PyYAML, snakeyaml) follows the BNF — multi-line implicit keys in top-level flow mappings are accepted.

**Lead's verdict (final):** Strict-conformant. The implementation correctly traces the BNF context propagation and enforces the §7.3.x prose restriction precisely where the BNF mandates (BLOCK-KEY hardcoded sites and FLOW-KEY hardcoded sites for flow-sequence-pair compact form), without over-restricting the parent-context flow-mapping case the BNF permits.

**Doc-rewrite note.** The conformance doc rewrite should preserve this BNF-tracing analysis verbatim — it is the only place the prose-vs-BNF terminology trap is documented for future readers. The in-code comment at `flow.rs:1124-1135` could be tightened to "flow-mapping entry keys at top level (FLOW-IN context) per BNF §7.4.2, not formal implicit keys per §7.3.x terminology" — but since the analysis lives here, code-comment changes are optional.

### [111] nb-double-one-line

**A's verdict:** Strict-conformant
**B's verdict:** Stricter-than-spec — hex-escape printability gate + bidi-control rejection.

**Lead's investigation:** Same root as [107] — strictness is at sub-productions [59]/[60]/[61], not at this composing production.

**Lead's verdict:** Strict-conformant.

### [121] nb-single-text(n,c)

**A's verdict:** Indeterminate — same concern as [110] for single-quoted scalars.
**B's verdict:** Strict-conformant — same composed-enforcement reasoning as [110].

**Lead's investigation (resolved 2026-04-30):** Same §7.4.2-vs-§7.3.x BNF analysis as [110]. The single-quoted text production has the same shape:
```
nb-single-text(FLOW-OUT)  ::= nb-single-multi-line(n)
nb-single-text(FLOW-IN)   ::= nb-single-multi-line(n)
nb-single-text(BLOCK-KEY) ::= nb-single-one-line
nb-single-text(FLOW-KEY)  ::= nb-single-one-line
```
Flow-mapping entry keys inherit the parent context (FLOW-IN at top level → multi-line allowed); BLOCK-KEY and FLOW-KEY hardcoded sites force one-line. See [110] for the full BNF-trace and concrete examples.

**Lead's verdict (final):** Strict-conformant.

### [128] ns-plain-safe-out

**A's verdict:** Lenient — leniency inherited from §5 [34] ns-char.
**B's verdict:** Strict-conformant — implementation correctly composes [34]; leniency is at [34]'s source, not propagated.

**Lead's investigation:** Per the §5 reconciliation principle (attribute leniency to the production where the rule is enforced), [128] is Strict-conformant. [34]'s leniency is captured in §5's reconciliation; propagating it up the tree creates noise without informational value.

**Lead's verdict:** Strict-conformant.

### [131] ns-plain(n,c)

**A's verdict:** Indeterminate — same concern as [110] / [121] for plain scalars.
**B's verdict:** Strict-conformant — same composed-enforcement reasoning.

**Lead's investigation (resolved 2026-04-30):** Same §7.4.2-vs-§7.3.x BNF analysis as [110]. The plain scalar production has the same shape:
```
ns-plain(n,FLOW-OUT)  ::= ns-plain-multi-line(n,FLOW-OUT)
ns-plain(n,FLOW-IN)   ::= ns-plain-multi-line(n,FLOW-IN)
ns-plain(n,BLOCK-KEY) ::= ns-plain-one-line(BLOCK-KEY)
ns-plain(n,FLOW-KEY)  ::= ns-plain-one-line(FLOW-KEY)
```
Flow-mapping entry keys inherit the parent context (FLOW-IN at top level → multi-line allowed); BLOCK-KEY and FLOW-KEY hardcoded sites force one-line. See [110] for the full BNF-trace and concrete examples.

**Lead's verdict (final):** Strict-conformant.

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
