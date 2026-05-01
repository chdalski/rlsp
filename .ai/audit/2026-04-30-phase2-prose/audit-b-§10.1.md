---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: B
section: §10.1
date: 2026-04-30
---

# Phase 2 Behavioral Audit — §10.1 Failsafe Schema (Auditor B)

## Method

All inputs were exercised through the public `load()` API
via a standalone audit-probe Cargo project at
`/tmp/audit-probe-§10.1-b/` with a path dependency on
`rlsp-yaml-parser`. No probe code was added to the parser
tree (`git status --porcelain rlsp-yaml-parser/` returns
empty). Each requirement records the bytes fed to the
parser, the AST tag the loader produced under
`Schema::Failsafe`, and where relevant the comparison
against the default `Schema::Core` to confirm schema
isolation. The probe was deleted immediately after
observing output, before this audit was written.

The conformance doc records six §10.1 entries
(yaml-spec-conformance.md:2041-2074), all "Conformant".
The probes below test those claims behaviorally; where the
parser exhibits stricter or more lenient behavior than the
spec mandates, this audit records the discrepancy on the
requirement where the rule is enforced (per the symmetric
reconciliation principle).

Spec source: `https://yaml.org/spec/1.2.2/` §10.1
(Failsafe Schema), §10.1.1 (Tags) and §10.1.2 (Tag
Resolution). The fetched normative wording: "The failsafe
schema uses only three tags: tag:yaml.org,2002:map,
tag:yaml.org,2002:seq, and tag:yaml.org,2002:str". For the
non-specific `!` tag (§3.3.2 / §10.1.2): "YAML processors
should resolve nodes having the '!' non-specific tag as
'tag:yaml.org,2002:seq', 'tag:yaml.org,2002:map' or
'tag:yaml.org,2002:str' depending on their kind".

## REQ-§10.1-1 — Failsafe defines exactly three tags (`!!str`, `!!seq`, `!!map`)

- **Spec requirement (§10.1):** "The failsafe schema uses
  only three tags: `tag:yaml.org,2002:map`,
  `tag:yaml.org,2002:seq`, `tag:yaml.org,2002:str`."
- **Test method:** Inspected `Schema::Failsafe` enum
  variant and the implementation of `apply_schema_to_node`.
  For untagged inputs of every kind (scalar / sequence /
  mapping) under `Schema::Failsafe`, recorded the produced
  AST tag URI.
- **Observed output:** Every untagged scalar (regardless
  of plain content `42`, `null`, `true`, `hello`, `.inf`,
  `0xff`) produces tag `tag:yaml.org,2002:str`. Untagged
  sequences (block `- a` and flow `[a, 1]`) produce
  `tag:yaml.org,2002:seq`. Untagged mappings (block
  `a: 1` and flow `{a: 1}`) produce
  `tag:yaml.org,2002:map`. No untagged input under
  Failsafe ever produces `!!int`, `!!float`, `!!bool`, or
  `!!null`.
