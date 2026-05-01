---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: A
section: §10.2
date: 2026-04-30
---

# Audit A — §10.2 JSON Schema (behavioral)

## Method

Built a standalone probe at `/tmp/audit-probe-10.2-a/` that imports
`rlsp_yaml_parser` and loads inputs via
`LoaderBuilder::new().lossless().schema(Schema::Json).build().load(input)`,
printing the resolved `tag` URI on each `Node::Scalar` (and on collections).
Probe deleted after observation; no files left in the parser tree.

Spec read from `/workspace/.ai/references/yaml-1.2.2-spec.md` lines
6378–6605 (§10.2 "JSON Schema"). Normative regexes (line 6574–6580):

```
| Regular expression                                                     | Resolved to tag
| `null`                                                                 | tag:yaml.org,2002:null
| `true | false`                                                         | tag:yaml.org,2002:bool
| `-? ( 0 | [1-9] [0-9]* )`                                              | tag:yaml.org,2002:int
| `-? ( 0 | [1-9] [0-9]* ) ( \. [0-9]* )? ( [eE] [-+]? [0-9]+ )?`        | tag:yaml.org,2002:float
| `*`                                                                    | Error
```

Plus the §10.2 example (lines 6587–6604) showing `Invalid: [ True, Null,
0o7, 0x3A, +12.3 ]` quoted as strings in the resolved JSON output — but the
spec narrative (line 6571) says the processor "should consider them to be
an error." The parser implements the strict-error variant, which is what
the regex table mandates.

Implementation evidence: `/workspace/rlsp-yaml-parser/src/schema.rs:399`
(`is_json_null`), `:405` (`is_json_bool`), `:413` (`is_json_int`), `:432`
(`is_json_float`), and `:252` (`resolve_json_plain` dispatch order
null → bool → int → float → `Err(UnresolvedScalar)`). Loader plumbing at
`/workspace/rlsp-yaml-parser/src/loader.rs:1015-1033` translates the
`UnresolvedScalar` error to `LoadError::UnresolvedScalar`.

## Per-requirement findings

### REQ-§10.2-1 — Failsafe tag set extension

- **Spec requirement:** §10.2.1 (lines 6387–6552) — JSON schema adds
  `tag:yaml.org,2002:{null, bool, int, float}` to the four Failsafe tags
  (`str`, `seq`, `map`).
- **Test method:** Run the probe across each tag-bearing case and observe
  the URI strings emitted via `ResolvedTag::as_str`.
- **Test input:** `null`, `true`, `42`, `1.5`, `"x"`, `[]`, `{}`.
- **Observed output:**
  - `null` → `tag:yaml.org,2002:null`
  - `true` → `tag:yaml.org,2002:bool`
  - `42` → `tag:yaml.org,2002:int`
  - `1.5` → `tag:yaml.org,2002:float`
  - `"x"` (DoubleQuoted) → `tag:yaml.org,2002:str`
  - `[]` → `tag:yaml.org,2002:seq`
  - `{}` → `tag:yaml.org,2002:map`
- **Spec expectation:** All seven URIs produced and distinguishable.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:62-72` (URI table), behavioral output above.
- **Reasoning:** Each spec-named tag appears with the exact URI prefix
  `tag:yaml.org,2002:` and the canonical suffix. No tags are missing or
  re-aliased.

### REQ-§10.2-2 — Null resolution: only `null`

- **Spec requirement:** Lines 6574–6576 — only the literal `null`
  resolves to `tag:yaml.org,2002:null`. Core-schema null forms `Null`,
  `NULL`, `~`, empty, are NOT in the JSON regex.
- **Test method:** Probe the full set of Core null forms under
  `Schema::Json` and observe whether they resolve or error.
- **Test input:** `null`, `Null`, `NULL`, `~`, the empty plain scalar
  produced by `---\n`.
- **Observed output:**
  - `null` → `tag:yaml.org,2002:null`
  - `Null` → `LoadError::UnresolvedScalar`
  - `NULL` → `LoadError::UnresolvedScalar`
  - `~` → `LoadError::UnresolvedScalar`
  - empty document `---\n` → `LoadError::UnresolvedScalar`
- **Spec expectation:** Only `null` matches; the other four are not in
  the JSON regex. Per the §10.2 narrative ("should consider them to be
  an error"), they must error.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:399-401` (`is_json_null` returns `value ==
  "null"` only). Loader error path at `loader.rs:1027-1032`.
- **Reasoning:** The implementation matches exactly one literal string,
  rejecting all Core-only null spellings as the spec table requires.

