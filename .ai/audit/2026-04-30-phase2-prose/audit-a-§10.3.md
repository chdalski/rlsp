---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: A
section: §10.3
date: 2026-04-30
---

# Audit A — §10.3 Core Schema (behavioral)

## Method

Built a standalone probe at `/tmp/audit-probe-103-a/` that imports
`rlsp_yaml_parser` and loaded inputs via
`LoaderBuilder::new().build().load(input)` (Core is the loader default per
`loader.rs:189-196`) and explicit
`LoaderBuilder::new().schema(Schema::{Core,Json,Failsafe}).build()` for the
selection cases. For each input the probe printed the resolved
`tag` URI from each `Node::Scalar`/collection. Probe deleted **before**
authoring this report; `git status --porcelain` confirmed zero new files
in `rlsp-yaml-parser/`.

Spec read from `/workspace/.ai/references/yaml-1.2.2-spec.md` lines
6608–6678 (§10.3 "Core Schema"). The normative tag-resolution table is at
lines 6641–6652:

```
| Regular expression                          | Resolved to tag
| `null | Null | NULL | ~`                    | tag:yaml.org,2002:null
| `/* Empty */`                               | tag:yaml.org,2002:null
| `true | True | TRUE | false | False | FALSE`| tag:yaml.org,2002:bool
| `[-+]? [0-9]+`                              | tag:yaml.org,2002:int (Base 10)
| `0o [0-7]+`                                 | tag:yaml.org,2002:int (Base 8)
| `0x [0-9a-fA-F]+`                           | tag:yaml.org,2002:int (Base 16)
| `[-+]? ( \. [0-9]+ | [0-9]+ ( \. [0-9]* )? ) ( [eE] [-+]? [0-9]+ )?` | tag:yaml.org,2002:float
| `[-+]? ( \.inf | \.Inf | \.INF )`           | tag:yaml.org,2002:float
| `\.nan | \.NaN | \.NAN`                     | tag:yaml.org,2002:float
| `*`                                         | tag:yaml.org,2002:str (Default)
```

The §10.3 example output (lines 6655–6678) is also normative for Core
behaviour and was replayed end-to-end through the loader as a
cross-check.

Implementation evidence: `/workspace/rlsp-yaml-parser/src/schema.rs:195`
(`resolve_core_plain` first-byte dispatch), `:272` (`is_core_null`), `:278`
(`is_core_bool`), `:288` (`is_core_int`), `:318` (`is_core_float`), and
the loader plumbing at `/workspace/rlsp-yaml-parser/src/loader.rs:987-1033`
applying schema-resolved tags after loader construction. The default
schema is set at `loader.rs:195` (`schema: Schema::Core`).

## Per-requirement findings

### REQ-§10.3-1 — Tag set identical to JSON

- **Spec requirement:** Lines 6617–6619 — the Core schema "uses the same
  tags as the JSON schema". That is `Failsafe ∪ {null, bool, int,
  float}` — seven tags total.
- **Test method:** Drive the loader with one input per tag family and
  read back the URI string from `ResolvedTag::as_str`.
- **Test input:** `null`, `true`, `42`, `3.14`, `hello`, `[ ]`, `{ }`.
- **Observed output:**
  - `null` → `tag:yaml.org,2002:null`
  - `true` → `tag:yaml.org,2002:bool`
  - `42` → `tag:yaml.org,2002:int`
  - `3.14` → `tag:yaml.org,2002:float`
  - `hello` → `tag:yaml.org,2002:str`
  - `[ ]` → `tag:yaml.org,2002:seq`
  - `{ }` → `tag:yaml.org,2002:map`
- **Spec expectation:** All seven URI strings produced and unique.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:62-72` enumerates exactly the seven URIs;
  behavioral observation matches.
- **Reasoning:** The implementation surfaces the canonical
  `tag:yaml.org,2002:` URIs with the exact suffixes the spec lists. No
  extra or missing tag families.

