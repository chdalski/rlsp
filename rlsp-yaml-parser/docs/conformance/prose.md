# Phase 2 Normative Prose Findings

This file covers the 130 normative-prose requirements audited in Phase 2 across seven
areas: §5.2 character encodings, §6.8 directives, §6.9.1 tag resolution, §10.1 Failsafe
schema, §10.2 JSON schema, §10.3 Core schema, and error semantics + resource limits.

**Audit source:** `.ai/audit/2026-04-30-phase2-prose/` (7 reconciliation files + summary)

**Methodology:** behavioral — construct inputs, run through parser, compare observed
output to spec expectation. Dual-track: independent A and B subagents per area, lead
reconciliation. See `README.md` for the full methodology description.

**Verdict taxonomy:** Strict-conformant (SC), Stricter-than-spec (ST), Not-applicable
(NA). All Lenient findings from the original audit were either fixed or carry open
follow-up entries in the project queue. Open unfixed findings (NC1, L2/L3, L7, L11,
L12–L17) are documented in `README.md`'s open-findings table and are not listed here.

---

## §5.2 Character Encodings (10 requirements)

**Final tally:** SC: 9, Non-conformant: 1 (unfixed, tracked separately)

The YAML 1.2.2 spec §5.2 defines a 9-row encoding-detection table and rules for BOM
handling at document-prefix positions.

| # | Requirement | Verdict |
|---|------------|---------|
| 1 | BOM at stream start accepted as encoding signal | SC |
| 2 | Five required encodings supported (UTF-8, UTF-16-LE/BE, UTF-32-LE/BE) | SC |
| 3 | Encoding is presentation; same content across encodings yields same parse | SC |
| 4 | BOM accepted at document prefix; rejected mid-document and post-`---` | SC |
| 5 | BOM-less encoding detection per §5.2 detection table | Non-conformant (NC1) |
| 6 | Truncated and invalid byte sequences rejected with typed errors | SC |
| 7 | BOM allowed inside quoted scalars (JSON compatibility) | SC |
| 8 | Multi-document streams with per-document BOM via loader API | SC |
| 9 | Double BOM at stream start rejected | SC |
| 10 | Empty and BOM-only streams handled as zero-document streams | SC |

### Implementation sites

- **Encoding detection:** `detect_encoding()` in `src/encoding.rs`
- **BOM stripping:** `scan_line()` (stream-start) and `signal_document_boundary()` (per-doc prefix) in `src/lines.rs`

### Fixed finding: L1 — double BOM at stream start

**Spec requirement:** production [202] `l-document-prefix ::= c-byte-order-mark? l-comment*`
— at most one BOM at any document prefix.

**Original defect:** two BOM-stripping code paths both ran for the first document
(`lines.rs:scan_line` with `is_first=true` AND `lines.rs:signal_document_boundary`),
silently consuming two consecutive BOMs at stream start. The inter-document transition
correctly ran only one path, so a double BOM was rejected there but accepted at stream
start.

**Fix:** `6b8219e fix(rlsp-yaml-parser): reject double BOM at stream start` — gated the
stream-start strip so only one BOM is consumed, making stream-start behavior consistent
with inter-document behavior.

### Open finding: NC1 — BOM-less UTF-32 detection arms missing

`detect_encoding()` in `encoding.rs` implements 7 of the spec's 9 encoding-detection
table rows. The two BOM-less UTF-32 arms (`[0x00, 0x00, 0x00, a]` and
`[a, 0x00, 0x00, 0x00]`) are absent; BOM-less UTF-32-BE input is misclassified as UTF-8
and BOM-less UTF-32-LE input is misclassified as UTF-16-LE. Tracked in the follow-up
queue. Verdict: Non-conformant.

---

## §6.8 Directives (29 requirements)

**Final tally:** SC: 24, ST: 3, Lenient (unfixed): 2

The §6.8 directive grammar covers `%YAML` version directives, `%TAG` handle directives,
reserved directives, and document-level scoping.

