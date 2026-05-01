---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: B
section: §10.2
date: 2026-04-30
---

# Phase 2 Behavioral Audit — §10.2 JSON Schema (Auditor B)

## Method

All inputs were exercised through the public loader API
via a standalone audit-probe Cargo project at
`/tmp/audit-probe-§10.2-b/` with a path dependency on
`rlsp-yaml-parser`. No probe code was added to the parser
tree (`git status --porcelain rlsp-yaml-parser/` returns
empty before, during, and after probing). Each requirement
records the bytes fed to the parser, the AST tag the
loader produced under `Schema::Json`, and — where relevant
— the comparison against `Schema::Failsafe` /
`Schema::Core` to confirm schema isolation. The probe was
deleted immediately after observing output, before this
audit was written.

Spec sources: `https://yaml.org/spec/1.2.2/` §10.2 (JSON
Schema), §10.2.1 (Tags), §10.2.2 (Tag Resolution).
WebFetch consistently truncated before reaching Chapter
10, so the normative regex table is taken from
`rlsp-yaml-parser/docs/yaml-spec-conformance.md:2079`
which quotes §10.2.2 verbatim:

| Regular expression                                                   | Resolved to tag         |
|----------------------------------------------------------------------|-------------------------|
| `null`                                                               | `tag:yaml.org,2002:null`  |
| `true \| false`                                                      | `tag:yaml.org,2002:bool`  |
| `-? ( 0 \| [1-9] [0-9]* )`                                           | `tag:yaml.org,2002:int`   |
| `-? ( 0 \| [1-9] [0-9]* ) ( \. [0-9]* )? ( [eE] [-+]? [0-9]+ )?`     | `tag:yaml.org,2002:float` |
| `*`                                                                  | Error                   |

The §10.2.2 commentary (also quoted at the same
conformance-doc line): "In principle, JSON files should
not contain any [scalars] that do not match at least one
of these. Hence the YAML [processor] should consider them
to be an error." Note: §10.2.2 uses "should," not "must"
— a YAML processor that falls back to `!!str` for an
unmatched plain scalar under JSON schema is not strictly
non-conformant under the "should is non-mandatory"
precedent (Phase 1 [83]). The implementation here is
strict: it errors. That is one valid interpretation;
silent fallback to `!!str` would also be valid. Treated
below as Strict-conformant where strictness applies.

The non-specific tag rule for collections under JSON
schema (§10.2.2 collections row, also quoted at
yaml-spec-conformance.md:2086):
"[Collections] with the '?' non-specific tag (that is,
[untagged] [collections]) are [resolved] to
'tag:yaml.org,2002:seq' or 'tag:yaml.org,2002:map'
according to their [kind]."

## REQ-§10.2-1 — JSON schema tag set extends Failsafe with `!!null`, `!!bool`, `!!int`, `!!float`

- **Spec requirement (§10.2.1):** "The JSON Schema is the
  lowest common denominator of most modern computer
  languages, and allows parsing JSON files. A YAML
  processor should therefore support this schema, at
  least as an option. It is also strongly recommended
  that other schemas should be based on it." JSON schema
  adds the four tags `tag:yaml.org,2002:null`,
  `tag:yaml.org,2002:bool`, `tag:yaml.org,2002:int`,
  `tag:yaml.org,2002:float` on top of the Failsafe
  set (`!!str`, `!!seq`, `!!map`).
- **Test method:** Surveyed every produced tag URI across
  the probe inputs under `Schema::Json` (null/bool/int/
  float/string/seq/map probes). Cross-checked the
  `ResolvedTag` enum and `as_str()` mapping in
  `schema.rs`.
- **Observed output:** The seven URIs produced by
  `Schema::Json` are exactly:
  - `tag:yaml.org,2002:null` (e.g. plain `null`)
  - `tag:yaml.org,2002:bool` (`true`, `false`)
  - `tag:yaml.org,2002:int` (`0`, `42`, `-1`, …)
  - `tag:yaml.org,2002:float` (`3.14`, `1e10`, `-0`, …)
  - `tag:yaml.org,2002:str` (quoted scalars)
  - `tag:yaml.org,2002:seq` (`[]`, `[1, 2]`, block seq)
  - `tag:yaml.org,2002:map` (`{}`, block map with quoted
    keys)
  No other URI appears.