### REQ-§10.3-2 — Null forms: `null | Null | NULL | ~ | <empty>`

- **Spec requirement:** Lines 6643–6644 — `null | Null | NULL | ~` and
  the empty string all resolve to `tag:yaml.org,2002:null`.
- **Test method:** Probe each spec form plus a few mixed-case
  near-misses; verify implicit-empty (`k:`) maps to null and quoted-empty
  (`k: ""`, `k: ''`) maps to `!!str`.
- **Test input:** `null`, `Null`, `NULL`, `~`, `nUll`, `NuLL`, `none`,
  `nil`, implicit empty (`k:`), double-quoted empty (`k: ""`),
  single-quoted empty (`k: ''`), double-quoted `null` (`k: "null"`),
  single-quoted `null` (`k: 'null'`).
- **Observed output:**
  - `null` / `Null` / `NULL` / `~` → `tag:yaml.org,2002:null`
  - `nUll`, `NuLL`, `none`, `nil` → `tag:yaml.org,2002:str`
  - implicit `k:` (no value) → `tag:yaml.org,2002:null`, value `""`,
    style `Plain`
  - quoted empty `k: ""` / `k: ''` → `tag:yaml.org,2002:str`, value `""`
  - quoted `k: "null"` / `k: 'null'` → `tag:yaml.org,2002:str`,
    value `"null"`
- **Spec expectation:** Only the four literal forms plus the empty plain
  string are null; mixed-case forms (`nUll`, etc.), `none`, `nil` are
  not in the regex (fall through to `!!str` per the default rule).
  Quoted scalars must override per requirement §10.3-7.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:272-274` matches exactly the five forms;
  `schema.rs:198` short-circuits empty/`~` before any other matcher;
  loader unconditionally rewrites quoted styles to `!!str` at
  `schema.rs:137-141`.
- **Reasoning:** Every positive spec form maps to `!!null`; every
  near-miss maps to `!!str` per the default fallback rule; quoted
  styles remain `!!str` regardless of content.

### REQ-§10.3-3 — Bool forms: `true | True | TRUE | false | False | FALSE`

- **Spec requirement:** Line 6645 — exactly the six listed
  capitalisations resolve to `!!bool`. YAML 1.1 spellings (`yes`/`no`/
  `on`/`off`/`y`/`n`) are explicitly excluded by §10.3.
- **Test method:** Probe each spec form plus mixed-case near-misses
  (`tRue`, `FaLsE`) and the YAML-1.1 set.
- **Test input:** `true`, `True`, `TRUE`, `false`, `False`, `FALSE`,
  `tRue`, `trUE`, `FaLsE`, `yes`/`Yes`/`YES`, `no`/`No`/`NO`,
  `on`/`ON`, `off`/`Off`, `y`/`Y`/`n`/`N`, `t`/`T`/`f`/`F`.
- **Observed output:**
  - All six listed forms → `tag:yaml.org,2002:bool`.
  - All others (mixed-case, YAML-1.1 spellings, single letters)
    → `tag:yaml.org,2002:str`.
- **Spec expectation:** Six bool tags, everything else `!!str` via the
  default fallback rule.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:278-283` `is_core_bool` matches exactly the
  six listed strings via `matches!`; no fall-through to `is_yaml11_bool`
  exists in the source.
- **Reasoning:** Implementation rejects YAML-1.1 boolean spellings as
  the §10.3 spec requires, while preserving every spec-listed form.

### REQ-§10.3-4a — Decimal int: `[-+]? [0-9]+`

- **Spec requirement:** Line 6646 — decimal int regex permits an
  optional sign and one or more digits. The spec example (line 6662)
  uses `0` and `-19` as int.
- **Test method:** Probe positive, negative, signed-positive, zero
  forms; cross-test leading-zero forms (`007`) and invalid-by-shape
  forms (`+`, ``).
- **Test input:** `0`, `1`, `42`, `-1`, `-100`, `+42`, `+0`, `-0`,
  `007`, `00`, `01`, `+`, ``.
