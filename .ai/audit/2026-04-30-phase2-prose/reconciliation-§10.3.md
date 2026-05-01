---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: Reconciliation
section: §10.3
date: 2026-04-30
produced-by: lead
---

# Reconciliation: §10.3 Core Schema

A enumerated 13 requirements (10 SC + 2 Lenient + 1 Stricter-than-spec); B enumerated 18 (17 SC + 1 Stricter-than-spec). Both auditors agreed on the Stricter-than-spec finding (leading-zero decimal rejection). They disagreed on signed octal/hex int handling — **A's verdict is correct**: the §10.3 spec table places `[-+]?` only on the decimal row, not on the octal or hex rows; the parser accepts signed octal/hex which spec doesn't permit. B mis-read the spec table.

The lead's investigation re-read the §10.3 normative table directly to resolve the disagreement (lead has Phase 1/Phase 2 spec access for reconciliation purposes).

## Final Verdict Tally

- `Strict-conformant`: 16
- `Stricter-than-spec`: 1 (leading-zero decimal rejection)
- `Lenient`: 2 (signed octal and signed hex accepted)
- `Not-applicable`: 0
- `Non-conformant`: 0
- `Indeterminate`: 0
- **Total: 19 reconciled requirements**

## Resolved Disagreement

### Signed octal and signed hex integers (A's REQs 4b/4c, B's REQs 8/9)

**A's verdict:** Lenient. Reasoning: the §10.3 spec table places `[-+]?` only on the decimal int row. Octal (`0o [0-7]+`) and hex (`0x [0-9a-fA-F]+`) rows are unsigned. The parser accepts signed octal (`-0o10`, `+0o10`) and signed hex (`-0xFF`, `+0xFF`) as `!!int`, which spec doesn't permit.

**B's verdict:** Strict-conformant. Reasoning: "Sign permitted by the global preceding `[-+]?` in the §10.3.2 entry." (B claims the sign is shared globally across the int rows.)

**Lead's investigation:** Read the §10.3 spec table directly at `/workspace/.ai/references/yaml-1.2.2-spec.md` lines 6608+:

```
| `[-+]? [0-9]+`                    | tag:yaml.org,2002:int (Base 10)
| `0o [0-7]+`                       | tag:yaml.org,2002:int (Base 8)
| `0x [0-9a-fA-F]+`                 | tag:yaml.org,2002:int (Base 16)
```

The `[-+]?` appears ONLY on the decimal row. The octal and hex rows have no sign. Each row is its own regex; there is no "global preceding `[-+]?`." B's reading is incorrect.

**Behavioral evidence:** the parser at `schema.rs:289-293` strips a leading sign (`+`/`-`) before dispatching to `is_core_int` per-base validators. Because the sign-strip is unconditional, signed octal `-0o10` becomes `0o10` (matched), and signed hex `+0xFF` becomes `0xFF` (matched). The implementation does not gate the sign strip to the decimal row only.

**Lead's verdict:** Lenient on both signed octal (item 4b) and signed hex (item 4c). The parser accepts inputs the spec rejects. Two distinct Lenient findings (different code paths within `is_core_int`).

**Fix sketch:** in `schema.rs:289-293`, gate the sign strip so that only decimal-shaped digit sequences receive sign treatment. After strip, if the stripped body begins with `0o` or `0x`, the sign was invalid for that row → fall back to `!!str`.

## Confirmed Disagreement-Free Findings

### Leading-zero decimal rejection (A's REQ-4a, B's REQ-7)

Both auditors agree: the parser rejects `007`, `01`, `0123` as `!!str` rather than recognizing them as `!!int`. The §10.3 decimal regex `[-+]? [0-9]+` literally permits leading zeros (any one-or-more-digit sequence). Implementation at `schema.rs:307-309` rejects bodies whose first byte is `0` and length > 1.

**Verdict:** Stricter-than-spec. Rationale (defensive): leading-zero decimals are commonly user-error confusion with octal — many YAML producers wrote `007` intending octal under YAML 1.1 conventions. The parser's rejection prevents accidental misinterpretation. This is similar to Phase 1 `[86]` (major-0 rejection) and `[87]` (u8 digit cap) — pragmatic defensive choices that exceed spec but have a clear rationale.

The `Stricter-than-spec` rationale should be documented in the conformance doc rewrite.

## Reconciled Requirement Table

The 19 unified requirements after subject-matter consolidation:

| # | Topic | Final verdict | A | B |
|---|---|---|---|---|
| 1 | Tag set identical to JSON | Strict-conformant | REQ-1 | REQ-1 |
| 2 | Schema is loader default (Core) | Strict-conformant | REQ-8 (combined) | REQ-2 |
| 3 | Schema selectability via `LoaderBuilder` | Strict-conformant | REQ-8 (combined) | REQ-3 |
| 4 | Null forms `null \| Null \| NULL \| ~` | Strict-conformant | REQ-2 | REQ-4 |
| 5 | Empty plain scalar → `!!null` | Strict-conformant | REQ-2 (combined) | REQ-5 |
| 6 | Bool forms (six exact strings, case-sensitive) | Strict-conformant | REQ-3 | REQ-6 |
| 7 | Decimal int (leading zeros REJECTED) | **Stricter-than-spec** | REQ-4a Stricter | REQ-7 Stricter |
| 8 | Octal int unsigned only — signed accepted is **Lenient** | **Lenient** | REQ-4b Lenient | REQ-8 SC (mis-read) |
| 9 | Hex int unsigned only — signed accepted is **Lenient** | **Lenient** | REQ-4c Lenient | REQ-9 SC (mis-read) |
| 10 | Decimal float regex | Strict-conformant | REQ-5a | REQ-10 |
| 11 | Float infinity (signed) | Strict-conformant | REQ-5b | REQ-11 |
| 12 | Float NaN (unsigned per spec; signed correctly rejected) | Strict-conformant | REQ-5c | REQ-12 |
| 13 | Plain unmatched scalars → `!!str` (Core permissive) | Strict-conformant | REQ-6 | REQ-13 |
| 14 | Quoted scalars override regex matching → `!!str` | Strict-conformant | REQ-7 (combined) | REQ-14 |
| 15 | Block scalars (literal/folded) → `!!str` | Strict-conformant | REQ-7 (combined) | REQ-15 |
| 16 | Untagged collection resolution by kind | Strict-conformant | (covered in §10.1) | REQ-16 |
| 17 | Explicit tag overrides resolution | Strict-conformant | (covered in §10.1) | REQ-17 |
| 18 | `-0` Core dispatch → `!!int` (regex `[-+]? [0-9]+`) | Strict-conformant | (in REQ-4a) | REQ-18 |
| 19 | Spec example (lines 6657–6677) replay | Strict-conformant | REQ-9 | (not separately enumerated) |

## Behavioral Evidence Highlights

- **YAML 1.1 hold-overs correctly excluded.** Both auditors confirmed `yes`, `no`, `Yes`, `No`, `YES`, `NO`, `y`, `n`, `Y`, `N`, `on`, `off`, `On`, `Off`, `ON`, `OFF` all resolve to `!!str` under Core. Spec §10.3 explicitly excludes these (they are §10.4 YAML 1.1 schema only).
- **Mixed-case null aliases excluded.** `nUll`, `NuLL`, `none`, `nil` all → `!!str`. Spec only allows `null`, `Null`, `NULL`, `~`, empty.
- **Special floats handled correctly.** `.inf`, `.Inf`, `.INF`, `+.inf`, `-.inf`, `.nan`, `.NaN`, `.NAN` all match. Signed NaN (`-.NaN`, `+.NaN`) correctly rejected per spec (NaN regex has no sign).
- **`0o9` and `0o8` correctly rejected** as out-of-range octal digits.
- **`0x` and `0o` prefix-only correctly rejected** (require at least one digit).
- **`-0` resolves to `!!int`** under Core (regex `[-+]? [0-9]+` permits it; int row precedes float row in resolution order). Contrast with §10.2 JSON where `-0` resolves to `!!float` — different regex shapes; both behaviors are spec-conformant in their respective schemas.

## Architectural Findings

None new beyond §10.1 / §10.2.

## Disposition for Phase 2 Summary (Task 8)

**New follow-up entries to file:**

1. **Signed octal `0o` int Lenient (§10.3, item 8)** — the parser accepts `-0o10` and `+0o10` as `!!int`; spec §10.3 octal regex `0o [0-7]+` is unsigned. Sign-strip in `schema.rs:289-293` is unconditional and applies before per-base validation. Verdict `Lenient`. Fix: gate sign strip to decimal-shaped bodies only.

2. **Signed hex `0x` int Lenient (§10.3, item 9)** — the parser accepts `-0xFF` and `+0xFF` as `!!int`; spec §10.3 hex regex `0x [0-9a-fA-F]+` is unsigned. Same root cause as item 8 (unconditional sign-strip). Could be deduplicated with item 8 into a single fix entry since the fix is at the same code location (`schema.rs:289-293`).

3. **Leading-zero decimal Stricter-than-spec (§10.3, item 7)** — the parser rejects `007`, `01`, `0123` as `!!str` though spec regex `[-+]? [0-9]+` permits leading zeros. Implementation at `schema.rs:307-309`. Stricter-than-spec; rationale (defensive against octal-confusion) should be documented in the conformance doc, not changed.

**Items not generating follow-ups:**

- The Stricter-than-spec finding (item 7) does not auto-file per the dedup rule; user decides whether to relax to spec or keep defensive rejection during the post-Phase-2 design-decisions batch.

**Doc errata observation:** the conformance doc currently marks the integer rows as "Conformant" without documenting the leading-zero rejection or the signed-octal/hex laxity. Both should be reflected in the doc rewrite.

## Methodology Notes

- §10.3 had the highest disagreement rate of Phase 2 schemas (2 cases out of 19 reconciled, both real disagreements rather than terminology calls). The disagreement on signed octal/hex was a spec-reading error by B; A's reading was correct. The lead resolved by re-reading the §10.3 spec table directly — a procedure the dispatch prompts allow only to the lead at reconciliation time, not to the auditors during their parallel runs.
- B's broader enumeration (18 vs A's 13) reflects that B split the schema-selection requirement into "default" and "selectability" sub-cases. Both are valid; the consolidation here uses B's finer-grained split to surface schema selection as two distinct properties.
- Probe cleanup held: both auditors used `/tmp/audit-probe-{§10.3-a, 103-a}/` (note A used `103` to avoid the section symbol in the path; both outside git tree); final `git status` clean.
- Phase 2 schema audits complete: §10.1 (8/8 SC), §10.2 (13/13 SC after reconciliation), §10.3 (16 SC + 1 Stricter + 2 Lenient). Cumulative schema findings: 1 spec-internal inconsistency (`-0` example vs §10.2 int regex), 2 new Lenient (signed octal/hex), 1 Stricter-than-spec (leading-zero decimal rejection). Last task (Task 7: error semantics + limits) remains.