| # | Requirement | Verdict |
|---|------------|---------|
| 1 | `%YAML 1.2` accepted | SC |
| 2 | No `%YAML` directive accepted (implicit version) | SC |
| 3 | Higher major (`%YAML 2.0`) rejected | SC |
| 4 | Higher minor (`%YAML 1.3`) processed (spec says "with warning") | SC |
| 5 | Lower minor (`%YAML 1.1`) processed (spec says "with adjustment") | SC |
| 6 | Major version 0 rejected (`%YAML 0.x`) | ST |
| 7 | Minor digit overflow (256+) rejected | ST |
| 8 | Duplicate `%YAML` directive rejected | SC |
| 9 | Per-document `%YAML` scope (no carry across `---`/`...`) | SC |
| 10 | `%TAG` primary handle (`!`) | SC |
| 11 | `%TAG` secondary handle (`!!`) defaults and overrides | SC |
| 12 | `%TAG` named handle (`!handle!`) requires declaration | SC |
| 13 | `%TAG` per-document scope | SC |
| 14 | Duplicate `%TAG` handle rejected | SC |
| 15 | Reserved/unknown directive ignored (spec says "with warning") | SC |
| 16 | `MAX_DIRECTIVES_PER_DOC = 64` limit | ST |
| 17 | Lowercase directive names treated as reserved (case-sensitive `YAML`/`TAG`) | SC |
| 18 | NUL bytes in directive name pass through | Lenient (L2, unfixed) |
| 19 | NUL bytes in directive parameter pass through | Lenient (L3, unfixed) |
| 20 | Tab vs space separator before parameters | SC |
| 21 | Trailing comment after `%YAML` accepted | SC |
| 22 | Trailing junk after `%YAML 1.2` rejected | SC |
| 23 | `%TAG ! ! # primary` comment-after-prefix absorbed into prefix | SC |
| 24 | `%TAG !foo` missing trailing `!` rejected | SC |
| 25 | `%TAG` named handle with underscore rejected | SC |
| 26 | `%TAG` named handle with hyphen accepted | SC |
| 27 | `%TAG` missing prefix rejected | SC |
| 28 | `%YAML` directive without `---` rejected | SC |
| 29 | Indented `%` line not recognized as directive | SC |

### Implementation sites

- **Version parsing:** `parse_yaml_directive()` in `src/event_iter/directives.rs`
- **Major-version gate:** checks `major != 1` in `parse_yaml_directive()` — only major 1 accepted; `%YAML 1.0` is accepted (minor = 0 is permitted by this check)
- **Digit overflow:** `parse::<u8>()` on both major and minor strings — values 256–999 produce parse errors
- **Directive count:** `directive_count >= MAX_DIRECTIVES_PER_DOC` check in `directives.rs`
- **TAG prefix scanner:** `src/event_iter/directives.rs` tag-directive parsing

### "Should is non-mandatory" precedent (items 4, 5, 15)

The spec uses "should … with appropriate warning" three times in §6.8 (1.1 adjustment,
1.3 acceptance, unknown-directive ignore). The parser has no `Event::Warning` variant;
it silently accepts/ignores these cases. This is Strict-conformant per the precedent
established at Phase 1 `[83] ns-reserved-directive`: "should" is non-mandatory (RFC
2119); the parser's behavior satisfies the literal spec language. The version is surfaced
in `DocumentStart.version`, allowing consumers to emit their own warnings. See
[design-decisions.md](design-decisions.md) for the architectural note on the absent
Warning channel.

### Fixed finding: L4 — `%TAG` comment-after-prefix absorption

**Spec requirement:** `%TAG` prefix must be followed by `s-l-comments` before any
trailing content; the comment is not part of the prefix value.

**Original defect:** `%TAG ! ! # primary` absorbed `# primary` into the prefix string
because the prefix scanner did not honor `s-l-comments` after `ns-tag-prefix`. The
resolved prefix became `! # primary` instead of `!`.

**Fix:** `9056eed fix(rlsp-yaml-parser): stop %TAG prefix from absorbing trailing comments`
— the scanner now consumes optional whitespace and trailing comment after `ns-tag-prefix`.

### Refined behavior: `%YAML 1.0` accepted

Phase 1 [86] documented "major-0 rejection." The behavioral refinement is: only
`major == 0` is rejected; `%YAML 1.0` is accepted. The minor-component bound (255 max
via `parse::<u8>`) catches digit overflow but minor = 0 itself is not gated. This is the
correct reading of the major-0 Stricter-than-spec entry.

---

## §6.9.1 Tag Resolution (28 requirements)

**Final tally:** SC: 26, Lenient (unfixed): 1, Indeterminate (re-verdicted SC): 1

The §6.9.1 section covers verbatim tags, shorthand tags, non-specific tags, handle
declaration scoping, and post-concatenation URI validity.

