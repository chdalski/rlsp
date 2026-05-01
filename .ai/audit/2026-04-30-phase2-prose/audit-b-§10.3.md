---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: B
section: §10.3
date: 2026-04-30
---

# Audit B — §10.3 Core Schema (behavioral)

## Methodology

All findings are based on direct behavioral observation. A standalone
audit-probe Cargo project at `/tmp/audit-probe-§10.3-b/` exercised
`Loader::load` with `Schema::Core` (and contrast probes for `Schema::Json`
and `Schema::Failsafe`). Tag URIs were read from
`Node::Scalar.tag` / `Node::Mapping.tag` / `Node::Sequence.tag` after the
loader applied `apply_schema_to_node`. The probe was deleted immediately
after observation; no new files exist in `rlsp-yaml-parser/`.

Source map for evidence-citation:

- Schema selection: `rlsp-yaml-parser/src/loader.rs:185–197` (default
  `Schema::Core`), `loader.rs:264–268` (`LoaderBuilder::schema`).
- Resolver dispatch: `rlsp-yaml-parser/src/schema.rs:118–157`
  (`resolve_scalar`), `schema.rs:194–236` (`resolve_core_plain`).
- Per-form matchers: `schema.rs:272–274` (null), `:278–283` (bool),
  `:288–312` (int), `:318–337` (float), `:341–378`
  (`is_core_decimal_float`).
- Loader application: `loader.rs:987–1068` (`apply_schema_to_node`).

## Per-requirement entries

### REQ-§10.3-1 Tag set is identical to JSON (Core inherits Failsafe + JSON tags)

- Spec (§10.3.1): "The Core schema is an extension of the JSON schema,
  allowing for more human-readable presentation of the same types. […]
  The tags listed in this section are the same as for the JSON schema."
- Observed: All Core resolutions produce one of seven URIs:
  `tag:yaml.org,2002:str`, `:int`, `:float`, `:bool`, `:null`,
  `:seq`, `:map`. No additional tags emitted. The mapping
  `ResolvedTag → URI` is enumerated at `schema.rs:62–72` and is closed
  (matching tagless input never produces another URI under Core). The
  unique URI set was confirmed across the full probe.
- Evidence: probe outputs across `null`, `true`, `42`, `3.14`, `hello`,
  `a: 1`, `- a` map to exactly the seven URIs above; no others.
- Verdict: **Conformant**.

### REQ-§10.3-2 Schema selection — Core is loader default

- Spec (§10.3): "It is recommended that based on the Core schema for
  general purpose YAML data."
- Observed:
  - `LoaderBuilder::new().build()` and `LoaderBuilder::new().schema(Core).build()`
    produce identical resolutions (e.g. `42 → !!int`, `TRUE → !!bool`,
    `~ → !!null`).
  - `Schema::Core` is the literal default in `LoaderOptions::default()`
    (`loader.rs:188–197`).
- Evidence: `schema(Core)` and bare `build()` both resolved `TRUE` to
  `tag:yaml.org,2002:bool`; under explicit `Schema::Json` the same `TRUE`
  produced `Err(UnresolvedScalar)`.
- Verdict: **Conformant**.

### REQ-§10.3-3 Schema selectability via `LoaderBuilder::schema(Schema::Core)`

- Spec (§10.3): the Core schema is one of the three recommended schemas;
  the processor is expected to expose it as a selectable mode.
- Observed: `LoaderBuilder::schema(Schema::Core)` accepted; the resulting
  loader applies Core resolution. The `Schema::{Core,Json,Failsafe}`
  enum is `pub` at `schema.rs:25–36`. All three schemas were exercised
  in the same probe and produced distinct outcomes for the same input
  (`42 → !!int` under Core/Json; `42 → !!str` under Failsafe).
- Evidence: explicit-Core, explicit-Json, explicit-Failsafe all produced
  distinct resolutions for the same plain `42`.
- Verdict: **Conformant**.

### REQ-§10.3-4 Null forms — `null | Null | NULL | ~`

- Spec (§10.3.2 row): regex `null | Null | NULL | ~` resolves to
  `tag:yaml.org,2002:null`.
- Observed (all → `tag:yaml.org,2002:null`): `null`, `Null`, `NULL`, `~`.
- Negative cases (all → `tag:yaml.org,2002:str`): `nUll`, `Nul`, `NULl`,
  `nil`, `none`, `NULLA`, `NULLISH`, mixed-case variants outside the
  three exact forms.