- **Observed output:**
  - `0`, `1`, `42`, `-1`, `-100`, `+42`, `+0`, `-0` → `!!int`
  - `007`, `00`, `01` → `!!str`
  - `+` (sign only) → `!!str`
  - `` (empty plain) → `!!null` (caught by the null-empty rule first)
- **Spec expectation:** All eight signed-decimal cases match; `+`
  alone has no digit so does not match. The spec is silent on the
  leading-zero case — the regex `[0-9]+` would mathematically match
  `007`, but Core conventionally rejects that as ambiguous (libyaml,
  ruamel, PyYAML all do). The §10.3 spec table does not exclude `007`.
- **Verdict:** Stricter-than-spec on `007` / `00` / `01`.
- **Evidence:** `schema.rs:307-308` — `if rest.len() > 1 && rest
  .starts_with('0') { return false; }` explicitly rejects multi-digit
  leading-zero decimals.
- **Reasoning:** A literal reading of the §10.3 decimal regex
  `[-+]? [0-9]+` accepts `007` (the regex has no anti-leading-zero
  constraint). The implementation rejects it. This is interoperable
  practice — libyaml-derived parsers all do this — but it does not
  match the spec text. Documented as Stricter-than-spec; practical
  impact is zero for hand-written human YAML.

### REQ-§10.3-4b — Octal int: `0o [0-7]+`

- **Spec requirement:** Line 6647 — base-8 with `0o` prefix and one or
  more octal digits.
- **Test method:** Probe valid octal, invalid digit `0o9`,
  prefix-only `0o`, signed octals.
- **Test input:** `0o7`, `0o10`, `0o17`, `0o123`, `0o`, `0o9`,
  `-0o10`, `+0o10`.
- **Observed output:**
  - `0o7`, `0o10`, `0o17`, `0o123` → `!!int`
  - `-0o10`, `+0o10` → `!!int`
  - `0o`, `0o9` → `!!str`
- **Spec expectation:** Octal-form matches require digits in `[0-7]`;
  `0o9` is invalid. The spec regex `0o [0-7]+` has **no** sign prefix.
- **Verdict:** Lenient on signed octal (`-0o10`, `+0o10`).
- **Evidence:** `schema.rs:289-293` strips an optional sign **before**
  attempting either base-8 or base-16 dispatch. The §10.3 spec octal
  row regex is `0o [0-7]+` — sign is not in the row, only in the
  decimal row.
- **Reasoning:** A strict reading of the table treats `[-+]?` as
  binding only to the decimal row. The implementation accepts `-0o10`
  and `+0o10` as int. This is a Lenient extension of the spec — most
  parsers accept signed hex/octal in practice, but it is not in the
  spec table. Same finding applies to REQ-§10.3-4c.

### REQ-§10.3-4c — Hex int: `0x [0-9a-fA-F]+`

- **Spec requirement:** Line 6648 — base-16 with `0x` prefix and one
  or more hex digits (mixed case allowed). No sign prefix in the row.
- **Test method:** Probe valid hex, invalid digit (`0xZ`), prefix-only
  (`0x`), mixed-case digits, signed hex.
- **Test input:** `0x0`, `0xFF`, `0xff`, `0xAaBb`, `0x3A`, `0x`,
  `0xZ`, `-0xFF`, `+0xFF`.
- **Observed output:**
  - `0x0`, `0xFF`, `0xff`, `0xAaBb`, `0x3A` → `!!int`
  - `-0xFF`, `+0xFF` → `!!int`
  - `0x`, `0xZ` → `!!str`
- **Spec expectation:** Hex row matches without sign; sign is decimal-
  only per the table layout.
- **Verdict:** Lenient on signed hex (`-0xFF`, `+0xFF`).
- **Evidence:** Same code path as REQ-§10.3-4b — `schema.rs:289-293`
  strips the optional sign prefix before either prefix dispatch.
