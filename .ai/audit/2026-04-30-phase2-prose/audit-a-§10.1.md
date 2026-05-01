---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: A
section: §10.1
date: 2026-04-30
---

# Phase 2 Behavioral Audit — §10.1 Failsafe Schema (Auditor A)

Scope: end-to-end behavioral audit of `Schema::Failsafe` tag
resolution against the normative requirements of YAML 1.2.2 §10.1
(tags, kind-based resolution, `!` non-specific resolution, `?`
non-specific handling, scalar style independence, schema
selectability).

Method: probes were run via a standalone audit-probe Cargo project at
`/tmp/audit-probe-§10.1-a/` depending on `rlsp-yaml-parser` by path,
so nothing was added to the parser tree. The probe used
`LoaderBuilder::new().schema(Schema::Failsafe).build().load(input)`
and `Schema::Core` for cross-schema selectability checks. The probe
project was deleted immediately after observing all outputs; the
parser tree was not modified.
`git status --porcelain` shows zero new files in
`/workspace/rlsp-yaml-parser/`.

Spec source: `/workspace/.ai/references/yaml-1.2.2-spec.md`,
§10.1 beginning at line 6259.

Parser source: `/workspace/rlsp-yaml-parser/src/schema.rs`,
`/workspace/rlsp-yaml-parser/src/loader.rs`.

---

## REQ-§10.1-1 — Tag set: only `str`, `seq`, `map`

**Spec requirement.** §10.1 Tags subsections (lines 6267–6365) define
exactly three tags for the Failsafe schema:
`tag:yaml.org,2002:map`, `tag:yaml.org,2002:seq`,
`tag:yaml.org,2002:str`. No others are defined for this schema.

**Test method.** Inspected `Schema::Failsafe` resolution paths in
`schema.rs` and observed that every probe input under Failsafe
produced one of these three tag URIs. Cross-checked with
`ResolvedTag::as_str()` URIs.

**Test input.** Probes R2-* (12 scalar inputs of varied content),
R3-* (3 sequence inputs), R4-* (3 mapping inputs), E-bare-bang-*
inputs, E-mixed-types, E-multi-doc.

**Observed output.** Every Failsafe-produced node carried either
`tag:yaml.org,2002:str`, `tag:yaml.org,2002:seq`, or
`tag:yaml.org,2002:map`. No `int`, `float`, `bool`, or `null` URIs
were observed in any Failsafe probe (those URIs only appeared in the
R6 cross-schema check that selected `Schema::Core`, not Failsafe).

**Spec expectation.** Only `str`/`seq`/`map` URIs.

**Verdict.** `Strict-conformant`.

**Evidence.** `schema.rs:131` (`Schema::Failsafe => Ok(Some(ResolvedTag::Str))`);
`schema.rs:177–183` (`resolve_collection` returns
`Seq`/`Map` for both kinds across all schemas); enum
`ResolvedTag` covers seven URIs total but Failsafe paths only emit
the three §10.1 ones.

**Reasoning.** The Failsafe code paths cannot produce non-§10.1
tags by construction: the scalar branch is a single arm returning
`ResolvedTag::Str`, and the collection branch dispatches purely on
`CollectionKind` (Sequence → Seq, Mapping → Map). Schema-level
resolution cannot escape these three outcomes when no explicit
source tag is present.

---

## REQ-§10.1-2 — All scalars resolve to `!str` regardless of content

**Spec requirement.** §10.1 Generic String (line 6334) defines `!!str`
as "a sequence of zero or more Unicode characters". The Tag
Resolution subsection (lines 6368–6375) states that `!`-tagged
scalars resolve to `!!str` according to kind. Combined with the
schema's exclusive `str`/`seq`/`map` tag set (REQ-§10.1-1), the
resolved tag for every scalar produced by the Failsafe schema must
be `!!str` — including content that under other schemas would be
treated as int/float/bool/null.

**Test method.** Loaded each input under
`Schema::Failsafe`; inspected `Node::Scalar.tag` for the resolved
URI.