- Implementation: `is_core_null` at `schema.rs:272–274` is a closed
  `matches!` over the four exact strings — exact-string match, no
  prefix/suffix admittance.
- Verdict: **Conformant**.

### REQ-§10.3-5 Empty plain scalar (no characters) → `!!null`

- Spec (§10.3.2 row): regex `/* Empty */` resolves to
  `tag:yaml.org,2002:null`. The accompanying note states "These cover the
  cases where a `null` value is not specified explicitly."
- Observed:
  - Explicit empty key value: `k:\n` produces a scalar value with
    `tag = Some("tag:yaml.org,2002:null")`, `value = ""`.
  - `is_core_null("")` returns true (line 273 includes `""` in the
    closed match).
- Implementation: `resolve_core_plain` at `schema.rs:198` short-circuits
  on empty string via `None` first-byte branch. The `is_core_null("")`
  case is also covered as a pure-string call.
- Verdict: **Conformant**.

### REQ-§10.3-6 Bool forms — six exact strings

- Spec (§10.3.2 row): regex
  `true | True | TRUE | false | False | FALSE` resolves to
  `tag:yaml.org,2002:bool`.
- Observed (all → `!!bool`): `true`, `True`, `TRUE`, `false`, `False`,
  `FALSE`.
- Negative cases — all → `!!str`:
  - YAML 1.1 hold-overs: `yes`, `Yes`, `YES`, `no`, `No`, `NO`, `y`, `Y`,
    `n`, `N`, `on`, `On`, `ON`, `off`, `Off`, `OFF`.
  - Mixed-case false positives: `tRue`, `TRUe`.
- Implementation: `is_core_bool` at `schema.rs:278–283` is a closed
  `matches!` of exact strings. The dispatcher at `schema.rs:208–214`
  routes only `t/T/f/F` first bytes.
- Verdict: **Conformant**. Notably, `yes`/`no`/`y`/`Y`/`on`/`off` are
  rejected — Core schema correctly excludes the YAML 1.1 set.

### REQ-§10.3-7 Decimal int — `[-+]? [0-9]+`

- Spec (§10.3.2 row): regex `[-+]? [0-9]+` resolves to
  `tag:yaml.org,2002:int` (Base 10).
- Observed (all → `!!int`): `0`, `1`, `42`, `-1`, `-42`, `+0`, `+1`,
  `+42`.
- Spec-permitted-but-rejected? **Yes — leading-zero decimals are
  rejected** as a leniency-vs-strictness consideration:
  - `007` → `!!str`, `00` → `!!str`, `0123` → `!!str`,
    `-007` → `!!str`, `+007` → `!!str`.
  - Implementation: `is_core_int` at `schema.rs:307–309` rejects
    `rest.len() > 1 && rest.starts_with('0')` for the decimal branch.
- Spec analysis: the §10.3.2 regex `[-+]? [0-9]+` admits leading
  zeros — `007` literally matches `[0-9]+`. The implementation rejects
  these. This is a **Strict** classification: the parser rejects input
  the spec regex permits.
- Conformance doc: line 2095 lists `core_positive_signed_int_resolves_to_int`
  but does not name the leading-zero behavior; the regex it quotes is
  the unmodified `[-+]?[0-9]+`. The doc does not flag this as a
  divergence.
- Verdict: **Strict** (Core decimal regex rejects spec-permitted leading
  zeros). The conformance doc claims **Conformant**; this audit
  disagrees with the specific wording and a Strict sub-finding belongs
  here. Note: leading zeros render the value ambiguous between decimal
  and "should have been an octal" interpretations across YAML 1.1 vs
  1.2; the implementation chose strict. The behavior is internally
  consistent and deliberate (the matcher's comment at `schema.rs:286`
  explicitly says "Leading zeros in decimal (e.g. `007`) are rejected").

### REQ-§10.3-8 Octal int — `0o [0-7]+`

- Spec (§10.3.2 row): regex `0o [0-7]+` resolves to
  `tag:yaml.org,2002:int` (Base 8). Sign permitted by the global
  preceding `[-+]?` in the §10.3.2 entry.
- Observed (all → `!!int`): `0o0`, `0o7`, `0o17`, `0o755`, `-0o10`,
  `+0o10`, `0o0`, `0o377`.