- **Reasoning:** Symmetric with the octal finding; documented separately
  because they are independent regex rows in the spec table. Practical
  impact: low (signed hex is uncommon in human-authored YAML), but the
  parser is broader than the spec on this row.

### REQ-§10.3-5a — Decimal float: `[-+]? ( \. [0-9]+ | [0-9]+ ( \. [0-9]* )? ) ( [eE] [-+]? [0-9]+ )?`

- **Spec requirement:** Line 6649 — sign-optional, mantissa is either
  `\.[0-9]+` (leading-dot) or `[0-9]+(\.[0-9]*)?` (digit-first with
  optional fraction); optional exponent `[eE][-+]?[0-9]+`.
- **Test method:** Probe many sign × mantissa × exponent
  combinations, plus invalid edges (`1e` no exponent digits).
- **Test input:** `0.0`, `-0.0`, `0.`, `+0.5`, `.5`, `-.5`, `+.5`,
  `1.0`, `-1.5`, `1.5e3`, `1.5E-3`, `1.5e+3`, `1e10`, `1E10`, `1e+10`,
  `1e-10`, `12e03`, `-2E+05`, `1e`, `1e+`, `1.5.6`, `1e3.5`.
- **Observed output:**
  - All listed positives → `!!float`.
  - `1e`, `1e+`, `1.5.6`, `1e3.5` → `!!str`.
- **Spec expectation:** Every listed positive matches; the negatives
  must not. Notably `1e10` matches the digit-first mantissa with no
  fraction and a valid exponent.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:341-378` `is_core_decimal_float`. Note the
  guard at `:371-374`: a digit-first mantissa with no `.` and no
  exponent is rejected (so `42` resolves through the int path, not
  through float). This guard preserves the int/float dispatch order.
- **Reasoning:** The implementation honours each clause of the spec
  regex, including the asymmetric `[0-9]+(\.[0-9]*)?` shape that
  permits `0.` (trailing-dot) and the leading-dot form `.5`. Spec
  example (line 6664) uses `0.`, `-0.0`, `.5`, `+12e03`, `-2E+05` —
  all observed `!!float`.

### REQ-§10.3-5b — Float infinity: `[-+]? ( \.inf | \.Inf | \.INF )`

- **Spec requirement:** Line 6650 — three capitalisations of `.inf`,
  with optional sign.
- **Test method:** Probe each spec form, signed variants, and
  near-misses without the leading dot.
- **Test input:** `.inf`, `.Inf`, `.INF`, `-.inf`, `+.inf`, `-.INF`,
  `+.Inf`, `inf`, `Inf`, `INF`.
- **Observed output:**
  - All `.{inf,Inf,INF}` and signed variants → `!!float`.
  - Bare `inf`, `Inf`, `INF` → `!!str`.
- **Spec expectation:** Leading dot is mandatory; sign is optional;
  three capitalisations only.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:325-332` strips an optional sign and tests
  `unsigned ∈ {".inf",".Inf",".INF"}`.
- **Reasoning:** All twelve sign × capitalisation cases listed in the
  spec are covered; near-misses without the dot fall to `!!str`.

### REQ-§10.3-5c — Float NaN: `\.nan | \.NaN | \.NAN`

- **Spec requirement:** Line 6651 — three capitalisations of `.nan`,
  **no sign** in the regex.
- **Test method:** Probe each spec form, near-miss without dot, and
  unspec'd signed forms (`+.NaN`, `-.NaN`).
- **Test input:** `.nan`, `.NaN`, `.NAN`, `nan`, `NaN`, `+.NaN`,
  `-.NaN`.
- **Observed output:**
  - `.nan`, `.NaN`, `.NAN` → `!!float`.
  - `nan`, `NaN`, `+.NaN`, `-.NaN` → `!!str`.