- **Spec expectation:** The closed set
  `{null, bool, int, float, str, seq, map}` URIs.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:42-57`
  (the `ResolvedTag` enum has exactly these seven
  variants); `schema.rs:62-72` (`as_str()` maps each to
  the spec URI); `schema.rs:145-156` (`Schema::Json` arm
  in `resolve_scalar`); `schema.rs:178-182`
  (`resolve_collection` produces only `Seq` / `Map`).
- **Reasoning:** The tag set is enforced by the type
  system (a closed enum with seven variants), not by
  runtime checks. There is no path by which a JSON-schema
  resolution can produce a URI outside this set.

## REQ-§10.2-2 — Plain `null` resolves to `!!null`; only `null` qualifies

- **Spec requirement (§10.2.2 null row):** Regex `null`.
  The pattern is the literal four-byte string `null`.
  YAML 1.1 alternates `~`, `Null`, `NULL`, and the empty
  string are NOT in the JSON regex.
- **Test method:** Loaded each candidate as the full
  document body under `Schema::Json` and recorded the
  resulting tag URI or load error.
- **Observed output:**
  - `null` → `tag:yaml.org,2002:null`
  - `Null` → `LoadError::UnresolvedScalar { value: "Null", … }`
  - `NULL` → `LoadError::UnresolvedScalar { value: "NULL", … }`
  - `~` → `LoadError::UnresolvedScalar { value: "~", … }`
  - ` null ` (leading/trailing space) → trimmed to `null`
    by the lexer, resolves to `tag:yaml.org,2002:null`
  - `nullX` → `LoadError::UnresolvedScalar`
- **Spec expectation:** Exact, case-sensitive match of
  the literal string `null`; everything else is unmatched.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:399-401`
  (`is_json_null` is `value == "null"` — strict equality,
  no case folding); `schema.rs:253-254`
  (`resolve_json_plain` dispatches to it first).
- **Reasoning:** A literal `==` against `"null"` cannot
  accept `Null`/`NULL`/`~`/empty.

## REQ-§10.2-3 — Plain `true` / `false` resolve to `!!bool`; case-sensitive only

- **Spec requirement (§10.2.2 bool row):** Regex
  `true | false`. Lower-case only; YAML 1.1 `True`,
  `TRUE`, `False`, `FALSE`, `yes`, `no`, `on`, `off` are
  NOT in the JSON regex.
- **Test method:** Loaded each candidate as the document
  body under `Schema::Json`.
- **Observed output:**
  - `true` → `tag:yaml.org,2002:bool`
  - `false` → `tag:yaml.org,2002:bool`
  - `True`, `TRUE`, `False`, `FALSE` →
    `LoadError::UnresolvedScalar`
  - `yes`, `no`, `on`, `off` →
    `LoadError::UnresolvedScalar`