- Negative cases (all → `!!str`): `0o` (prefix only), `0o8`, `0o9`,
  `0b101` (no Core binary form).
- Implementation: `is_core_int` at `schema.rs:299–301` strips `0o`
  prefix, requires nonempty digits, validates each byte in `0..=7`.
  Sign-stripping at `schema.rs:289–293` handles `-` and `+` for octal.
- Verdict: **Conformant**.

### REQ-§10.3-9 Hex int — `0x [0-9a-fA-F]+`

- Spec (§10.3.2 row): regex `0x [0-9a-fA-F]+` resolves to
  `tag:yaml.org,2002:int` (Base 16). Sign permitted.
- Observed (all → `!!int`): `0x0`, `0xFF`, `0xff`, `0xFFFFFFFF`,
  `0xAbCdEf12` (mixed case), `0xDEADBEEF`, `-0x1A`, `+0x1A`.
- Negative cases (all → `!!str`): `0x` (prefix only), `0xZZ`.
- Implementation: `is_core_int` at `schema.rs:302–304` strips `0x`
  prefix, requires nonempty digits, validates each byte via
  `is_ascii_hexdigit`.
- Verdict: **Conformant**.

### REQ-§10.3-10 Decimal float — `[-+]? ( \. [0-9]+ | [0-9]+ ( \. [0-9]* )? ) ( [eE] [-+]? [0-9]+ )?`

- Spec (§10.3.2 row): regex above resolves to `tag:yaml.org,2002:float`
  (Number).
- Observed (all → `!!float`): `3.14`, `.5`, `0.5`, `1.`, `1.5`, `-1.5`,
  `+1.5`, `1e10`, `1E10`, `1e+10`, `1e-10`, `1.5e3`, `.5e3`, `0.0`.
- Negative cases (all → `!!str`): `.`, `1.2.3`, `1e`, `1e+`, `.e10`,
  `+.`, `-.`, `1.5e`, `1.5e+`, `.inf3`, `1einf`, `1ee2`, `1.2e3.4`.
- Spec-internal note: the regex literally matches `[0-9]+` with no
  fractional and no exponent (i.e. `42`), which would also match the
  decimal-int regex. Dispatch order resolves the ambiguity — int
  matchers run first (`schema.rs:217–223`), so `42` → `!!int`. The
  float matcher at `schema.rs:369–375` explicitly rejects digit-first
  forms with no fractional and no exponent — preventing fall-through
  collisions.
- Implementation: `is_core_decimal_float` at `schema.rs:341–378`
  validates leading-dot form, digit-first form (with optional
  fractional or empty fractional), and optional exponent.
- Verdict: **Conformant**.

### REQ-§10.3-11 Infinity — `[-+]? ( \.inf | \.Inf | \.INF )`

- Spec (§10.3.2 row): regex above resolves to `tag:yaml.org,2002:float`
  (Infinity).
- Observed (all → `!!float`): `.inf`, `.Inf`, `.INF`, `+.inf`, `-.inf`.
- Negative cases (all → `!!str`): `inf` (no leading dot), `Infinity`,
  `+inf` (sign without dot), `-inf` (sign without dot).
- Implementation: `is_core_float` at `schema.rs:319–337` strips optional
  sign, then exact-matches `.inf | .Inf | .INF` after sign-strip.
- Verdict: **Conformant**.

### REQ-§10.3-12 Not-a-number — `\.nan | \.NaN | \.NAN`

- Spec (§10.3.2 row): regex above resolves to `tag:yaml.org,2002:float`
  (Not a number). Note: the spec table does NOT permit a sign on
  `.nan` (the leading `[-+]?` is on the infinity row, not the NaN row).
- Observed (all → `!!float`): `.nan`, `.NaN`, `.NAN`.
- Negative cases (all → `!!str`): `nan` (no dot), `NaN` (no dot),
  `Infinity`.
- Implementation: `is_core_float` at `schema.rs:320–322` matches the
  three NaN forms BEFORE sign-stripping — i.e. `+.nan` and `-.nan`
  are NOT recognized as NaN by this branch alone. After sign-strip,
  the unsigned matcher only matches `.inf` family. So `+.nan` falls
  through both inf and nan branches → must be checked.