| # | Requirement | Verdict |
|---|------------|---------|
| 1 | Verbatim tag delivered as-is to application | SC |
| 2 | Verbatim URI body must be `ns-uri-char+` (non-empty) | SC |
| 3 | Verbatim must begin with `!` or be valid URI | SC |
| 4 | Verbatim tag must be separated from content by whitespace | SC |
| 5 | Verbatim `%XX` decoded values not re-validated against `ns-uri-char` | SC |
| 6 | Tag and anchor properties allowed in either order | SC |
| 7 | Tag with no following content yields empty scalar | SC |
| 8 | `c-ns-tag-property` dispatches to (verbatim \| shorthand \| non-specific) | SC |
| 9 | Multiple tag properties on one node = error | SC |
| 10 | Tag handle is a presentation detail; may be discarded | SC |
| 11 | Tag must be separated from content (shorthand path) | SC |
| 12 | Primary tag handle (`!`) defaults to `!` prefix | SC |
| 13 | Secondary tag handle (`!!`) defaults to `tag:yaml.org,2002:` | SC |
| 14 | Named tag handle (`!h!`) requires explicit `%TAG` declaration | SC |
| 15 | Empty suffix on shorthand handles is invalid | Lenient (L7, unfixed) |
| 16 | Shorthand suffix may not contain `!` | SC |
| 17 | Shorthand suffix may not contain `[ ] { } ,` | SC |
| 18 | Shorthand suffix `ns-tag-char` characters | SC |
| 19 | Percent-encoded `%XX` sequences allowed in suffix | SC |
| 20 | Post-concatenation resolved tag must be valid URI or local tag | SC |
| 21 | Non-specific tag (`!`) for non-plain scalars and `?` for other nodes | SC |
| 22 | Explicit `!` non-specific tag forces failsafe resolution | SC |
| 23 | `?` non-specific tag has no explicit syntax | SC |
| 24 | Default tags applied by kind for untagged nodes (loader) | SC |
| 25 | `%TAG` handles scoped per document | SC |
| 26 | Tag resolution depends only on non-specific tag, path, content | SC |
| 27 | Unresolved tags allow partial representation | SC |
| 28 | (Cross-attribution) `%TAG` prefix admits non-`ns-uri-char` characters | SC (fixed upstream) |

### Implementation sites

- **Verbatim tag parsing:** `src/event_iter/properties.rs` (verbatim arm)
- **Shorthand tag parsing:** `src/event_iter/properties.rs` (shorthand arm)
- **Separator enforcement:** `src/event_iter/step.rs`
- **Post-concatenation URI validation:** tag resolution path in `src/event_iter/directives.rs`
- **Loader tag handling:** `loader.rs`

### Fixed findings: L5, L6 — verbatim tag admissibility and separator

**L5 — verbatim admissibility:** `!<$:?>`, `!<:foo>`, `!<!>` were accepted; the spec
(§6.9.1, "Verbatim Tags") requires the body to begin with `!` (local tag form) or be a
valid URI (global tag form). Spec Example 6.25 lists these as invalid verbatim tags.
Additionally, the loader's bare-`!` shortcut at `loader.rs` misclassified verbatim
`!<!>` as a shorthand non-specific tag.

**L6 — verbatim separator:** `!<URI>foo` (no whitespace between `>` and content) was
accepted; the shorthand path correctly rejected the parallel case. Asymmetric.

**Fix:** `02babe6 fix(rlsp-yaml-parser): enforce verbatim tag admissibility and separator`
— the verbatim arm in `properties.rs` now checks the prose-level admissibility rule
(body must begin with `!` or match URI well-formedness) and enforces `s-separate(n,c)`
before content, mirroring the shorthand path.

### Fixed finding: L8 — post-concatenation tag URI validity

**Spec requirement:** the resolved tag (handle prefix + suffix concatenation) must be a
valid URI or local tag.

**Original defect:** handle+suffix concatenation result was not re-validated. A prefix
with bytes outside `ns-uri-char` (permitted by the then-Lenient `%TAG` prefix scanner)
propagated through to the AST tag field.

**Fix:** `0a6f09e fix(rlsp-yaml-parser): validate resolved tag URI after handle+suffix concatenation`

### Indeterminate re-verdict: item 27

Item 27 (unresolved tags allow only partial representation, §6.9.1 / §3.3.2) was
initially Indeterminate pending the §10 schema audits. After §10.1/§10.2/§10.3 audits,
all three schemas fully resolve unresolved nodes to specific tags. The spec's "may
compose a partial representation" is non-mandatory; the implementation's full-resolution
choice is permissible. Re-verdicted SC.