- **Spec expectation:** Schema's resolved-tag output set
  must be `{ str, seq, map }`.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:130-131`
  (`Schema::Failsafe => Ok(Some(ResolvedTag::Str))` —
  unconditional `Str` for scalars regardless of style or
  content); `schema.rs:178-182` (`resolve_collection`
  returns `Seq`/`Map` from `kind` alone, the `schema`
  parameter is discarded with `let _ = schema`); applied
  in `loader.rs:1015` and `loader.rs:1039-1054`.
- **Reasoning:** The resolver function for Failsafe is a
  constant function of (kind) — it never inspects scalar
  content, never produces `Int`/`Float`/`Bool`/`Null`. The
  only way a Failsafe-loaded AST can carry a tag outside
  `{str, seq, map}` is via an *explicit* user tag in the
  source (covered by REQ-§10.1-7).

## REQ-§10.1-2 — All scalars resolve to `!!str` regardless of plain content

- **Spec requirement (§10.1.2 / §10.1.1.3):** Generic
  String tag — the failsafe schema is conservative: "all
  untagged scalars [are] strings rather than attempting to
  infer numeric or boolean types". Plain `42`, `null`,
  `true`, etc. must resolve to `tag:yaml.org,2002:str`,
  not `!!int`/`!!null`/`!!bool`.
- **Test method:** Loaded each of `42\n`, `3.14\n`,
  `null\n`, `~\n`, `true\n`, `TRUE\n`, `hello\n`, `yes\n`,
  `.inf\n`, `.nan\n`, `0o17\n`, `0xff\n`, `-1\n`, `1e10\n`
  under `Schema::Failsafe`.
- **Observed output:** All produce a Plain-style scalar
  with `tag = Some("tag:yaml.org,2002:str")`. The
  surface-level value text is preserved verbatim
  (`value="0xff"` etc., parser does not interpret).
- **Spec expectation:** `tag = !!str` for every plain
  scalar.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:130-131`
  (Failsafe arm returns `ResolvedTag::Str`
  unconditionally); behavioral confirmation across 14
  semantically-charged plain scalars.
- **Reasoning:** Cross-checking against
  `Schema::Core` for the same input `42\n` produces
  `tag:yaml.org,2002:int` — proving the Failsafe arm is
  schema-isolated and content-blind, exactly as §10.1
  requires.

## REQ-§10.1-3 — All untagged sequences resolve to `!!seq`

- **Spec requirement (§10.1.1.2):** Generic Sequence tag
  `tag:yaml.org,2002:seq` for ordered collections.
- **Test method:** Loaded `- a\n- 1\n- true\n` (block),
  `[a, 1, true]\n` (flow), and `[]\n` (empty flow) under
  `Schema::Failsafe`.
- **Observed output:** All three roots are `Sequence` with
  `tag = Some("tag:yaml.org,2002:seq")`. Empty sequence is
  no exception (`items=0`, tag still `!!seq`). Inner
  scalars all become `!!str` (REQ-§10.1-2).
- **Spec expectation:** Every untagged sequence
  (regardless of style or emptiness) gets `!!seq`.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:178-182`
  (`resolve_collection` returns `Seq` from
  `CollectionKind::Sequence`; `schema` argument
  intentionally discarded, behaviour identical across all
  three schemas); `loader.rs:1051-1063` applies on
  `Node::Sequence` construction.
- **Reasoning:** No content-dependent branching exists in
  the sequence-resolution path.

## REQ-§10.1-4 — All untagged mappings resolve to `!!map`

- **Spec requirement (§10.1.1.1):** Generic Mapping tag
  `tag:yaml.org,2002:map` for unordered key-value
  associations.
- **Test method:** Loaded `a: 1\nb: 2\n` (block),
  `{a: 1, b: 2}\n` (flow), `{}\n` (empty), and a nested
  mixed structure with both inner sequences and inner
  mappings.
- **Observed output:** All four roots are `Mapping` with
  `tag = Some("tag:yaml.org,2002:map")`. Empty mapping
  carries `!!map` (`entries=0`). Inner mappings in the
  nested case (`b: { c: 3 }`) likewise carry `!!map`.
- **Spec expectation:** Every untagged mapping
  (regardless of style or emptiness) gets `!!map`.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:178-182`
  (returns `Map` from `CollectionKind::Mapping`);
  `loader.rs:1035-1049` applies on `Node::Mapping`
  construction.
- **Reasoning:** Same constant-function argument as
  REQ-§10.1-3 — `kind` alone determines the tag.

## REQ-§10.1-5 — Plain and quoted scalars resolve identically (both → `!!str`)

- **Spec requirement (§10.1):** Failsafe makes no
  scalar-style distinction — quoted and plain scalars are
  both strings.