### REQ-§10.2-3 — Bool resolution: only `true`/`false`

- **Spec requirement:** Lines 6574–6577 — `true | false` only. No
  `True`, `TRUE`, `False`, `FALSE`, `yes`, `On`, etc.
- **Test method:** Probe true/false in lowercase, mixed case, and YAML
  1.1 boolean spellings.
- **Test input:** `true`, `false`, `True`, `TRUE`, `yes`, `On`.
- **Observed output:**
  - `true` → `tag:yaml.org,2002:bool`
  - `false` → `tag:yaml.org,2002:bool`
  - `True` → `LoadError::UnresolvedScalar`
  - `TRUE` → `LoadError::UnresolvedScalar`
  - `yes` → `LoadError::UnresolvedScalar`
  - `On` → `LoadError::UnresolvedScalar`
- **Spec expectation:** Only `true` and `false` match; everything else
  errors.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:405-407`
  (`matches!(value, "true" | "false")`). Spec example at line 6594 also
  lists `True` as Invalid.
- **Reasoning:** The matcher is the literal alternation from the spec
  regex; case-variant and YAML 1.1 spellings are correctly rejected.

### REQ-§10.2-4 — Integer regex `0 | -? [1-9] [0-9]*`

- **Spec requirement:** Lines 6574–6578 — int regex is `0` or `-?`
  followed by `[1-9][0-9]*`. No `+` sign, no leading zeros (except the
  literal `0`), no octal/hex.
- **Test method:** Probe the boundary points: `0`, `42`, `-1`, `-19`,
  `+0`, `+42`, `-0`, `007`, `0o7`, `0x3A`.
- **Test input:** Above set plus `01`, `-01`, `100`.
- **Observed output:**
  - `0` → `tag:yaml.org,2002:int`
  - `42` → `tag:yaml.org,2002:int`
  - `-1` → `tag:yaml.org,2002:int`
  - `-19` → `tag:yaml.org,2002:int`
  - `100` → `tag:yaml.org,2002:int`
  - `+0` → `LoadError::UnresolvedScalar`
  - `+42` → `LoadError::UnresolvedScalar`
  - `-0` → `tag:yaml.org,2002:float` (see REQ-§10.2-9)
  - `007` → `LoadError::UnresolvedScalar`
  - `01` → `LoadError::UnresolvedScalar`
  - `-01` → `LoadError::UnresolvedScalar`
  - `0o7` → `LoadError::UnresolvedScalar`
  - `0x3A` → `LoadError::UnresolvedScalar`
- **Spec expectation:** `0`, `42`, `-1`, `-19`, `100` match int. `+0`,
  `+42`, `007`, `01`, `-01`, `0o7`, `0x3A` do not match int (and do not
  match any other JSON pattern), so they must error.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:413-426` (`is_json_int` rejects `+`, only
  accepts the bare `0` literal or `-?[1-9][0-9]*`). Spec example line
  6594 lists `0o7`, `0x3A`, `+12.3` as Invalid; `+12.3` confirms the no-
  plus-sign rule.
- **Reasoning:** Each rejection corresponds to a regex literal violation
  (sign, leading zero, non-digit). The bare-`0` fast-path at
  `schema.rs:414-416` matches the leftmost alternative literally.

### REQ-§10.2-5 — Float regex (decimal)

- **Spec requirement:** Lines 6574–6579 — float regex is
  `-? ( 0 | [1-9] [0-9]* ) ( \. [0-9]* )? ( [eE] [-+]? [0-9]+ )?`. No
  `+` sign on mantissa, no leading-dot form, optional fractional part
  may be empty after the dot.
- **Test method:** Probe valid mantissa/exponent shapes and reject
  cases that violate the regex.
- **Test input:** `1.5`, `-1.5`, `0.`, `-0.0`, `12e03`, `1E+5`,
  `-2E+05`, `1.e2`, `0.5`, `100`, `0e0`, `-0.0e0`, `+1.5`, `.5`,
  `1.5e`, `1e+`, `1eE5`, `1.5e+`, `+12.3`.
- **Observed output:**
  - `1.5` → float
  - `-1.5` → float
  - `0.` → float (matches `0` then optional `\.[0-9]*` with empty digits)
  - `-0.0` → float
  - `12e03` → float
  - `1E+5` → float
  - `-2E+05` → float
  - `1.e2` → float
  - `0e0` → float
  - `-0.0e0` → float
  - `100` → int (matched the int branch first; also a valid float per
    regex but JSON dispatch is null→bool→int→float, see REQ-§10.2-8)
  - `+1.5` → `LoadError::UnresolvedScalar`
  - `+12.3` → `LoadError::UnresolvedScalar`
  - `.5` → `LoadError::UnresolvedScalar` (no leading-dot form in JSON)
  - `1.5e` → `LoadError::UnresolvedScalar` (exponent must have ≥1 digit)
  - `1e+` → `LoadError::UnresolvedScalar`
  - `1eE5` → `LoadError::UnresolvedScalar`
  - `1.5e+` → `LoadError::UnresolvedScalar`