**Test input.** Twelve plain scalar inputs spanning the categories
that other schemas would resolve away from `!str`:
`42`, `3.14`, `true`, `FALSE`, `~`, `null`, `hello`, `0o17`, `0xFF`,
`.inf`, `.nan`, plus an explicitly-empty scalar `key:\n` (yielding
the value `""`).

**Observed output.** Every input produced
`tag=Some("tag:yaml.org,2002:str")`. Examples (full literals from the
probe):

```
INPUT: "42\n"     → tag=Some("tag:yaml.org,2002:str") value="42"
INPUT: "true\n"   → tag=Some("tag:yaml.org,2002:str") value="true"
INPUT: "~\n"      → tag=Some("tag:yaml.org,2002:str") value="~"
INPUT: "null\n"   → tag=Some("tag:yaml.org,2002:str") value="null"
INPUT: ".inf\n"   → tag=Some("tag:yaml.org,2002:str") value=".inf"
INPUT: ".nan\n"   → tag=Some("tag:yaml.org,2002:str") value=".nan"
INPUT: "0xFF\n"   → tag=Some("tag:yaml.org,2002:str") value="0xFF"
INPUT: "key:\n"   → key tag=str, val tag=str (empty value)
```

**Spec expectation.** Resolved tag URI is
`tag:yaml.org,2002:str` for all scalar content.

**Verdict.** `Strict-conformant`.

**Evidence.** `schema.rs:130–131`
(`Schema::Failsafe => Ok(Some(ResolvedTag::Str))`) — the Failsafe
arm has no content branching whatsoever; every scalar (regardless of
style or value) maps to `Str`. `loader.rs:1015–1024` writes the
resolved URI back to the node's `tag` field.

**Reasoning.** The `Schema::Failsafe` branch is a single
unconditional arm returning `ResolvedTag::Str`. Plain scalars
carrying number/boolean/null lexical forms do not divert from this
path. Spec content-irrelevance is preserved exactly.

---

## REQ-§10.1-3 — All sequences resolve to `!seq`

**Spec requirement.** §10.1 Generic Sequence (line 6303) defines
`!!seq` and §10.1 Tag Resolution (line 6370) requires kind-based
resolution under the Failsafe schema's `!`-non-specific convention.
Combined with the schema's tag set, every sequence resolves to
`!!seq`.

**Test method.** Loaded sequence inputs under `Schema::Failsafe`;
inspected `Node::Sequence.tag`.

**Test input.** Block sequence (`- a\n- b\n`), flow sequence
(`[1, 2, 3]\n`), empty flow sequence (`[]\n`), bare-`!`-tagged
sequence (`! [1, 2]\n`), and the outer sequence in
`E-mixed-types` (5 mixed scalars).

**Observed output.** Every sequence produced
`tag=Some("tag:yaml.org,2002:seq")`:

```
INPUT: "- a\n- b\n"      → Sequence style=Block tag=seq items=2
INPUT: "[1, 2, 3]\n"     → Sequence style=Flow  tag=seq items=3
INPUT: "[]\n"            → Sequence style=Flow  tag=seq items=0
INPUT: "! [1, 2]\n"      → Sequence style=Flow  tag=seq items=2
```

**Spec expectation.** `tag:yaml.org,2002:seq` for every sequence
node.

**Verdict.** `Strict-conformant`.

**Evidence.** `schema.rs:179–182`
(`resolve_collection` returns `ResolvedTag::Seq` when
`kind == CollectionKind::Sequence`, ignoring the schema parameter
since all three schemas share collection resolution); `loader.rs:1051–1064`
writes the URI to `Node::Sequence.tag`. Bare-`!` translation:
`loader.rs:1052` filters `!` to `None` so the schema fallback
applies (`effective_tag = tag.as_deref().filter(|t| *t != "!")`).

**Reasoning.** Behavioral observation matches across block, flow,
empty, and bare-`!`-tagged forms. The bare-`!` case correctly
resolves to `!!seq` rather than being preserved as the literal
non-specific tag.