---

## §10.1 Failsafe Schema (8 requirements)

**Final tally:** SC: 8

Both auditors enumerated 8 requirements and verdicted all 8 as Strict-conformant. No
disagreements.

| # | Requirement | Verdict |
|---|------------|---------|
| 1 | Failsafe defines exactly three tags (`!!str`, `!!seq`, `!!map`) | SC |
| 2 | All scalars resolve to `!!str` regardless of content | SC |
| 3 | All sequences resolve to `!!seq` | SC |
| 4 | All mappings resolve to `!!map` | SC |
| 5 | Plain and quoted scalars resolve identically (both → `!!str`) | SC |
| 6 | `!` non-specific tag resolves by kind under Failsafe | SC |
| 7 | Explicit non-failsafe tags pass through unmodified | SC |
| 8 | Schema selection is per-loader; Failsafe is selectable | SC |

### Implementation sites

- **Failsafe resolution:** `resolve_scalar()` constant arm in `src/schema.rs` (returns `!!str` unconditionally for all scalar styles/content)
- **Collection resolution:** `resolve_collection()` in `src/schema.rs` — `let _ = schema` discards the schema parameter; all three schemas share kind-only collection dispatch
- **Bare `!` normalization:** `effective_tag = tag.filter(|t| *t != "!")` at loader-resolution site in `loader.rs`
- **Schema selection:** `LoaderBuilder::schema(Schema::Failsafe)`

### Architectural notes

- The default schema is `Core`, not `Failsafe` — matches §10.3's "recommended default" language.
- Explicit non-Failsafe tags (`!!int`, `!foo`) pass through the AST under Failsafe; schemas govern resolution, not source-tag rejection.

---

## §10.2 JSON Schema (13 requirements)

**Final tally:** SC: 13

Both auditors enumerated 13 requirements. All are Strict-conformant after reconciliation
of the `-0` resolution question (see spec errata note below).

| # | Requirement | Verdict |
|---|------------|---------|
| 1 | Tag set: Failsafe + null + bool + int + float | SC |
| 2 | Null regex: `null` only (case-sensitive) | SC |
| 3 | Bool regex: `true \| false` only (case-sensitive) | SC |
| 4 | Int regex: `0 \| -? [1-9] [0-9]*` | SC |
| 5 | Float regex: `-? ( 0 \| [1-9] [0-9]* ) ( \. [0-9]* )? ( [eE] [-+]? [0-9]+ )?` plus `.inf`/`.nan` | SC |
| 6 | `+0`, `+42`, `+12.3` (leading `+`) resolve to `!!str` | SC |
| 7 | Octal/hex (`0o7`, `0x3A`) resolve to `!!str` | SC |
| 8 | `.inf`, `-.inf`, `.nan` resolve to `!!str` under JSON | SC |
| 9 | `-0` resolves to `!!float` | SC |
| 10 | Quoted scalars (single + double) → `!!str` | SC |
| 11 | Plain scalars not matching any regex → strict-mode error | SC |
| 12 | Empty implicit scalars → strict-mode error | SC |
| 13 | Schema selection per-loader (`Schema::Json`) | SC |

### Implementation sites

- **JSON regex functions:** `is_json_int()`, `is_json_float()`, `is_json_bool()`, `is_json_null()` in `src/schema.rs`
- **Unresolved scalar error:** `UnresolvedScalar` error path in `loader.rs`
- **Quoted scalar override:** bypasses regex matching for `ScalarStyle::SingleQuoted` and `ScalarStyle::DoubleQuoted`

### Spec errata: `-0` worked example contradicts int regex

**Spec line 6578 (normative regex):** `0 | -? [1-9] [0-9]*` — the `0` alternative
carries no sign; `-0` does NOT match. The `-? [1-9] [0-9]*` alternative requires the
integer part to begin with `[1-9]`.

**Spec line 6601 (worked example):** shows `-0` resolving to integer `0`.

These are internally inconsistent. In YAML spec convention, BNF/regex is the formal
normative rule; examples are illustrative. The parser follows the literal regex:
`is_json_int()` rejects `-0` (no sign on the `0` alternative); `is_json_float()` accepts
`-0` via `-? ( 0 )`. The parser's behavior (`-0` → `!!float`) matches de-facto practice
among mature JSON-schema YAML parsers. See [design-decisions.md](design-decisions.md)
for the formal errata note.

---

## §10.3 Core Schema (19 requirements)

