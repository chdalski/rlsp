# Design Decisions and Stricter-than-Spec Rationales

This file documents all Stricter-than-spec entries across Phase 1 (BNF) and Phase 2
(prose), user design decisions confirmed post-Phase-2, and spec errata observed during
the audit. It also holds the §7 BNF-trace analysis cross-reference and the
"should is non-mandatory" precedent.

---

## Stricter-than-spec Entries

### Phase 1: [59] / [60] / [61] — Trojan Source mitigation via hex numeric escapes

**Productions:** `ns-esc-8-bit` / `ns-esc-16-bit` / `ns-esc-32-bit`

**Spec behavior:** the BNF for `\x`, `\u`, `\U` escapes specifies only the hex-digit
count; it places no restriction on the decoded codepoint beyond it being a valid Unicode
scalar value.

**Parser behavior:** stricter. After decoding the hex digits, `decode_escape()` in
`src/chars.rs` rejects the decoded character if it falls outside `is_c_printable()` or
is a bidirectional control character (U+202A–U+202E, U+2066–U+2069, U+200F, U+061C).
Named escapes (`\0`, `\a`, `\n`, etc.) are exempt — only hex numeric escapes face this
check.

**Rationale:** Trojan Source attacks (CVE-2021-42574) exploit bidirectional control
characters to make malicious code appear visually innocuous. A YAML document read by an
LSP server may contain user-supplied strings that are then embedded in generated source
code or diagnostics. Rejecting bidi-control characters via numeric escapes at parse time
prevents that class of injection at the protocol boundary. The exemption for named
escapes follows the principle that explicit intent (`\n` = newline, `\0` = null) is less
ambiguous than opaque codepoints produced via `\u202E`.

**User decision (Phase 1):** keep as-is.

**Enforcement site:** `decode_escape()` in `src/chars.rs` — `is_c_printable()` predicate.

---

### Phase 1: [86] — Major-version-0 rejection

**Production:** `ns-yaml-directive`

**Spec behavior:** a 1.2 YAML processor must reject directives with a major version
higher than 1. The spec places no requirement on `major == 0`.

**Parser behavior:** stricter. `parse_yaml_directive()` in `src/event_iter/directives.rs`
checks `if major != 1` — this rejects both `major == 0` and `major >= 2`.

**Behavioral refinement (Phase 2):** only `major == 0` is blocked beyond what the spec
requires. `%YAML 1.0` is accepted — the check gates on `major != 1`, so minor = 0 is
not independently rejected. `%YAML 1.300` is rejected via the u8 digit overflow check
([87]), not this gate.

**Rationale:** no defined YAML 0.x version exists; accepting `%YAML 0.5` would be
meaningless at best and misleading at worst. The defensive rejection costs nothing —
there is no valid document that needs it.

**User decision (Phase 1):** keep as-is.

**Enforcement site:** `parse_yaml_directive()` in `src/event_iter/directives.rs`.

---

### Phase 1: [87] — Minor-version u8 digit overflow

**Production:** `ns-yaml-version`

**Spec behavior:** the BNF `ns-dec-digit+` admits any number of decimal digits for the
major and minor version components.

**Parser behavior:** stricter. `parse_yaml_directive()` uses `parse::<u8>()` for both
major and minor; values outside [0, 255] produce a parse error. `%YAML 1.300` is
rejected.

**Rationale:** no realistic YAML version exceeds 255 for either component. The `u8`
parse is a pragmatic limit that avoids unbounded version-number inputs. The cost is
negligible: no real YAML document uses a minor version above 255.

**User decision (Phase 1):** keep as-is.

**Enforcement site:** `parse::<u8>()` calls in `parse_yaml_directive()` in
`src/event_iter/directives.rs`.

---

### Phase 2: S3 — `MAX_DIRECTIVES_PER_DOC = 64`

**Spec behavior:** YAML 1.2.2 §6.8 has no per-document directive count limit. The spec
allows an arbitrary number of `%TAG` and `%YAML` directives before a document marker.