---

## REQ-§10.1-4 — All mappings resolve to `!map`

**Spec requirement.** §10.1 Generic Mapping (line 6269) defines
`!!map`; resolved analogously to REQ-§10.1-3.

**Test method.** Loaded mapping inputs under `Schema::Failsafe`;
inspected `Node::Mapping.tag`.

**Test input.** Block mapping (`key: value\n`), flow mapping
(`{a: 1}\n`), empty flow mapping (`{}\n`), bare-`!`-tagged mapping
(`! {a: 1}\n`).

**Observed output.** Every mapping produced
`tag=Some("tag:yaml.org,2002:map")`:

```
INPUT: "key: value\n"    → Mapping style=Block tag=map entries=1
INPUT: "{a: 1}\n"        → Mapping style=Flow  tag=map entries=1
INPUT: "{}\n"            → Mapping style=Flow  tag=map entries=0
INPUT: "! {a: 1}\n"      → Mapping style=Flow  tag=map entries=1
```

**Spec expectation.** `tag:yaml.org,2002:map` for every mapping.

**Verdict.** `Strict-conformant`.

**Evidence.** `schema.rs:179–182` (mapping branch returns
`ResolvedTag::Map`); `loader.rs:1035–1049` writes the URI to
`Node::Mapping.tag`. Bare-`!` translation is symmetric with the
sequence case at `loader.rs:1038`.

**Reasoning.** Same structure as REQ-§10.1-3; all four input
shapes resolve to `!!map` with no content sensitivity.

---

## REQ-§10.1-5 — Plain and quoted scalars resolve identically

**Spec requirement.** §10.1's tag set has no provision for style-
based differentiation: a scalar's resolved tag is `!!str` regardless
of whether it was authored as plain, single-quoted, double-quoted,
literal block, or folded block. Combined with REQ-§10.1-2, every
scalar style must produce `!!str` under Failsafe.

**Test method.** Loaded the same value `42` under five distinct
scalar styles (and `true`/`hello` under additional styles) under
`Schema::Failsafe`; compared the resolved tags.

**Test input.**

```
plain:           "42\n"            → style=Plain
double-quoted:   "\"42\"\n"        → style=DoubleQuoted
single-quoted:   "'42'\n"          → style=SingleQuoted
literal block:   "|\n  hello\n"    → style=Literal(Clip)
folded block:    ">\n  hello\n"    → style=Folded(Clip)
```

**Observed output.** All five produced
`tag=Some("tag:yaml.org,2002:str")`:

```
"42\n"           → Scalar style=Plain        tag=str value="42"
"\"42\"\n"       → Scalar style=DoubleQuoted tag=str value="42"
"'42'\n"         → Scalar style=SingleQuoted tag=str value="42"
"|\n  hello\n"   → Scalar style=Literal(Clip) tag=str value="hello\n"
">\n  hello\n"   → Scalar style=Folded(Clip)  tag=str value="hello\n"
```

**Spec expectation.** Identical resolved tag (`!!str`) across all
styles for any value.

**Verdict.** `Strict-conformant`.

**Evidence.** `schema.rs:130–131` — the Failsafe arm does not
match on `style`. The `style` parameter is bound but not inspected,
unlike the Core/JSON arms which do dispatch on style. Behavioral
observation confirms: identical `Str` URI across plain, single-
quoted, double-quoted, literal, and folded.

**Reasoning.** Style-blindness is structurally enforced by the
match arm.

---

## REQ-§10.1-6 — Schema is per-loader selectable; Failsafe is selectable

**Spec requirement.** §10.1 line 6263: "A YAML processor should
therefore support this schema, at least as an option." Failsafe must
be selectable independently of any other schema chosen as the
default.

**Test method.** Built two loaders from the same `LoaderBuilder`
constructor with different schema selections; loaded the same input
(`42\n`) through each and compared resolved tags.

**Test input.**