**Final tally:** SC: 18, ST: 1

Core extends JSON with broader null/bool/int/float recognition and is the parser's
default schema.

| # | Requirement | Verdict |
|---|------------|---------|
| 1 | Tag set identical to JSON | SC |
| 2 | Schema is loader default | SC |
| 3 | Schema selectability via `LoaderBuilder` | SC |
| 4 | Null forms `null \| Null \| NULL \| ~` | SC |
| 5 | Empty plain scalar → `!!null` | SC |
| 6 | Bool forms (six exact strings, case-sensitive) | SC |
| 7 | Decimal int (leading zeros REJECTED) | ST |
| 8 | Octal int `0o [0-7]+` (unsigned) | SC |
| 9 | Hex int `0x [0-9a-fA-F]+` (unsigned) | SC |
| 10 | Decimal float regex | SC |
| 11 | Float infinity (signed) | SC |
| 12 | Float NaN (unsigned per spec; signed correctly rejected) | SC |
| 13 | Plain unmatched scalars → `!!str` (Core permissive) | SC |
| 14 | Quoted scalars override regex matching → `!!str` | SC |
| 15 | Block scalars (literal/folded) → `!!str` | SC |
| 16 | Untagged collection resolution by kind | SC |
| 17 | Explicit tag overrides resolution | SC |
| 18 | `-0` Core dispatch → `!!int` (regex `[-+]? [0-9]+`) | SC |
| 19 | Spec example (§10.3 lines 6657–6677) replay | SC |

### Implementation sites

- **Core schema functions:** `is_core_int()`, `is_core_float()`, `is_core_bool()`, `is_core_null()` in `src/schema.rs`
- **Leading-zero rejection:** `is_core_int()` in `src/schema.rs` — rejects strings whose first byte is `0` and length > 1
- **Sign strip:** `is_core_int()` strips leading `+`/`-` before per-base dispatch; after Phase 2 fixes, sign strip is gated so only decimal-shaped bodies receive sign treatment (octal/hex bodies with a sign now reject rather than match)

### Behavioral highlights

- YAML 1.1 hold-overs correctly excluded: `yes`, `no`, `on`, `off` and all case variants resolve to `!!str` under Core
- Mixed-case null aliases excluded: `nUll`, `none`, `nil` → `!!str`
- Special floats handled correctly: `.inf`, `.Inf`, `.INF`, `+.inf`, `-.inf`, `.nan`, `.NaN`, `.NAN` all match; signed NaN correctly rejected
- `0o9`, `0o8` correctly rejected as out-of-range octal; `0x` and `0o` prefix-only correctly rejected
- `-0` resolves to `!!int` under Core (regex `[-+]? [0-9]+` permits it); contrast with JSON where `-0` → `!!float`

### Fixed findings: L9 and L10 — signed octal and hex integers

**Spec requirement (§10.3 normative table):** three distinct int rows:
- Decimal: `[-+]? [0-9]+` (sign permitted)
- Octal: `0o [0-7]+` (no sign)
- Hex: `0x [0-9a-fA-F]+` (no sign)

**Original defect:** the sign-strip in `is_core_int()` was unconditional — it applied
before per-base validation. `-0o10` and `+0o10` were stripped to `0o10` (matched as
octal `!!int`); `-0xFF` and `+0xFF` were stripped to `0xFF` (matched as hex `!!int`).
The spec's octal and hex rows carry no sign.

**Fix:** `2865a74 fix(rlsp-yaml-parser): reject signed octal and hex integers under Core schema`
— the sign strip in `is_core_int()` is now gated: after stripping a leading sign, if the
remaining body begins with `0o` or `0x`, the sign is invalid for that row and the input
falls back to `!!str`.

---

## Error Semantics and Resource Limits (23 requirements)

**Final tally:** SC: 16, Lenient (unfixed): 7

This area covers error production, error structure, no-panic guarantees, error-position
accuracy, and enforced resource limits.