- Edge probe: `+.nan` was not in the test list above. Reading source:
  `is_core_float(+.nan)` → first check `matches!(value, ".nan"|...)`
  fails (value has `+`). Then sign-strip yields `.nan`. Then check
  `matches!(unsigned, ".inf"|...)` fails. Then `is_core_decimal_float(".nan")`
  — leading-dot form, requires digits after `.`; `.nan` after the dot
  is `nan` which fails `b.is_ascii_digit()` → false. So `+.nan` →
  `!!str`. This is **Conformant** with the spec (no sign on NaN).
- Verdict: **Conformant**.

### REQ-§10.3-13 Plain-scalar fallback to `!!str`

- Spec (§10.3.2 row): final row `*` resolves to
  `tag:yaml.org,2002:str` (Default). Stated as: "if none of the regular
  expressions matches, the [scalar] is [resolved] to
  `tag:yaml.org,2002:str`."
- Observed (all → `!!str`): `hello`, `abc`, `true_thing`, `123abc`,
  `0x` (hex prefix only), `007` (leading-zero decimal), `nUll`, `nil`,
  `none`, `yes`, `on`, `off`, `inf`, `nan`, `Infinity`, `1_000`,
  `0b101`.
- Implementation: dispatcher branches at `schema.rs:196–235` route
  every non-matching plain scalar to `ResolvedTag::Str`. The catch-all
  `Some(_) => Str` at line 234 handles all first bytes outside the
  enumerated prefix sets.
- Verdict: **Conformant**.

### REQ-§10.3-14 Quoted scalars always `!!str`

- Spec (§10.3.2): the regex table applies to scalars with the `?`
  non-specific tag — i.e. plain scalars. Non-plain (quoted/block)
  scalars carry the `!` non-specific tag, which the Failsafe schema
  resolves to `!!str`.
- Observed (all → `!!str`): `"42"`, `'42'`, `"true"`, `"~"`.
- Implementation: `resolve_scalar` at `schema.rs:137–141` matches
  `SingleQuoted | DoubleQuoted | Literal | Folded` and returns
  `ResolvedTag::Str` unconditionally — the value is never inspected.
- Verdict: **Conformant**.

### REQ-§10.3-15 Block scalars (literal/folded) always `!!str`

- Spec (§10.3.2): same rule — non-plain scalars are `!!str`.
- Observed (all → `!!str`): `|\n  42\n` (literal),
  `>\n  null\n` (folded).
- Implementation: same as REQ-§10.3-14; `Literal(_)` and `Folded(_)`
  variants resolve unconditionally.
- Verdict: **Conformant**.

### REQ-§10.3-16 Untagged collection resolution

- Spec (§10.3.2): "[Collections] with the "`?`" non-specific tag (that
  is, [untagged] [collections]) are [resolved] to "`tag:yaml.org,2002:seq`"
  or "`tag:yaml.org,2002:map`" according to their [kind]." (Inherited
  from JSON schema.)
- Observed:
  - Block mapping `a: 1` → root tag `tag:yaml.org,2002:map`.
  - Block sequence `- a` → root tag `tag:yaml.org,2002:seq`.
  - Flow mapping `{}` → root tag `tag:yaml.org,2002:map`.
  - Flow sequence `[]` → root tag `tag:yaml.org,2002:seq`.
- Implementation: `resolve_collection` at `schema.rs:168–183` ignores
  schema parameter (line 178: `let _ = schema;`) and dispatches purely
  on `CollectionKind`. All three schemas use the same code path; under
  Core specifically, untagged sequences/mappings always receive the
  correct kind-tag.
- Verdict: **Conformant**.

### REQ-§10.3-17 Explicit tag overrides resolution