```rust
let core_loader     = LoaderBuilder::new().schema(Schema::Core).build();
let failsafe_loader = LoaderBuilder::new().schema(Schema::Failsafe).build();
core_loader.load("42\n");      // → Core resolution
failsafe_loader.load("42\n");  // → Failsafe resolution
```

**Observed output.**

```
CORE schema:     tag = Some("tag:yaml.org,2002:int")
FAILSAFE schema: tag = Some("tag:yaml.org,2002:str")
```

**Spec expectation.** Failsafe is independently selectable from
the public API; selection changes resolution behavior.

**Verdict.** `Strict-conformant`.

**Evidence.** `loader.rs:181–185` (`LoaderOptions.schema: Schema`
field exists), `loader.rs:260–268`
(`LoaderBuilder::schema(self, s: Schema) -> Self` selector), and
`schema.rs:24–36` (`Schema::Failsafe` is one of three public enum
variants). The behavioral observation that `42` resolves to `int`
under Core and to `str` under Failsafe confirms the selector
actually drives different resolution paths at load time.

**Reasoning.** The default schema is `Schema::Core`
(`loader.rs:195`), but `Schema::Failsafe` is reachable as a public
enum variant via the builder selector. Both options compose into
distinct observable outcomes for the same input.

---

## REQ-§10.1-7 — Explicit `!`-tag forces kind-based resolution

**Spec requirement.** §10.1 Tag Resolution (line 6370): "All [nodes]
with the `!` non-specific tag are resolved … to `tag:yaml.org,2002:seq`,
`tag:yaml.org,2002:map` or `tag:yaml.org,2002:str`, according to
their [kind]." The bare `!` written in source is the
non-specific-tag marker; it must resolve to the kind-appropriate
schema tag rather than being preserved as a literal `!`.

**Test method.** Loaded inputs with leading bare `!` for each kind
(scalar, sequence, mapping) under `Schema::Failsafe`; inspected the
resolved tag.

**Test input.**

```
"! 42\n"        → bare-! scalar
"! [1, 2]\n"    → bare-! sequence
"! {a: 1}\n"    → bare-! mapping
```

**Observed output.**

```
"! 42\n"        → Scalar   style=Plain tag=str value="42"
"! [1, 2]\n"    → Sequence style=Flow  tag=seq items=2
"! {a: 1}\n"    → Mapping  style=Flow  tag=map entries=1
```

**Spec expectation.** Bare-`!`-tagged nodes resolve to the
kind-appropriate Failsafe tag (`!!str` / `!!seq` / `!!map`); the
literal `!` does not survive into the resolved AST.

**Verdict.** `Strict-conformant`.

**Evidence.** `loader.rs:1010–1013` handles the scalar bare-`!`
case explicitly: `if tag.as_deref() == Some("!") { *tag =
Some(Cow::Borrowed(crate::schema::ResolvedTag::Str.as_str())); }`.
`loader.rs:1038` and `loader.rs:1052` filter the literal `!` to
`None` for collections, allowing `resolve_collection` to inject the
kind-based URI. All three observed resolutions match the spec's
kind-dispatch convention.

**Reasoning.** Bare-`!` non-specific tags are correctly mapped
through to the schema's three tags. The behavioral path is
asymmetric in implementation (scalar handled inline, collections
through filter+resolve) but both paths converge on the spec
requirement.

---

## REQ-§10.1-8 — Explicit specific tag is preserved (not overridden)