**Parser behavior:** stricter. `src/event_iter/directives.rs` hard-rejects when
`directive_count >= MAX_DIRECTIVES_PER_DOC` (64). The 65th directive in a single
document produces a parse error regardless of its type.

**Rationale:** DoS protection. A pathological document with thousands of `%TAG` directives
would force the parser to allocate and track an unbounded number of handle-to-prefix
mappings. The 64-entry limit bounds the per-document allocation at the parser level,
before the loader sees the events. 64 directives covers every realistic use case (a
document needing more than a handful of `%TAG` declarations is rare in practice).

**User decision (2026-05-05):** keep as-is. The DoS protection rationale is accepted;
the limit is not configurable via `LoaderOptions`.

**Enforcement site:** `src/event_iter/directives.rs` — `directive_count >= MAX_DIRECTIVES_PER_DOC`.

Constant definition: `src/limits.rs` — `pub const MAX_DIRECTIVES_PER_DOC: usize = 64`.

---

### Phase 2: S4 — Core schema leading-zero decimal rejection

**Spec behavior:** YAML 1.2.2 §10.3 Core schema int regex: `[-+]? [0-9]+`. The regex
literally permits leading zeros: `007`, `01`, `0123` all match and would resolve to
`!!int`.

**Parser behavior:** stricter. `is_core_int()` in `src/schema.rs` rejects strings whose
stripped body begins with `0` and has length > 1 — these resolve to `!!str` instead.

**Rationale:** two independent motivations:

1. **YAML 1.1 octal confusion.** Many YAML 1.1 producers wrote `007` intending octal
   (YAML 1.1's `[0-9]+` octal notation). A YAML 1.2 parser that silently resolves `007`
   to integer 7 would silently misinterpret those documents. Rejecting leading-zero
   decimals forces the user to fix the input rather than receiving a silent wrong answer.

2. **LSP diagnostic enablement.** An LSP server consuming the parsed AST can surface a
   precise "leading-zero decimal — did you mean octal?" diagnostic when the parser
   rejects the token and returns it as `!!str`. This is more informative than silently
   accepting `007` as integer 7.

**User decision (2026-05-05):** keep as-is. Both motivations accepted; the rejection
is intentional and should not be relaxed to match spec.

**Enforcement site:** `is_core_int()` in `src/schema.rs` — the leading-zero check after
sign stripping.

---

## Deviations from libfyaml

### [75] c-nb-comment-text — non-printable characters and length in comments

**Productions:** `c-nb-comment-text` ([75]: `"#" nb-char*`), `nb-char`, `c-printable` ([1]).

**libfyaml behavior** (`fy-tool --testsuite`, 0.9.6): accepts C0 controls, DEL, C1 controls
and U+FFFE in a comment body at every position (`# \x01`, `a: v # \x01`, `a: "q" # \x01`,
`k: # \x01`, `- # \x01`, `a: | # \x01`, `--- # \x01`, `... # \x01`, `%YAML 1.2 # \x01`,
`[a, # \x01`), and accepts a comment of any length (5000 bytes). It rejects NUL, at the
stream level, before it reaches the scanner.

**Parser behavior:** rejects every character outside `c-printable` in a comment body at
every position that reads a comment, NUL included (`InvalidCharacter`), and rejects a
comment body longer than `MAX_COMMENT_LEN` bytes (a syntax error at the `#`) at every
position. The spec allows only `nb-char` in a comment, and `nb-char` is `c-printable`
minus the line breaks and the byte order mark, so libfyaml is the wrong side on the
character set. The length limit is not in the spec; it is the parser's guard against
unbounded input, which the comment on its own line and the comment after a plain scalar in
a mapping value already had in part. A comment that the formatter moves to its own line
must still parse, so one rule holds for all positions: after a node or indicator (a quoted
scalar, `k:`, `-`, `?`, `:`, an anchor or tag, a block scalar header, the last line of a
multi-line plain scalar, a scalar on the `---` line, a flow collection line), after
`---` and `...`, and after a directive.

**libfyaml issue tracker** (<https://github.com/pantoniou/libfyaml/issues>, searched for
"non-printable", "c-printable", "control character", and "printable comment"): no issue
about non-printable characters in comments. The one hit for "non-printable" is about
anchor names.

**User decision (2026-10-30):** keep as-is. The parser stays strict per [75]; libfyaml
differs. The comment plan (`2026-10-30-formatter-keeps-every-comment`) extended the same
rule and one length limit to every position that reads a comment.

**Enforcement site:** `Lexer::comment_in_suffix()` and `Lexer::try_consume_comment()` in
`src/lexer/comment.rs`.

---

### [69] s-flow-line-prefix — a tab between the indentation and a property or alias

**Productions:** `s-flow-line-prefix(n)` ([69]: `s-indent(n) s-separate-in-line?`),
`s-separate-in-line` ([66]: `s-white+ | <start-of-line>`), `c-ns-properties` ([96]),
`c-ns-alias-node` ([104]).

**libfyaml behavior** (`fy-tool --testsuite`, 0.9.6): rejects a property or alias on its own
line when white space with a tab follows the indentation spaces (`-\n  \t!t a`,
`k:\n  \t&a x`, `-\n  \t*a`) with "plain scalar cannot start with '!'" (`'&'`, `'*'`). The
same tab after the dash (`- \t!t a`) is accepted.

**Parser behavior:** accepts. The line prefix of a node is the indentation spaces and then
optional white space, tabs included, so the tag, anchor or alias starts at its indicator
and the tab is separation. Before the fix the tab stayed in the line content the property
scanners started from: the tag read as `"\t!t"` and the anchor as `"&a"`, which no output
form can repeat. A tab as the first character of the line stays an error (§6.1: tabs are
not indentation), except before a flow collection delimiter.

**User decision:** pending; reported by the lead.

**Enforcement site:** `trimmed` in `EventIter::step_in_document()` in
`src/event_iter/step.rs`, which the tag, anchor and alias branches read.

---

### [82] l-directive — a `%` line inside a document

**Productions:** `c-directive` in `c-indicator` ([22]), `l-directive` ([82]),
`ns-plain-first` ([126]), `l-directive-document` ([209]), `l-yaml-stream` ([211]).

**libfyaml behavior** (`fy-tool --testsuite`, 0.9.6): warns "Unsupported directive" and skips
a `%` line wherever it starts a line inside a document: below the marker (`---` then `%a`,
with or without a node after it), below a tag (`---`, `!t`, `%a`), below a key (`a:` then
`%b`), after a mapping entry (`a: 1` then `%b`), after a marker-line scalar and comment
(`--- x # c` then `%y`), below a marker comment (`--- # c` then `%a`), and before a second
document (`---`, `%a`, `---`, `x`). The line is gone from the events, with no error.

**Parser behavior:** a syntax error at the `%`. A directive is valid only before `---`
([209], and [211] after a `...`), and `%` is an indicator ([22]) that no plain scalar starts
with ([126]), so a `%` that starts a line inside a document is neither a directive nor a
node. Skipping it would delete its text when the formatter rewrites the file. `%` stays
content where a scalar holds it: a continuation line of a plain scalar (`--- x` then `%y`
is the scalar `x %y`, as in libfyaml), a line of a block or quoted scalar (`--- |` then
`%a`), and the directive lines above `---` or after `...` are unchanged. `--- x # c` then
`%y` is an error as well: the comment ends the scalar, so the `%y` line starts a node
(libfyaml skips it).

**libfyaml issue tracker** (<https://github.com/pantoniou/libfyaml/issues>, searched for
"directive", "unsupported directive", "percent line", and "%TAG inside document"): no issue
about a directive line inside a document.

**Decision:** an error, not libfyaml's warning and skip; recorded in the approved plan
`2026-11-01-parser-keeps-the-data`.

**Enforcement site:** `is_reserved_plain_start()` in `src/lexer/plain.rs`, called by
`try_consume_scalar()` in `src/event_iter/base.rs`; the dispatcher's last branch in
`src/event_iter/step.rs` reports any other unclaimed line as an error.

---

### [1] c-printable — a non-printable character in a plain scalar

**Productions:** `c-printable` ([1]), `nb-char` ([27]), `ns-char` ([34]), `ns-plain-first` ([126]),
`ns-plain-char` ([130]).

**libfyaml behavior** (`fy-tool --testsuite`, 0.9.6): accepts C0 controls (except NUL), DEL,
C1 controls and the byte order mark as text of a plain scalar, at the start of a line and
inside it: `\x07zz` and `key: val\x07ue` give the scalar with the character in it, as do the
indented forms (`  \x07zz`, `---` then `  \x07zz`, `a:` then `  \x7fzz`, `  \uFEFFzz`). It
rejects NUL, at the stream level.

**Parser behavior:** an error for every character outside `c-printable`, and for the byte
order mark ([27], [34]), in a plain scalar that is not an implicit key, wherever it stands,
NUL included. U+FEFF is inside `c-printable`, but `nb-char` and `ns-char` exclude it, so a
plain scalar cannot hold it. The spec admits no such character in a scalar, so libfyaml is the wrong side on the character set,
the same as for comments (see the [75] entry). The column-0 and mid-scalar forms already
behaved this way. The indented forms (`  \x07zz`, `a:` then `  \x7fzz`, `---` then
` \t\uFEFFzz`) used to be consumed without an event or an error, so the text vanished from a
formatted file; they now report "invalid character U+XXXX in document" at the character.
An implicit key (`\x07b: 2`, `a:` then `  \x07b: 2`, `- \0b: 1`) still takes the character
into the key without an error, as before; its text is kept, so nothing is lost, but it is
not yet checked against [1]. That gap is open (see the user decision below). `- \0b: 1`
differs from libfyaml too, which rejects NUL while the parser loads it.

**libfyaml issue tracker** (<https://github.com/pantoniou/libfyaml/issues>, searched for
"non-printable", "c-printable", "control character", and "printable"): no issue about
accepting non-printable characters in a plain scalar; the hits cover anchor names and the
text length of a folded block scalar.

**User decision (2026-11-01):** keep the error. The parser stays strict per [1]; libfyaml
differs. The decision covers a non-printable character at the start of a line and inside a
plain scalar that is not an implicit key. The implicit-key gap above is not decided: it is
open and goes to the user.

**Enforcement site:** the last branch of `EventIter::step_in_document()` in
`src/event_iter/step.rs` (indented first character); `Lexer::try_consume_plain_scalar()`
(first line) and `Lexer::collect_plain_continuations()` (continuation lines) in
`src/lexer/plain.rs`, through `find_non_c_printable()`.

---

### [194] c-l-block-map-implicit-value — a block collection on the line of an empty key's `:`

**Productions:** `c-l-block-map-implicit-value(n)` ([194]), `s-l+block-node(n,c)` ([196]),
`s-l+block-collection(n,c)` ([200]: `s-l-comments` before the sequence or mapping),
`c-ns-properties` ([96]).

**libfyaml behavior** (`fy-tool --testsuite`, 0.9.6): reads a `:` line without a key as an
entry with an empty key and accepts a block collection that starts on the same line: `: - b`
is `{null: [b]}`, `a:` then `: - b` is `{a: null, null: [b]}`; so are `[x]:` then `: - a`,
`a:` then `: b: c` (`{a: null, null: {b: c}}`), `a:` then `: : v` (`{a: null, null: {null:
v}}`) and `: ? b`. It reads a property line below a nested entry with an empty key as the
empty value's property (`*a :`, ` :`, ` &y` is `{*a: {null: &y null}}`). With a key before
the `:` it rejects `a: - b`, `a: b: c` and `a: ? b`, and accepts `a: : v` (`{a: {null: v}}`).

**Parser behavior:** a syntax error for `: - b`, `: b: c` and `: : v`, with or without a
key before them ("block node cannot appear as inline value; use a new line or a flow
node"), and for a property line at a column the value cannot take (`*a :`, ` :`, ` &y`:
"node property is not indented enough for this context"). The spec reads the `:` line
without a key as an implicit entry ([192] with `e-node` as key, [194]), whose value is an
`s-l+block-node`: a block sequence or mapping there needs `s-l-comments` first ([200]), so
it starts on a line below the `:`, as after `a:` with a key. `: ? b` and `a: ? b` load
(libfyaml loads the first and rejects the second) although they put a block mapping on the
`:` line as well: `consume_mapping_entry()` rejects only `- ` and a further `:` in the
text after the `:`, not `? `. Before this change, a `:` line after an empty value made the
inputs above load with the wrong pairing (`a:` then `: - b` was `{a: [b]}`, `*a :`,
` :`, ` &y` was `{*a: &y null}`); now `a:` is `a: null` and the `:` line is an entry of its
own, as in libfyaml, and the error is the spec's reading of the block collection.

**libfyaml issue tracker** (<https://github.com/pantoniou/libfyaml/issues>, searched for
"block sequence same line", "compact mapping value indicator", "empty key", "invalid nested
block mapping", "null key colon", and "explicit value indicator"): no issue about a block
collection on the line of an empty key's `:`; the one hit for "empty key" is about test
failures on Alpine Linux.

**User decision:** pending; reported by the lead.

**Enforcement site:** `ConsumedMapping::InlineImplicitMappingError` in
`consume_mapping_entry()` in `src/event_iter/block/mapping.rs` (block collection after
the `:`); `consume_own_key_indicator()` and `handle_mapping_entry()` in the same file (the
`:` line of an empty key is an entry of its own).

### [189] / [191] — a `:` after an explicit key that is a mapping with a value

**libfyaml behavior** (`fy-tool --testsuite`, 0.9.6): in `?` then `  ? a`, `  : b` and
`: v`, the `:` at the column of the outer `?` closes the outer entry with an empty value
and starts an entry of its own: `{{a: b}: null, null: v}`. With the inner mapping
holding `? a` alone (`?`, `  ? a`, `: v`) the same `:` is the outer value
(`{{a: null}: v}`). So the reading depends on whether the last inner entry has a value.

**Parser behavior:** the `:` at the column of the outer `?` is the value indicator of
the outer key in both inputs: `{{a: b}: v}`. The same holds for `? ? a` then `  : b` and
`: v`, for an inner block mapping (`?`, `  x: 1`, `  ? a`, `  : b`, `: v`), and for an
inner sequence holding such a mapping (`?`, `  - ? a`, `    : b`, `: v`). A bare `:`
instead of `: v` gives the outer key an empty value.

**Spec:** [189] `c-l-block-map-explicit-entry(n)` is the key [190] followed by
[191] `l-block-map-explicit-value(n)` = `s-indent(n) ":" s-l+block-indented(n,block-out)`.
The inner mapping lies at indent `n+2` or deeper, so its entries need `s-indent(n+2)` and
the `:` at indent `n` cannot be one of them; the only production that takes it is [191]
of the outer entry. Nothing in the productions makes the reading depend on the value of
the inner entry.

**libfyaml issue tracker** (<https://github.com/pantoniou/libfyaml/issues>, searched for
"explicit key", "nested explicit", "complex key" and "? ?"): no issue about it; the hit
for "explicit key" is about test failures on Alpine Linux.

**User decision:** follow the spec; libfyaml is not followed.

**Enforcement site:** `MappingPhase::ExplicitKey` and `MappingPhase::AfterExplicitKey` in
`src/event_iter/state.rs`; `close_collections_at_or_above()` in
`src/event_iter/base.rs` moves the outer mapping from `ExplicitKey` to
`AfterExplicitKey` when its key node closes; `handle_mapping_entry()` in
`src/event_iter/block/mapping.rs` takes the `:` line for the value indicator of the key
in `AfterExplicitKey` phase.

---

## "Should is non-mandatory" Precedent

**Established at:** Phase 1, `[83] ns-reserved-directive` (see
[bnf-§6.md](bnf-§6.md)).

**Rule:** when the YAML spec uses "should" (RFC 2119 meaning: recommended, not mandatory),
the parser's silence is Strict-conformant. The implementation need not emit a warning or
take the recommended action.

**Application across Phase 2:** this precedent settled three §6.8 cases where the spec
says "should … with appropriate warning":

- §6.8.1: `%YAML 1.3` processed — "should be processed with an appropriate warning." The
  parser stores `version: (1, 3)` in `DocumentStart` and parses with 1.2 rules. Consumers
  may emit their own warning by reading `DocumentStart.version`.
- §6.8.1: `%YAML 1.1` processed — "should be processed with appropriate adjustment." The
  parser stores `version: (1, 1)` and applies 1.2 rules uniformly. Same consumer opt-in
  as above.
- §6.8: Unknown directive ignored — "should ignore unknown directives with an appropriate
  warning." Silent ignore per [83] precedent.

**Architectural note:** the parser has no `Event::Warning` variant or warning collector.
The event stream is `Result<(Event, Span), Error>` — success or error, no middle path.
The three "should warn" sites above are the most visible consequence. A future design
enhancement could add a warning side-channel (e.g., a `warnings: Vec<Warning>` field on
the `ParseEventIter`), which would benefit LSP consumers in particular.

---

## BNF-trace Analysis: §7.3.x vs §7.4.2 Implicit Keys

The full analysis is in `[110] nb-double-text(n,c)` in [bnf-§7.md](bnf-§7.md). Summary
for cross-reference:

The spec prose in §7.3.1, §7.3.2, §7.3.3 says "scalars are restricted to a single line
when contained inside an implicit key." This is a terminology trap — the formal meaning
is encoded in the BNF context labels, not the prose:

- `BLOCK-KEY` and `FLOW-KEY` are the formal "implicit key" contexts. Scalars in these
  contexts are restricted to a single line.
- `FLOW-IN` means "inside a flow collection but not formally an implicit key." Multi-line
  scalars are permitted in `FLOW-IN`.

The spec cross-reference at §7.4.2 `ns-flow-pair` uses `FLOW-KEY` for JSON-style keys,
confirming the single-line restriction applies to JSON-style implicit keys. The parser
applies the context label correctly: `try_consume_double_quoted()` in `lexer/quoted.rs`
enforces single-line in `BLOCK-KEY` and `FLOW-KEY` contexts; multi-line is permitted in
`FLOW-OUT` / `FLOW-IN`. This matches the BNF, not the potentially-misleading prose
summary.

---

## Spec Errata Observed

### §10.2 JSON Schema: `-0` worked example contradicts int regex

**Spec line 6578 (normative int regex):** `0 | -? [1-9] [0-9]*`

- The `0` alternative: no sign. `-0` does NOT match.
- The `-? [1-9] [0-9]*` alternative: requires integer part beginning with `[1-9]`.

**Spec line 6601 (worked example):** shows `-0` resolving to integer `0`.

These are internally inconsistent. The YAML spec convention is that BNF/regex is the
formal normative rule; worked examples are illustrative. The parser follows the literal
regex: `is_json_int()` in `src/schema.rs` rejects `-0` (the `0` arm carries no sign);
`is_json_float()` accepts `-0` via the float regex's `-? ( 0 )` arm.

Under the Core schema (§10.3), the int regex is `[-+]? [0-9]+`, which permits `-0`. So
`-0` resolves to `!!int` under Core and `!!float` under JSON — both behaviors are correct
for their respective schemas per the normative regex tables.

**Parser implementation choice:** follow the literal regex, not the worked example. This
matches de-facto behavior among mature JSON-schema YAML parsers (libyaml, PyYAML, etc.).

**Enforcement site:** `is_json_int()` in `src/schema.rs`.