- Spec (§10.3.2 introductory wording, common to §10.x: explicit tags
  are not subject to schema pattern resolution; the explicit tag wins.
- Observed:
  - `!!str 42` → `tag:yaml.org,2002:str` (explicit `!!str` preserved;
    `42` not promoted to `!!int`).
  - `!!int hello` → `tag:yaml.org,2002:int` (explicit `!!int` preserved
    even though the value would resolve to `!!str` by default).
  - `!custom 42` → `!custom` (local tag preserved verbatim, no schema
    resolution).
- Implementation: `resolve_scalar` at `schema.rs:126–128` short-circuits
  on `source_tag.is_some()`. The `apply_schema_to_node` at
  `loader.rs:1010–1014` translates only the bare-`!` non-specific tag
  to `!!str` (a separate concern); other tags are passed through.
- Verdict: **Conformant**.

### REQ-§10.3-18 `-0` Core dispatch — int, not float

- Spec (§10.3.2): the int row matches `[-+]? [0-9]+`. `-0` matches this
  regex; the int row precedes the float row, so `-0` resolves to
  `!!int` under Core.
- Observed: `-0` (Core) → `tag:yaml.org,2002:int`. (Compare:
  `-0` (Json) → `tag:yaml.org,2002:float` — JSON's int regex is the
  bare `0 | -?[1-9][0-9]*`, and `-0` does not match `0 | -?[1-9][0-9]*`
  but does match the JSON float regex.)
- Implementation: dispatcher routes on first byte; `is_core_int("-0")`
  returns true (sign-strip leaves `0`, decimal branch accepts since
  `rest.len() == 1`). Float matcher never runs.
- Verdict: **Conformant** (Core). The `-0` distinction between Core
  and JSON is an artifact of the different regexes in §10.2 vs §10.3,
  not a parser bug.

## Verdict tally

| ID | Title | Verdict |
|----|-------|---------|
| REQ-§10.3-1 | Tag set identical to JSON | Conformant |
| REQ-§10.3-2 | Schema selection — Core is default | Conformant |
| REQ-§10.3-3 | Selectability via `LoaderBuilder::schema` | Conformant |
| REQ-§10.3-4 | Null forms `null \| Null \| NULL \| ~` | Conformant |
| REQ-§10.3-5 | Empty plain scalar → `!!null` | Conformant |
| REQ-§10.3-6 | Bool forms (six exact strings) | Conformant |
| REQ-§10.3-7 | Decimal int `[-+]? [0-9]+` | **Strict** |
| REQ-§10.3-8 | Octal int `0o [0-7]+` | Conformant |
| REQ-§10.3-9 | Hex int `0x [0-9a-fA-F]+` | Conformant |
| REQ-§10.3-10 | Decimal float | Conformant |
| REQ-§10.3-11 | Infinity (signed) | Conformant |
| REQ-§10.3-12 | NaN (unsigned) | Conformant |
| REQ-§10.3-13 | Plain-scalar fallback to `!!str` | Conformant |
| REQ-§10.3-14 | Quoted scalars → `!!str` | Conformant |
| REQ-§10.3-15 | Block scalars → `!!str` | Conformant |
| REQ-§10.3-16 | Untagged collections | Conformant |
| REQ-§10.3-17 | Explicit tag overrides | Conformant |
| REQ-§10.3-18 | `-0` Core dispatch — int | Conformant |

**Totals:** 17 Conformant, 1 Strict, 0 Lenient, 0 Indeterminate.

## Conformance-doc disagreement

The conformance doc at `rlsp-yaml-parser/docs/yaml-spec-conformance.md`
line 2090–2095 marks "Core Schema — tag resolution for plain scalars"
as **Conformant** without naming the leading-zero decimal-int rejection
behavior. This audit disagrees: under the spec's literal regex
`[-+]? [0-9]+`, `007` and `0123` match and should resolve to `!!int`.
The implementation rejects these (`schema.rs:307–309`), classifying
them as `!!str`. This is a **Strict** divergence — the parser rejects
spec-permitted input.

The implementation's choice is internally consistent and explicitly
documented in code (`schema.rs:286`: "Leading zeros in decimal
(e.g. `007`) are rejected"). It guards against the ambiguity between
YAML 1.1 octal interpretation (`007` = octal 7) and YAML 1.2 decimal
interpretation. Whether to classify this as `Strict (security-hardened)`
vs plain `Strict` is a deliberateness assessment for the reviewer; this
audit reports the bare `Strict` class because the source comment
documents the rejection but does not reference a security or
compatibility rationale. The conformance doc should add this finding
or the entry should be re-classified.

## Notes on test-coverage authority

The conformance doc lists `core_negative_zero_resolves_to_int` (line
2095) — this audit confirms that test's expectation behaviorally:
`-0` under Core → `!!int`. Cross-referenced unit test at
`schema.rs:797–798` verifies `-0` (Json) → `!!float`, which is consistent
with the dispatch documented at `schema.rs:248–251`.