- **Spec expectation:** Three forms only; sign forbidden; bare nan
  without dot is not in the regex.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:319-322` `matches!(value, ".nan" |
  ".NaN" | ".NAN")` runs **before** sign-stripping; signed `.NaN`
  forms therefore fall through to the decimal-float path which
  rejects them.
- **Reasoning:** The spec is unusual in not allowing signed NaN
  (vs. signed inf); the implementation honours that asymmetry by
  testing NaN before sign-strip.

### REQ-§10.3-6 — Plain unmatched fallback to `!!str`

- **Spec requirement:** Lines 6635–6638 — for plain scalars, "if
  none of the regular expressions matches, the scalar is resolved
  to `tag:yaml.org,2002:str`". Distinguishing feature vs. JSON
  schema.
- **Test method:** Probe a variety of plain scalars that look like
  but do not match any tag family.
- **Test input:** `hello`, `abc123`, `1abc`, `true_value`, `null!`,
  `xyz`, `5x`, `1+2`, plus the rejected forms surfaced in earlier
  tests (`007`, `nUll`, `Yes`, `inf`, `+.NaN`, `1.5.6`).
- **Observed output:** All inputs above → `tag:yaml.org,2002:str`.
- **Spec expectation:** Default fallback bucket is `!!str` (not an
  error, unlike JSON schema where `*` resolves to "Error").
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:204`, `:212`, `:222`, `:230`, `:234` — all
  branches of `resolve_core_plain` end at `ResolvedTag::Str` for
  unmatched values; the loader does **not** error.
- **Reasoning:** The §10.3 default rule is implemented as
  unconditional `!!str` fallthrough. No `UnresolvedScalar` error path
  is reachable under `Schema::Core` (the error branch in
  `loader.rs:1027` is JSON-only).

### REQ-§10.3-7 — Quoted scalars override regex resolution to `!!str`

- **Spec requirement:** Lines 6635–6636 — only **plain scalars** are
  matched against the regex table. Single-quoted, double-quoted, and
  block scalars carry their own scalar style and are unconditionally
  string-typed.
- **Test method:** Wrap each tag-family-recognised value in single
  and double quotes, plus literal/folded blocks, and observe the
  resolved tag.
- **Test input:** `'null'`, `"null"`, `'42'`, `"42"`, `'3.14'`,
  `"3.14"`, `'true'`, `"true"`, `'~'`, `"~"`, `''`, `""`, `'.inf'`,
  `".nan"`, plus literal `|null`, `|42` and folded `>true`, `>3.14`.
- **Observed output:** Every quoted/blocked input → `tag:yaml.org,
  2002:str`. Style is preserved as `SingleQuoted`/`DoubleQuoted`/
  `Literal(_)`/`Folded(_)` and value is the lexed content.