- **Test method:** Loaded the same value under three
  styles: `x\n` (plain), `"x"\n` (double-quoted), `'x'\n`
  (single-quoted). Also probed semantically-charged
  values: `"42"\n`, `'true'\n`, `"null"\n`. Also probed
  block-scalar styles: `|\n  hello\n`, `>\n  hello\n`,
  `|\n  42\n`.
- **Observed output:** Every style — Plain,
  SingleQuoted, DoubleQuoted, Literal(Clip),
  Folded(Clip) — yields tag `tag:yaml.org,2002:str`. The
  `style` field of the AST node distinguishes the source
  presentation, but the resolved tag is identical.
- **Spec expectation:** Style-independent resolution to
  `!!str`.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:130-131`
  — the Failsafe arm in `resolve_scalar` is unconditional
  `Ok(Some(ResolvedTag::Str))`; `style` argument is not
  inspected on this path.
- **Reasoning:** The Core arm at `schema.rs:133-143`
  branches on `style` to dispatch plain vs non-plain; the
  Failsafe arm has no such branch. This is the structural
  guarantee §10.1 requires.

## REQ-§10.1-6 — `!` non-specific tag resolves by kind under Failsafe

- **Spec requirement (§10.1.2 / §3.3.2):** "All [nodes]
  with the `!` non-specific tag are [resolved], by the
  standard [convention], to `tag:yaml.org,2002:seq`,
  `tag:yaml.org,2002:map` or `tag:yaml.org,2002:str`,
  according to their [kind]."
- **Test method:** Loaded `! 42\n`, `! [a, b]\n`,
  `! {a: 1}\n`, `! "42"\n`, and the nested form
  `! [! a, ! b]\n` under `Schema::Failsafe`.
- **Observed output:** `! 42\n` → scalar tagged
  `!!str`. `! [a, b]\n` → sequence tagged `!!seq` with
  inner scalars `!!str`. `! {a: 1}\n` → mapping tagged
  `!!map`. `! "42"\n` (bare `!` on a quoted scalar) →
  scalar tagged `!!str`. Nested `! [! a, ! b]\n` →
  outer `!!seq`, inner items both `!!str`.
- **Spec expectation:** `!` resolves to the kind-matched
  tag from the trio; never produces `Null`/`Int`/`Bool`/etc.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/loader.rs:1010-1013`
  (scalar branch — `tag.as_deref() == Some("!")` →
  unconditional rewrite to `ResolvedTag::Str` regardless
  of content or schema, which collapses into the same
  `!!str` outcome the Failsafe schema would produce
  anyway); `loader.rs:1038` and `loader.rs:1052`
  (collection branches — `effective_tag = tag.filter(|t|
  *t != "!")` causes the `!` source-tag to be erased
  before `resolve_collection`, which then treats the node
  as untagged and produces `!!seq`/`!!map`).
- **Reasoning:** All three kinds correctly produce one of
  the three Failsafe tags. The implementation correctly
  distinguishes "bare `!`" from "explicit foreign tag":
  the former is normalised to a Failsafe tag, the latter
  passes through.

## REQ-§10.1-7 — Explicit non-failsafe tags pass through unmodified

- **Spec requirement (§10.4):** "None of the above
  recommended [schemas] preclude the use of arbitrary
  explicit [tags]." The Failsafe schema specifies
  *resolution* of unresolved nodes; explicit tags in the
  source are not subject to schema resolution.
- **Test method:** Loaded `!!int 42\n`, `!!bool true\n`,
  `!!null ~\n`, `!foo bar\n`, `!foo [a,b]\n`,
  `!foo {a:1}\n` under `Schema::Failsafe`.
- **Observed output:** `!!int` shorthand expands to
  `tag:yaml.org,2002:int` and is preserved (`tag =
  Some("tag:yaml.org,2002:int")`) — Failsafe does not
  rewrite it. Likewise `!!bool` → `tag:yaml.org,2002:bool`,
  `!!null` → `tag:yaml.org,2002:null`. Local tag `!foo`
  passes through verbatim on scalars (`tag = Some("!foo")`)
  and collections.