**Spec requirement.** §10.1 governs *non-specific* tag resolution.
Spec line 6261 begins: "The failsafe schema is guaranteed to work
with any YAML document" — including documents that already carry
explicit specific tags. An explicit `!!str`, `!!seq`, `!!map`, or
any other source tag must not be replaced by the schema resolver,
because resolution is defined for the `!` and `?` non-specific
markers (lines 6370, 6374), not for nodes that already carry a
specific tag (line 1141: "a complete representation specifies the
tag of each node").

**Test method.** Loaded inputs with explicit tag prefixes (`!!str`,
`!!int`) under `Schema::Failsafe`; verified the source tag is
preserved.

**Test input.**

```
"!!str 42\n"     → explicit str tag
"!!int 42\n"     → explicit int tag (not in Failsafe's tag set,
                   but must be preserved because it is explicit)
```

**Observed output.**

```
"!!str 42\n"     → Scalar tag=Some("tag:yaml.org,2002:str") value="42"
"!!int 42\n"     → Scalar tag=Some("tag:yaml.org,2002:int") value="42"
```

**Spec expectation.** Source tag survives schema resolution
unchanged.

**Verdict.** `Strict-conformant`.

**Evidence.** `schema.rs:125–128`: "Explicit source tag takes
priority over schema resolution. … if source_tag.is_some() {
return Ok(None); }". The early return ensures Failsafe never
overwrites an explicit specific tag. `loader.rs:1015–1026` honours
the `Ok(None)` return by leaving `tag` unchanged.

**Reasoning.** Behavioral output preserves both the in-schema
`!!str` and the out-of-schema `!!int` exactly as authored. This
matches §10.1's scope (it governs non-specific tag resolution, not
override of explicit tags).

---

## §10.1 Verdict Tally

- REQ-§10.1-1 (only str/seq/map URIs): `Strict-conformant`
- REQ-§10.1-2 (all scalars → !str): `Strict-conformant`
- REQ-§10.1-3 (all sequences → !seq): `Strict-conformant`
- REQ-§10.1-4 (all mappings → !map): `Strict-conformant`
- REQ-§10.1-5 (plain ≡ quoted resolution): `Strict-conformant`
- REQ-§10.1-6 (schema selectable per-loader): `Strict-conformant`
- REQ-§10.1-7 (bare `!` → kind-based tag): `Strict-conformant`
- REQ-§10.1-8 (explicit specific tag preserved): `Strict-conformant`

Eight requirements, all Strict-conformant.

## Architectural Findings

### `?` non-specific tag handling vs §10.1's literal text

§10.1 Tag Resolution (lines 6373–6375) reads:

> "All [nodes] with the `?` non-specific tag are left [unresolved].
> This constrains the [application] to deal with a [partial
> representation]."

The parser's `Schema::Failsafe` arm
(`schema.rs:131`) does not preserve "unresolved" status on `?`-tagged
nodes (i.e., on plain scalars and untagged collections). Instead it
unconditionally resolves them to `!str`/`!seq`/`!map`. The AST does
not carry a "left unresolved" marker — every node ends with a
resolved tag URI in `Node::*.tag`.

This is the universally-implemented Failsafe behaviour (the Core and
JSON schemas of §10.2/§10.3 do the same kind of replacement; the
"partial representation" path of §3.1 is not what production parsers
emit). It is therefore not a per-requirement non-conformance — the
behavioural outcomes that downstream consumers observe are exactly
the §10.1 tag URIs. But it is worth flagging architecturally:
nothing in `Node` distinguishes a tag injected by the resolver from
one preserved from the source. Downstream code that wanted to
distinguish "explicit tag" from "resolver-injected tag" would have
to consult `NodeMeta.tag_loc` (which the resolver clears on
injection — see `loader.rs:1018–1023, 1042–1048, 1057–1063`). This
does work, but it is a side-channel, not an explicit "unresolved"
marker.

This is a downstream-design observation, not a §10.1 verdict: the
spec text about "partial representation" is processor-permissive
("the YAML processor *may* compose a partial representation",
line 1221), and resolving everything to `!str` under Failsafe is the
ecosystem-standard interpretation.

### Default schema is Core, not Failsafe

`LoaderOptions::default()` (`loader.rs:188–198`) and the
top-level `load()` convenience function (`loader.rs:313`) default
to `Schema::Core`, not `Schema::Failsafe`. §10.1 line 6262 calls
Failsafe "the recommended schema for generic YAML tools", but
"recommended" is non-mandatory and §10.3 line 6612 calls Core
"the recommended default schema that YAML processor should use
unless instructed otherwise" — so the parser follows §10.3's
recommendation.
This is consistent with the spec; recording for completeness.