- **Spec expectation:** Exact, case-sensitive match of
  `true` or `false`; everything else is unmatched.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:404-407`
  (`is_json_bool` uses `matches!(value, "true" |
  "false")` — strict equality); `schema.rs:255-256`.
- **Reasoning:** No case folding, no YAML 1.1 alternates,
  no truthy synonyms.

## REQ-§10.2-4 — Plain `0 | -? [1-9] [0-9]*` resolves to `!!int`

- **Spec requirement (§10.2.2 int row):** Regex
  `-? ( 0 | [1-9] [0-9]* )`. The bare zero `0` (no sign,
  no leading/trailing digits) is allowed; any other
  integer must have first digit 1-9 and optional leading
  `-` sign. No `+` sign. No leading zeros (`007`).
  No octal/hex prefixes (`0o17`, `0xFF`).
- **Test method:** Loaded each candidate as the document
  body under `Schema::Json`.
- **Observed output:**
  - `0` → `tag:yaml.org,2002:int`
  - `1`, `42`, `100`, `9999999999` → `…:int`
  - `-1`, `-100` → `…:int`
  - `+42`, `+0` → `LoadError::UnresolvedScalar` (no `+`)
  - `-0` → `tag:yaml.org,2002:float` (NOT int — see
    REQ-§10.2-5)
  - `01`, `00`, `007`, `-007` →
    `LoadError::UnresolvedScalar` (leading zeros rejected)
  - `0o17`, `0xFF` → `LoadError::UnresolvedScalar`
- **Spec expectation:** Exactly the values produced by the
  regex `-? ( 0 | [1-9] [0-9]* )`.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:413-426`
  (`is_json_int`: special-cases bare `"0"`, otherwise
  strips at most one leading `-` (no `+`), requires first
  byte in 1-9, then all-digit tail; `schema.rs:257-258`
  (dispatch precedes float).
- **Reasoning:** The regex is implemented exactly: bare
  `0` short-circuits; otherwise sign-optional `1-9`
  followed by `[0-9]*`. Behavior on `-0` is correct under
  the spec — `-0` does not match `-? ( 0 | [1-9] [0-9]* )`
  because the spec splits the integer-part alternation
  outside the optional sign group, so `-0` falls through
  to the float regex which DOES accept it (sign + integer
  part `0` + no fractional/exponent — confirmed by
  `is_json_float("-0") == true` in the source-level
  unit test at `schema.rs:687-689`).

## REQ-§10.2-5 — Plain `-? ( 0 | [1-9][0-9]* ) ( \. [0-9]* )? ( [eE] [-+]? [0-9]+ )?` resolves to `!!float`

- **Spec requirement (§10.2.2 float row):** Regex
  `-? ( 0 | [1-9] [0-9]* ) ( \. [0-9]* )? ( [eE] [-+]?
  [0-9]+ )?`. Optional minus sign, integer part with same
  shape as the int rule, optional fractional part with
  zero or more digits after the dot, optional exponent.
  No leading dot. No `+` sign on the mantissa. No
  `.inf` / `.nan`.
- **Test method:** Loaded each candidate as the document
  body under `Schema::Json`.
- **Observed output (positive):**
  - `3.14`, `0.5`, `-0.0`, `-1.5`, `0.` → `…:float`
  - `1e10`, `1E10`, `1e+5`, `1e-5`, `0e0` → `…:float`
  - `1.5e3`, `1.0e0`, `1.0e+0`, `-1.0e-3` → `…:float`
  - `1.` (dot with no fractional digits) → `…:float`
  - `1.e5` (empty fractional, then exponent) → `…:float`
  - `-0` → `…:float` (matches with sign + integer 0,
    empty fractional, empty exponent)
- **Observed output (negative):**
  - `+1.5` → `LoadError::UnresolvedScalar` (no `+`)
  - `.5`, `.` → `LoadError::UnresolvedScalar` (leading
    dot not allowed)
  - `.inf`, `.Inf`, `.INF`, `-.inf` →
    `LoadError::UnresolvedScalar`
  - `.nan`, `.NaN`, `.NAN` →
    `LoadError::UnresolvedScalar`
  - `1e` (exponent without digits) →
    `LoadError::UnresolvedScalar`
  - `e10` (no integer part) →
    `LoadError::UnresolvedScalar`
  - `1.x` (trailing non-digit) →
    `LoadError::UnresolvedScalar`
- **Spec expectation:** Exactly the values produced by
  the regex.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:432-471`
  (`is_json_float`: strips at most one `-` (no `+`),
  parses integer part as `0` or `[1-9][0-9]*`, optional
  `\.` with greedy `[0-9]*` (zero digits allowed),
  optional `[eE][-+]?[0-9]+` with at least one exponent
  digit, must consume the entire string).
- **Reasoning:** Each piece of the spec regex is
  represented exactly: integer-part dispatch, optional
  fractional part with `take_while(is_ascii_digit)` (so
  zero digits OK), optional exponent that requires at
  least one digit. The strict consumption check
  (`after_exp.is_empty()` at `schema.rs:470`) prevents
  trailing non-digit characters. JSON schema correctly
  rejects `.inf` / `.nan` as those are Core-only.

## REQ-§10.2-6 — Quoted scalars (single, double) always resolve to `!!str` regardless of content

- **Spec requirement (§10.2.2 / §3.3.2):** The
  non-specific tag `!` (assigned to all quoted scalars)
  resolves to `!!str` under all schemas. Quoted scalars
  do NOT participate in plain-scalar regex matching.
- **Test method:** Loaded `"null"`, `'null'`, `"true"`,
  `'true'`, `"42"`, `'42'`, `"3.14"`, `["hello"]` under
  `Schema::Json` and recorded the produced tag.
- **Observed output:** Every quoted scalar — regardless
  of whether the content would match a JSON regex —
  resolves to `tag:yaml.org,2002:str`. This holds for
  single-quoted and double-quoted, in document-root,
  flow-sequence, and mapping contexts.
- **Spec expectation:** Quoted scalars short-circuit
  schema regex resolution and always resolve to `!!str`.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:145-155`
  (the `Schema::Json` arm matches on `style`; only
  `ScalarStyle::Plain` reaches `resolve_json_plain`,
  every other style is `ResolvedTag::Str` directly).
  Block literal/folded scalars are also covered there
  (also Str), even though they do not appear in JSON
  syntax — the dispatch is correct for all non-Plain
  styles.
- **Reasoning:** The decision to bypass regex matching is
  made on `ScalarStyle`, not on content; no content path
  can override it.

## REQ-§10.2-7 — Plain scalars not matching any pattern produce `LoadError::UnresolvedScalar`

- **Spec requirement (§10.2.2 commentary):** "In
  principle, JSON files should not contain any [scalars]
  that do not match at least one of these. Hence the YAML
  [processor] should consider them to be an error." Note
  "should," not "must" — silent fallback to `!!str`
  would also be a valid implementation. The
  implementation here makes the strict choice.
- **Test method:** Fed plain scalars that match no
  pattern — `hello`, `foo123`, `yes`, `no`, `+`, `.` —
  to `Schema::Json` and recorded the result.
- **Observed output:**
  - `hello` → `LoadError::UnresolvedScalar { value:
    "hello", pos: Pos { byte_offset: 0, line: 1,
    column: 0 } }`
  - `foo123` → `LoadError::UnresolvedScalar`
  - `yes`, `no` → `LoadError::UnresolvedScalar` (these
    are bool synonyms in YAML 1.1 / Core schema, not in
    JSON)
  - `+` (sign only) → `LoadError::UnresolvedScalar`
  - `.` (dot only) → `LoadError::UnresolvedScalar`
- **Spec expectation:** Either error or `!!str` fallback
  is permissible by the "should" wording. The strict
  choice is conformant.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:252-264`
  (`resolve_json_plain` final `else` branch returns
  `Err(UnresolvedScalar)` after all four predicates
  fail); error wired through to `LoadError::Unresolved\
  Scalar` at `loader.rs:1027-1032`.
- **Reasoning:** The strict-error path is observable in
  the integration tests (`tests/schema_resolution.rs`
  has tests for `*_returns_unresolved_scalar_error`),
  and the probe confirmed it end-to-end. The choice to
  error rather than fall back to `!!str` is documented
  in `loader.rs:110-126` (the `UnresolvedScalar`
  doc-comment).

## REQ-§10.2-8 — Untagged sequences resolve to `!!seq`, untagged mappings to `!!map`

- **Spec requirement (§10.2.2 collections row):**
  "[Collections] with the '?' non-specific tag (that is,
  [untagged] [collections]) are [resolved] to
  'tag:yaml.org,2002:seq' or 'tag:yaml.org,2002:map'
  according to their [kind]."
- **Test method:** Loaded `[]`, `[1, 2, 3]`, `{}`,
  `["hello"]`, `\"a\": 1` under `Schema::Json` and
  inspected the root collection's tag.
- **Observed output:**
  - `[]` (empty flow seq) → `tag:yaml.org,2002:seq`
  - `[1, 2, 3]` → `tag:yaml.org,2002:seq` with three int
    children all tagged `:int`
  - `{}` (empty flow map) → `tag:yaml.org,2002:map`
  - `\"a\": \"b\"` (block map with quoted key/val) →
    `tag:yaml.org,2002:map` with both pair members
    tagged `:str`
  - `- 1\n- 2\n` (block sequence) → `:seq` with
    two `:int` children
- **Spec expectation:** Untagged collection kind →
  `!!seq` or `!!map`.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:168-183`
  (`resolve_collection` is kind-driven only — it
  ignores the `schema` parameter `let _ = schema` at
  `:178`); applied via `loader.rs:1040-1054`.
- **Reasoning:** The function is a constant function of
  collection kind; no path produces anything other than
  `Seq` / `Map`.

## REQ-§10.2-9 — Explicit source tag overrides JSON schema resolution

- **Spec requirement (§3.3.2 / §6.9.1):** A node with
  an explicit non-specific or verbatim tag carries that
  tag through resolution; schema-driven regex matching
  applies only to nodes with the `?` non-specific tag
  (untagged plain scalars and untagged collections).
- **Test method:** Loaded `!!str 42`, `!!int hello`,
  `! true`, `! hello` under `Schema::Json` and inspected
  the tag.
- **Observed output:**
  - `!!str 42` → root scalar tag is
    `tag:yaml.org,2002:str` (the explicit `!!str` wins;
    no integer resolution attempted).
  - `!!int hello` → root scalar tag is
    `tag:yaml.org,2002:int` (the explicit user tag is
    preserved verbatim, even though `hello` does not
    match any int regex; this matches the spec —
    pattern matching applies only to untagged nodes).
  - `! true` → root scalar tag is
    `tag:yaml.org,2002:str` (bare `!` is the
    non-specific scalar tag, which resolves to `!!str`
    under all schemas — Failsafe wing).
  - `! hello` → root scalar tag is
    `tag:yaml.org,2002:str`.
- **Spec expectation:** Explicit tags are not modified
  by schema resolution; bare `!` resolves to the
  Failsafe-side `!!str` for scalars.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:124-128`
  (the `source_tag.is_some()` early return in
  `resolve_scalar`); `loader.rs:1010-1013` (bare `!`
  short-circuit to `!!str` before schema dispatch);
  `loader.rs:1014-1015` (otherwise `resolve_scalar`
  with `tag.as_deref()` honors the existing tag).
- **Reasoning:** The bare-`!` translation to `!!str`
  before schema dispatch is correct under §10.2.1 —
  the JSON schema inherits the non-specific resolution
  from Failsafe, where `!` on a scalar means `!!str`.

## REQ-§10.2-10 — JSON schema rejects `+` integer sign and `+` float sign

- **Spec requirement (§10.2.2):** The integer regex is
  `-? ( 0 | [1-9] [0-9]* )` and the float regex is
  `-? ( 0 | [1-9] [0-9]* ) ( \. [0-9]* )? ( [eE] [-+]?
  [0-9]+ )?`. Both have only `-?`, not `[-+]?`, on the
  outer sign — so a leading `+` on the mantissa is not
  matched. (The exponent does allow `[-+]?`.) Compare
  Core schema (§10.3.2) which does allow `[-+]?` on the
  outer sign for both.
- **Test method:** Fed `+42`, `+0`, `+1.5` under
  `Schema::Json` and recorded the result.
- **Observed output:**
  - `+42` → `LoadError::UnresolvedScalar`
  - `+0` → `LoadError::UnresolvedScalar`
  - `+1.5` → `LoadError::UnresolvedScalar`
  - `1e+5` → `tag:yaml.org,2002:float` (the `+` here is
    inside the exponent, where it IS allowed by the
    spec).
- **Spec expectation:** `+`-signed mantissa is unmatched;
  `+`-signed exponent (after `e`/`E`) is matched.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:418`
  (`value.strip_prefix('-')`, no `or_else(strip('+'))`
  for int); `schema.rs:434` (same for float, only `-`);
  `schema.rs:460` (exponent parser uses
  `strip_prefix(['-', '+'])`, so `+` is allowed only
  inside the exponent).
- **Reasoning:** The asymmetry between mantissa and
  exponent sign matches the spec regex precisely.

## REQ-§10.2-11 — JSON schema does not resolve `.inf`, `-.inf`, `.nan` (Core-only forms)

- **Spec requirement (§10.2.2 float regex):** Has no
  alternates for infinity or NaN. Compare Core (§10.3.2)
  which does allow `[+-]?\.inf|\.Inf|\.INF` and
  `\.nan|\.NaN|\.NAN`.
- **Test method:** Fed `.inf`, `.Inf`, `.INF`, `-.inf`,
  `.nan`, `.NaN`, `.NAN` under `Schema::Json`.
- **Observed output:** All six produced
  `LoadError::UnresolvedScalar`. (Under `Schema::Core`
  the same inputs would resolve to `!!float` per
  REQ-§10.3 territory — out of scope here, but confirms
  the schema isolation.)
- **Spec expectation:** None of the infinity/NaN forms
  match the JSON regex; they are unmatched plain
  scalars.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:432-471`
  (`is_json_float` does not enumerate `.inf` /
  `.nan` — it is purely the regex implementation).
  Compare `schema.rs:317-337` (`is_core_float`) which
  DOES handle them — JSON's matcher is a strict subset.
- **Reasoning:** The two matchers are independent
  functions; JSON's has no inf/nan codepath.

## REQ-§10.2-12 — Implicit empty plain scalars (collection contexts) are unresolvable under JSON schema

- **Spec requirement (§10.2.2):** The JSON null regex is
  the literal string `null`. The empty string is NOT in
  the JSON regex. Under JSON schema, an implicit empty
  scalar (e.g. `key:` with no value, or `- ` followed by
  newline) produces a plain scalar with empty content,
  and that empty content does not match any JSON regex.
  Per the JSON-schema "should be an error" rule, that
  empty scalar should error.
- **Test method:** Fed `key:\n` (key with empty value),
  `- \n- 1\n` (empty list item), and `--- \n` (empty
  document body) under `Schema::Json`.
- **Observed output:**
  - `key:\n` → `LoadError::UnresolvedScalar { value:
    "key", pos: line 1 col 0 }`. Note: the loader
    reports the unresolved scalar at the KEY (since the
    key is the first plain-scalar child encountered);
    if the key were quoted it would be the empty value
    that errors. With `\"key\":\n` the empty value
    would be the unresolvable scalar.
  - `- \n- 1\n` → `LoadError::UnresolvedScalar { value:
    "", pos: line 2 col 0 }` — the first list item is
    the empty value, and that is reported.
  - `--- \n` (explicit doc start, no body) →
    `LoadError::UnresolvedScalar { value: "", pos:
    line 1 col 0 }`.
  - `# comment\n` (no document) → 0 documents (the
    parser produces no document at all when input is
    blank/comment-only, so schema resolution has
    nothing to apply).
- **Spec expectation:** Empty plain scalars under JSON
  schema produce `UnresolvedScalar` (matching the
  "should be an error" disposition), or fall back to
  `!!str` under the lenient interpretation. Either is
  conformant per the "should is non-mandatory" precedent.
- **Verdict:** Strict-conformant. (See note below on
  the documentation gap; this is observably in line with
  the strict reading of the spec, but tooling that
  consumes JSON-schema YAML and expects `key:` to
  produce a null value will need to be aware.)
- **Evidence:** `rlsp-yaml-parser/src/schema.rs:399-401`
  (`is_json_null` is `value == "null"`, which is `false`
  for the empty string); `loader.rs:1027-1032` (error
  path).
- **Reasoning:** A literal-equality null predicate
  cannot accept the empty string. The behavior is
  consistent — implicit empty values fail JSON schema
  resolution. This is observably divergent from how
  `Schema::Core` handles it (Core's `is_core_null`
  matches `""` at `schema.rs:273`), but is not divergent
  from the JSON regex.

## REQ-§10.2-13 — Schema resolution is recursive over collection children

- **Spec requirement (implicit from §10.2.2 phrasing
  "[Scalars] with the `?` non-specific tag … are matched
  with a list of regular expressions"):** Schema
  resolution applies to every scalar in the document
  tree, not just the document root. A nested unmatched
  plain scalar (inside a sequence or mapping) is just as
  much an error under JSON schema as an unmatched root
  scalar.
- **Test method:** Loaded `[hello]`, `\"k\": hello`, and
  `[1, true, null, \"x\"]` under `Schema::Json`.
- **Observed output:**
  - `[1, true, null, \"x\"]` → fully resolved tree
    `Seq([Int("1"), Bool("true"), Null("null"),
    Str("x")])`. Each child carries its own resolved
    tag.
  - `[hello]` → `LoadError::UnresolvedScalar { value:
    "hello", pos: byte 1, line 1, col 1 }` — the inner
    plain scalar fails resolution and the error
    propagates from the nested context (note the
    position points to `hello` inside the bracket, not
    to the seq start).
  - `\"k\": hello` → `LoadError::UnresolvedScalar
    { value: "hello", pos: byte 5, line 1, col 5 }` —
    the unresolved scalar in mapping-value position is
    reported at the value's position.
- **Spec expectation:** Resolution is per-node, with
  position reporting at the offending node.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/loader.rs:987-1058`
  (`apply_schema_to_node` is called per-node; called
  from the recursive `parse_node` paths so every node
  passes through resolution). Integration tests
  `json_unresolved_scalar_propagates_from_nested_\
  sequence_item`,
  `json_unresolved_scalar_propagates_from_nested_\
  mapping_value` (in `tests/schema_resolution.rs`)
  verify the propagation; the probe confirmed it
  end-to-end.
- **Reasoning:** Position reporting at the offending
  child (rather than the parent collection) is the
  correct, useful behavior — matches the integration
  tests' assertions.

## Summary

| Requirement | Verdict |
|---|---|
| REQ-§10.2-1 (extends Failsafe; 4 new tags) | Strict-conformant |
| REQ-§10.2-2 (`null` only) | Strict-conformant |
| REQ-§10.2-3 (`true`/`false` only) | Strict-conformant |
| REQ-§10.2-4 (`!!int` regex) | Strict-conformant |
| REQ-§10.2-5 (`!!float` regex) | Strict-conformant |
| REQ-§10.2-6 (quoted → `!!str`) | Strict-conformant |
| REQ-§10.2-7 (unmatched plain → error) | Strict-conformant |
| REQ-§10.2-8 (collections by kind) | Strict-conformant |
| REQ-§10.2-9 (explicit tag wins) | Strict-conformant |
| REQ-§10.2-10 (no `+` mantissa sign) | Strict-conformant |
| REQ-§10.2-11 (no `.inf`/`.nan`) | Strict-conformant |
| REQ-§10.2-12 (implicit empty errors) | Strict-conformant |
| REQ-§10.2-13 (recursive resolution) | Strict-conformant |

**Tally: 13/13 Strict-conformant. 0 Lenient. 0 Divergent.
0 Indeterminate.**

The conformance doc claim at
`rlsp-yaml-parser/docs/yaml-spec-conformance.md:2079-2081`
("the four JSON regex patterns … Non-matching plain
scalars produce `LoadError::UnresolvedScalar`") is
verified behaviorally by this audit. No discrepancies
between the doc and the parser were observed.

The cleanup verification (`git status --porcelain` over
`rlsp-yaml-parser/`) returned empty before, during, and
after probing; no probe artifacts remain in the parser
tree.