- **Spec expectation:** Schema resolution applies only to
  unresolved nodes (the `?` non-specific tag). Explicit
  tags survive unchanged.
- **Verdict:** Strict-conformant — but this is a *neutral*
  observation, not a §10.1 requirement satisfied.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:125-128`
  (`if source_tag.is_some() return Ok(None)` — explicit
  tags short-circuit resolution). `schema.rs:174-176`
  (same for collections). `loader.rs:1014-1015`
  (resolve_scalar called with `tag.as_deref()` so any
  non-`!` tag suppresses resolver overwrite).
- **Reasoning:** The Failsafe schema "uses only three
  tags" for *resolution*. Source-level explicit tags are
  not part of the schema's domain — they are the user's
  declaration and remain in the AST. This matches the
  spec's distinction between schema-resolved tags and
  user-authored tags. (Architectural observation: an
  application that wants to enforce "only the three
  Failsafe tags" must walk the AST and reject foreign
  tags itself; the parser does not — and is not required
  to — emit a diagnostic in this case.)

## REQ-§10.1-8 — Schema selection is per-loader and Failsafe is selectable

- **Spec requirement (§10):** YAML processors offer
  recommended schemas as alternatives — Failsafe must be
  available even if not the default.
- **Test method:** Constructed a loader via
  `LoaderBuilder::new().lossless().schema(Schema::Failsafe).build()`
  and verified its output for `42\n` differs from the
  default loader's output for the same input.
- **Observed output:** `Schema::Failsafe` produces
  `tag:yaml.org,2002:str` for `42\n`; the default
  (`Schema::Core`) produces `tag:yaml.org,2002:int` for
  the same input. The two configurations coexist in the
  same process and do not interfere.
- **Spec expectation:** Failsafe is one of three
  selectable schemas.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:25-36`
  (`enum Schema { Failsafe, Json, Core }` —
  `Failsafe` is a first-class variant);
  `loader.rs:181-185` (`pub schema: Schema` field on
  `LoaderOptions`); `loader.rs:265-268`
  (`LoaderBuilder::schema(Schema::Failsafe)` selector);
  `loader.rs:188-198` (`Default for LoaderOptions` sets
  `Schema::Core`, so Failsafe must be opted into — but it
  *is* opt-in-able). Public re-export at
  `lib.rs:30` (`pub use schema::{ResolvedTag, Schema};`).
- **Reasoning:** Default of `Core` is permissible per the
  YAML 1.2.2 specification (which says "It is strongly
  recommended that such [schemas] be based on the [core
  schema]"). The Failsafe schema is reachable through the
  builder API and produces correct §10.1 behaviour.

## Architectural observations (not §10.1 requirements)

- **Schema-resolved tag tracking.** The loader clears
  `tag_loc` on schema-injected tags (`loader.rs:1018-1024`,
  `1043-1048`, `1057-1062`) but preserves it for
  user-authored tags including bare `!`
  (`loader.rs:1006-1009`). This is correct for round-trip
  formatters but is not a §10.1 requirement.
- **No diagnostic for foreign explicit tags under
  Failsafe.** The spec's "uses only three tags" applies to
  *resolution*. Whether a Failsafe-mode loader should
  warn on encountering `!!int`, `!!bool`, etc. in the
  source is unspecified by §10.1. Per the "should is
  non-mandatory" precedent, the parser's silent
  passthrough is permissible.
- **`!!int`/`!!bool`/`!!null` shorthand always resolves
  via the active tag-handle directive scope, not the
  schema.** This means a user can write tags whose URIs
  are outside `{str,seq,map}` even under Failsafe — they
  travel through the AST untouched. This is semantically
  correct (the user opted into a foreign tag) and does
  not violate §10.1.