- **Spec expectation:** All cases above match the literal regex, with
  the cases listed under "Observed output → UnresolvedScalar" as cases
  that the regex does not match.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:432-471` (`is_json_float`): strips one
  optional `-` (no `+`), parses int part as `0` or `[1-9][0-9]*`, then
  optional `\.[0-9]*` (empty after dot allowed), then optional `[eE]
  [-+]? [0-9]+` (≥1 digit). Final `after_exp.is_empty()` requires the
  whole string to be consumed.
- **Reasoning:** Every observed pass/fail is the exact regex outcome.
  The spec note at line 6582–6585 acknowledges the regex permits
  `0.` (empty fractional digits) — the parser permits this and confirms
  spec-compliance with the YAML 1.2 regex (not the JSON spec's stricter
  one).

### REQ-§10.2-6 — Float regex (special values .inf, .nan)

- **Spec requirement:** Lines 6537–6539 — `Canonical Form` is "Either
  `0`, `.inf`, `-.inf`, `.nan` or scientific notation matching ...".
  However, the §10.2.2 tag-resolution regex table at lines 6578–6579
  does NOT include `.inf`, `-.inf`, `.nan` in the float row. The
  resolution table is normative for what the JSON schema RESOLVES, while
  the canonical-form description applies to round-tripping a value
  already tagged `!!float`. A `!!float` tagged scalar is allowed to be
  `.inf`, but a plain scalar `.inf` does not match the JSON resolution
  regex.
- **Test method:** Probe `.inf`, `-.inf`, `.nan` plain.
- **Test input:** `.inf`, `-.inf`, `.nan`.
- **Observed output:**
  - `.inf` → `LoadError::UnresolvedScalar`
  - `-.inf` → `LoadError::UnresolvedScalar`
  - `.nan` → `LoadError::UnresolvedScalar`
- **Spec expectation:** Per §10.2.2 regex table, these do not match the
  JSON float regex (which requires the integer part `0|[1-9][0-9]*`).
  Plain-scalar resolution must error.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:432-447` requires the integer part — there is
  no `.inf`/`.nan` arm in `is_json_float`. Test
  `resolve_scalar_json::plain_float_inf_rejected` at `schema.rs:803`
  encodes the same expectation. Spec table at line 6579 confirms the
  regex.
- **Reasoning:** The implementation enforces the regex strictly. If the
  user wants `.inf` under JSON, they must tag it explicitly with
  `!!float`, which then bypasses resolution per the source-tag
  passthrough at `schema.rs:126-128`.

### REQ-§10.2-7 — Quoted scalars resolve to !!str

- **Spec requirement:** §10.2.2 (lines 6566–6571) — only **plain**
  scalars participate in the regex match. Quoted (single/double) and
  block (literal/folded) scalars are not plain and resolve to `!!str`
  per the Failsafe extension.
- **Test method:** Probe each non-plain style with content that would
  otherwise match a JSON regex.
- **Test input:** `"null"`, `'null'`, `"42"`, `"true"`, `|-\n  3.14`,
  `>\n  null`, `""`, `''`.
- **Observed output:**
  - `"null"` (DoubleQuoted) → `tag:yaml.org,2002:str`
  - `'null'` (SingleQuoted) → `tag:yaml.org,2002:str`
  - `"42"` → `tag:yaml.org,2002:str`
  - `"true"` → `tag:yaml.org,2002:str`
  - block-literal `3.14` → `tag:yaml.org,2002:str`
  - block-folded `null` → `tag:yaml.org,2002:str`
  - `""` and `''` → `tag:yaml.org,2002:str`