| # | Requirement | Verdict |
|---|------------|---------|
| 1 | Errors are produced for malformed input | SC |
| 2 | Errors are structured (`Error` / `LoadError`, not panic) | SC |
| 3 | No panics in production code paths (25+ adversarial probes) | SC |
| 4 | Error recovery: stop-at-first behavior documented and observed | SC |
| 5 | Error position present (line/column/byte_offset structure) | SC |
| 6 | Error position points to offending byte — implicit-key 1024 limit | SC |
| 7 | Error position points to offending byte — directive count limit | SC |
| 8 | Error position — Phase 1 [59]/[60]/[61] numeric escape rejections | SC |
| 9 | Error position — `%YAML` major-0 rejection | Lenient (L12, unfixed) |
| 10 | Error position — u8 digit-overflow | Lenient (L13, unfixed) |
| 11 | Error position — unterminated single-quoted scalar | Lenient (L14, unfixed) |
| 12 | Error position — resolved-tag overflow | Lenient (L15, unfixed) |
| 13 | Error position — `MAX_ANCHOR_NAME_BYTES` overflow | Lenient (L16, unfixed) |
| 14 | Error position — `LoadError` variants carry no pos field | Lenient (L17, unfixed) |
| 15 | `MAX_COLLECTION_DEPTH = 512` — limit enforced | SC |
| 16 | `MAX_ANCHOR_NAME_BYTES = 1024` — limit enforced (multi-byte verified) | SC |
| 17 | `MAX_TAG_LEN = 4096` — limit enforced | SC |
| 18 | `MAX_COMMENT_LEN = 4096` — limit enforced | SC |
| 19 | `MAX_DIRECTIVES_PER_DOC = 64` — limit enforced | SC |
| 20 | `MAX_TAG_HANDLE_BYTES = 256` — limit enforced | SC |
| 21 | `MAX_RESOLVED_TAG_LEN = 4096` — limit enforced | SC |
| 22 | Loader limits (`max_nesting_depth`, `max_anchors`, `max_expanded_nodes`) enforced | SC |
| 23 | 1 MiB quoted-scalar cap covers all paths | Lenient (L11, unfixed) |

### Implementation sites

All limits defined in `src/limits.rs`. Enforcement sites:

| Limit | Enforced in |
|-------|------------|
| `MAX_DIRECTIVES_PER_DOC = 64` | `src/event_iter/directives.rs` — `directive_count >= MAX_DIRECTIVES_PER_DOC` |
| `MAX_COLLECTION_DEPTH = 512` | `src/event_iter/` collection-open paths |
| `MAX_ANCHOR_NAME_BYTES = 1024` | `src/event_iter/properties.rs` `scan_anchor_name()` |
| `MAX_TAG_LEN = 4096` | `src/event_iter/directives.rs` tag-prefix scanner |
| `MAX_COMMENT_LEN = 4096` | `src/event_iter/directives.rs` comment consumer |
| `MAX_TAG_HANDLE_BYTES = 256` | `src/event_iter/directives.rs` handle scanner |
| `MAX_RESOLVED_TAG_LEN = 4096` | tag resolution path |
| Loader limits | `LoaderOptions` fields; enforced in `loader.rs` |

### No-panic property

Both auditors confirmed via 25+ adversarial probes (unterminated quotes, raw control
bytes, deep nesting, lone indicators, malformed directives) that all error paths produce
structured `Error` or `LoadError` values. Production `unreachable!()` calls are
caller-side invariant guards on private functions, not user-reachable. This is a real
implementation strength.

### Position-precision design contract

The parser has a precision asymmetry: most error positions point to the start-of-construct
where parsing began, not the offending byte where the violation occurred. The
implicit-key 1024 limit (item 6) is the exception — it correctly captures the
offending byte. Items 9–14 (L12–L17) are the known imprecise cases:

| Error class | Reported position | Should point to |
|-------------|-------------------|-----------------|
| `%YAML` major != 1 rejection | `%` at column 0 | The major digit |
| `%YAML` u8 digit-overflow | `%` at column 0 | The first digit beyond the limit |
| Unterminated single-quoted scalar | EOF | The opening `'` |
| Resolved-tag overflow | `---` line position | The offending `!handle!` token |
| `MAX_ANCHOR_NAME_BYTES` overflow | `&` start-of-anchor | The first byte beyond the limit |
| `LoadError` variants (UndefinedAlias, CircularAlias, limits) | (no pos field) | The offending node |

These are usability defects, not spec-conformance defects (the spec is silent on position
precision). They are tracked in the follow-up queue.

### Open finding: L11 — 1 MiB quoted-scalar cap bypass on no-escape borrow path

The documented 1 MiB quoted-scalar length cap in `lexer/quoted.rs` is enforced only on
the owned path (allocated after a `\` escape triggers the decode-and-buffer routine). A
double-quoted scalar with no escape characters takes the borrow path, which has no
length check. A 100 MiB escape-free double-quoted scalar parses without error. Tracked
in the follow-up queue as DoS-relevant.