- **Spec expectation:** Quoting overrides every regex match.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:137-141` — non-`Plain` styles short-circuit
  to `ResolvedTag::Str` before the matcher is consulted.
- **Reasoning:** Single line of code in `resolve_scalar` collapses
  every non-plain style to `!!str`. Behavioural output confirms this
  for every spec-recognised text plus the empty quoted forms.

### REQ-§10.3-8 — Schema selection (default Core, explicit Json/Failsafe)

- **Spec requirement:** Lines 6612–6613 — Core is "the recommended
  default schema". The implementation must permit it as the
  unparametrized default and also support explicit selection of the
  other two §10 schemas.
- **Test method:** Test the same input (`0xFF`) under each schema
  and confirm the bare `load()` and `LoaderBuilder::new().build()`
  paths both default to Core.
- **Test input:** `0xFF` under each `Schema` variant; whole-document
  comparison `load(yaml)` vs. `LoaderBuilder::schema(Core).build()
  .load(yaml)` for `yaml = "- 0xFF\n- null\n- 3.14\n"`.
- **Observed output:**
  - `0xFF` under `Schema::Core` → `tag:yaml.org,2002:int`
  - `0xFF` under `Schema::Json` → `LoadError::UnresolvedScalar`
  - `0xFF` under `Schema::Failsafe` → `tag:yaml.org,2002:str`
  - `format!("{:?}", default_load) == format!("{:?}",
    core_load)` evaluated to `true`.
- **Spec expectation:** Core is default; explicit Failsafe and Json
  selectable; tag resolution differs across schemas as the spec
  prescribes.
- **Verdict:** Strict-conformant.
- **Evidence:** `loader.rs:189-196` (`LoaderOptions::default()` sets
  `schema: Schema::Core`); `loader.rs:265-267` builder method;
  `schema.rs:130-156` schema-dispatch in `resolve_scalar`.
- **Reasoning:** Default is Core; selection works; behaviour
  differs across schemas as the spec table prescribes.

### REQ-§10.3-9 — Spec example replay (lines 6657–6677)

- **Spec requirement:** The spec gives a specific example of Core
  tag resolution at lines 6657–6677. All twenty-four nodes have
  named expected tags.
- **Test method:** Load the example verbatim and walk every node;
  print the resolved tag.
- **Test input:**

  ```yaml
  A null: null
  Also a null:
  Not a null: ""
  Booleans: [ true, True, false, FALSE ]
  Integers: [ 0, 0o7, 0x3A, -19 ]
  Floats: [ 0., -0.0, .5, +12e03, -2E+05 ]
  Also floats: [ .inf, -.Inf, +.INF, .NAN ]
  ```

- **Observed output:**
  - `A null: null` → `null::!!null`
  - `Also a null:` → `(empty)::!!null`
  - `Not a null: ""` → `(empty)::!!str`
  - `Booleans: [true, True, false, FALSE]` → all `!!bool`
  - `Integers: [0, 0o7, 0x3A, -19]` → all `!!int` (with values 0,
    7, 58, -19 if parsed numerically; the parser keeps the source
    text and applies the tag)
  - `Floats: [0., -0.0, .5, +12e03, -2E+05]` → all `!!float`
  - `Also floats: [.inf, -.Inf, +.INF, .NAN]` → all `!!float`
- **Spec expectation:** Every node's tag matches the spec example.
- **Verdict:** Strict-conformant.
- **Evidence:** Behavioral output above; cross-references to
  `schema.rs:272-336` matchers cited per requirement.
- **Reasoning:** The spec's own end-to-end example resolves
  byte-for-byte under the implementation. This is the strongest
  single behavioral check for §10.3 conformance.

## Summary

11 requirements enumerated:

| Requirement | Verdict |
|---|---|
| §10.3-1 tag set | Strict-conformant |
| §10.3-2 null forms | Strict-conformant |
| §10.3-3 bool forms | Strict-conformant |
| §10.3-4a decimal int | Stricter-than-spec (rejects `007` / `00` / `01`) |
| §10.3-4b octal int | Lenient (accepts signed `-0o10`, `+0o10`) |
| §10.3-4c hex int | Lenient (accepts signed `-0xFF`, `+0xFF`) |
| §10.3-5a decimal float | Strict-conformant |
| §10.3-5b float infinity | Strict-conformant |
| §10.3-5c float NaN | Strict-conformant |
| §10.3-6 fallback to !!str | Strict-conformant |
| §10.3-7 quoted overrides | Strict-conformant |
| §10.3-8 schema selection | Strict-conformant |
| §10.3-9 spec example replay | Strict-conformant |

Tally: 10 Strict-conformant, 2 Lenient (signed octal/hex int —
indistinguishable code path), 1 Stricter-than-spec (leading-zero
decimal int).

The two Lenient findings are practical-interoperability divergences:
the spec table places `[-+]?` only on the decimal int row, yet the
implementation strips signs before dispatching to any int subform.
The Stricter-than-spec finding (leading-zero decimal rejected) is
common-practice across reference parsers but is not what the §10.3
regex literally says — it is a deliberate over-strictness, similar
to the `-0` JSON-spec inconsistency surfaced in §10.2.

No `Indeterminate` cases. No `Not-applicable` cases.