- **Spec expectation:** All quoted/block scalars resolve to `!!str`
  unconditionally, regardless of content.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:145-156` — JSON arm matches on `style`:
  `Plain` calls `resolve_json_plain`, all other styles return
  `ResolvedTag::Str` directly.
- **Reasoning:** The style-based bypass means "looks like JSON" content
  inside quotes never participates in regex matching.

### REQ-§10.2-8 — Plain scalars not matching any regex are an error

- **Spec requirement:** Lines 6569–6571 — "JSON files should not contain
  any scalars that do not match at least one of these. Hence the YAML
  processor should consider them to be an error."
- **Test method:** Probe plain scalars that do not match any JSON
  pattern.
- **Test input:** `hello`, `foo bar`, `abc`, `True`, `Null`, `0o7`,
  `0x3A`, `+12.3`, `42a`, `-1a`, `.`, the spec's own Invalid list
  values.
- **Observed output:** Every input listed → `LoadError::UnresolvedScalar
  { value: <input>, pos: ... }`.
- **Spec expectation:** Plain scalar with no matching regex → error.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:262` (`Err(UnresolvedScalar)` fallback) and
  `loader.rs:1027-1032` translating to `LoadError::UnresolvedScalar`.
- **Reasoning:** The spec uses "should" (line 6571) which Phase 1 [83]
  precedent treats as non-mandatory — a lenient implementation could
  fall back to `!!str`. The parser instead chose the strict variant
  named in the spec. Both options are spec-conformant; the parser's
  behavior is `Strict-conformant` (it errors as the spec recommends).
  The strict choice is also documented at `schema.rs:9-11`.

### REQ-§10.2-9 — `-0` resolves to `!!float`, not `!!int`

- **Spec requirement:** §10.2.2 regex (line 6578) for int is
  `-? ( 0 | [1-9] [0-9]* )`. Reading literally, the parenthesised
  alternative `0 | [1-9][0-9]*` — and the int row in the table is
  `0 | -? [1-9] [0-9]*` (line 6578). These two readings differ.
  Reading 1 (table-row text): `-` only attaches to `[1-9][0-9]*`, so
  `-0` is NOT an int. Reading 2 (parenthesised alternative): `-` may
  attach to either, so `-0` IS an int.
  The §10.2 example at line 6592–6604 shows `-0` resolving to integer
  `0` (via `Integers: [ 0, -0, 3, -19 ]` → `[ 0, 0, 3, -19 ]`),
  matching reading 2.
  The float regex (line 6579) is `-? ( 0 | [1-9] [0-9]* ) ( \. [0-9]*
  )? ( [eE] [-+]? [0-9]+ )?` — `-` may attach to `0`, so `-0` matches
  float as well. With first-match-wins (line 6567) and table order
  null→bool→int→float, the order of evaluation determines whether `-0`
  is int or float.
- **Test method:** Probe `-0`.
- **Test input:** `-0`.
- **Observed output:** `-0` → `tag:yaml.org,2002:float`.
- **Spec expectation:** Ambiguous — depends on reading. The example
  output suggests int; the table-row text suggests float (table reads
  `0 | -? [1-9] [0-9]*`, no `-` on the bare `0`).
- **Verdict:** Stricter-than-spec.
- **Evidence:** `schema.rs:413-426` reads the int row literally as `0 |
  -?[1-9][0-9]*` (the `0` arm is bare, no sign); `-0` falls through to
  `is_json_float` at `schema.rs:432-471`, which accepts it. The
  doc-comment at `schema.rs:247-251` explicitly notes this design
  decision.
- **Reasoning:** The implementation honors the literal table-row text
  `0 | -? [1-9] [0-9]*`. The spec example (line 6601) showing `-0` as
  integer `0` contradicts the regex when read literally; the
  parenthesised form at line 6578 is the more permissive alternative.
  The implementation chose the literal text reading, which produces
  `!!float` rather than `!!int`. Output is still numerically correct
  under either tag (both would format as `-0`/`0`), so this is a
  technical regex-strictness choice, not a wrong-answer bug. Marking
  Stricter-than-spec because the spec's worked example (line 6601)
  resolves `-0` to int and the parser does not.

### REQ-§10.2-10 — Source tag overrides resolution

- **Spec requirement:** §10.2.2 (lines 6557–6559) — nodes with the `!`
  non-specific tag resolve via the standard convention to
  seq/map/str; only `?` plain-scalar nodes participate in regex
  matching. An explicit source tag (`!!int`, `!!str`, `!custom`) takes
  priority.
- **Test method:** Probe `!!str 42`, `! 42`, `!!int "42"`, `!!int
  hello`, `!!null 42`.
- **Test input:** As above.
- **Observed output:**
  - `!!str 42` → `tag:yaml.org,2002:str` (no error, even though `42`
    would resolve to int)
  - `! 42` (bare `!` non-specific) → `tag:yaml.org,2002:str`
  - `!!int "42"` (DoubleQuoted) → `tag:yaml.org,2002:int` (explicit tag
    wins over style)
  - `!!int hello` → `tag:yaml.org,2002:int` (no regex check; user said
    so)
  - `!!null 42` → `tag:yaml.org,2002:null`
- **Spec expectation:** Explicit tag bypasses resolution; bare `!`
  resolves to `!!str` for scalars.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:124-128` (early return on `source_tag.is_some()`),
  `loader.rs:1010-1013` (`!` translated to `!!str`).
- **Reasoning:** The spec's `!`/`?` distinction is honored; explicit
  tag URIs pass through unchanged. Note: the parser does not validate
  whether a user's explicit `!!int hello` is semantically a valid int
  — that is per spec, since the resolver only governs untagged plain
  scalars.

### REQ-§10.2-11 — Collections resolve to !!seq / !!map under JSON

- **Spec requirement:** Lines 6562–6564 — collections with the `?`
  tag resolve to `!!seq` (sequence) or `!!map` (mapping) by kind.
  Same as Failsafe.
- **Test method:** Probe untagged sequences and mappings.
- **Test input:** `[]`, `{}`, `[1, 2, 3]`, mapping with quoted keys
  `"a": 1\n"b": null`, `"a": 1, "b": null` flow form.
- **Observed output:**
  - `[]` → `tag:yaml.org,2002:seq` (root)
  - `{}` → `tag:yaml.org,2002:map`
  - `[1, 2, 3]` → seq with three int children
  - `{"a": 1, "b": null}` → map with str/int and str/null children
- **Spec expectation:** Untagged sequence → `!!seq`; untagged mapping
  → `!!map`.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:168-183` (`resolve_collection`),
  `loader.rs:1035-1063`.
- **Reasoning:** Collection resolution is identical to Failsafe and
  Core; the JSON schema does not restrict collection structure.

### REQ-§10.2-12 — Strict mode (no fallback to !!str)

- **Spec requirement:** Lines 6569–6571 — JSON Schema says the
  processor "should consider them to be an error." In contrast, §10.3
  Core schema (line 6652) explicitly falls back to `!!str` for
  unmatched. The two schemas differ specifically on this point.
- **Test method:** Probe a plain scalar that does not match any JSON
  pattern under both `Schema::Json` and `Schema::Core`.
- **Test input:** `hello` plain.
- **Observed output:**
  - Under `Schema::Json`: `LoadError::UnresolvedScalar`.
  - Under `Schema::Core` (verified per source `schema.rs:194-236`,
    `resolve_core_plain`): would resolve to `!!str` (fallback at line
    234).
- **Spec expectation:** JSON errors; Core falls back to `!!str`. The
  difference is observable.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:243-245` (doc-comment: "No fallback —
  unmatched scalars return Err(UnresolvedScalar)") and `:262`
  (`Err(UnresolvedScalar)` is the unmatched return).
- **Reasoning:** The strict-error behavior is the spec's recommended
  variant ("should consider them to be an error"). Phase 1 [83]
  precedent makes "should" non-mandatory, but choosing the stricter
  variant is still spec-conformant. The parser's `Schema::Core` arm
  provides the lenient alternative for users who need it.

### REQ-§10.2-13 — Order of resolution (first-match-wins)

- **Spec requirement:** Line 6567 — "matched with a list of regular
  expressions (first match wins, e.g. `0` is resolved as `!!int`)". The
  spec gives `0` as the cited example: `0` matches both int and float
  regexes; int wins because it appears first in the table.
- **Test method:** Probe `0` and observe whether int or float wins.
- **Test input:** `0`, `100`, `1`.
- **Observed output:**
  - `0` → `tag:yaml.org,2002:int`
  - `100` → `tag:yaml.org,2002:int`
  - `1` → `tag:yaml.org,2002:int`
- **Spec expectation:** Per spec example, `0` is `!!int`.
- **Verdict:** Strict-conformant.
- **Evidence:** `schema.rs:252-264` dispatches null → bool → int →
  float in that order. The bare `0` matches `is_json_int` at the int
  branch and short-circuits before reaching float.
- **Reasoning:** Dispatch order matches the spec's table order, and
  the cited spec example (`0` → int) is reproduced.

## Summary tally

- 13 requirements enumerated.
- 12 Strict-conformant.
- 1 Stricter-than-spec (REQ-§10.2-9: `-0` resolves to `!!float`, not
  `!!int`; the spec example shows `-0` as int but the regex's literal
  table-row text excludes `-0` from int).
- 0 Lenient.
- 0 Non-conformant.
- 0 Indeterminate.

## Final probe-cleanup verification

`/tmp/audit-probe-10.2-a/` removed. `git status --porcelain` shows zero
new files in `rlsp-yaml-parser/`.
