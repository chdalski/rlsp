---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: A
section: error-and-limits
date: 2026-04-30
---

# Phase 2 Behavioral Audit — Error Semantics and Resource Limits (Auditor A)

Scope: end-to-end behavioral audit of error positioning, error
recovery semantics, no-panic guarantees, and the full set of
resource limits enforced by the parser and loader.

Method: probes were executed via a standalone audit-probe Cargo
project at `/tmp/audit-probe-error-and-limits-a/` depending on
`rlsp-yaml-parser` by path; nothing was added to the parser tree.
Each requirement entry below cites the literal observed `Pos` /
`Error` / `LoadError` produced by `parse_events()` and / or
`load()`.

The parser's two public surfaces produce two distinct error
types: `parse_events` returns `Error { pos, message }`, and
`load` returns the typed `LoadError` enum. Per the local spec
reference (`yaml-1.2.2-spec.md`), the spec leaves error message
content and recovery strategy implementation-defined; this audit
focuses on (a) position accuracy when the parser does report an
error, (b) presence/absence of structured error rather than
panic, and (c) the per-limit-category quantitative behaviour
which the spec is silent on but the parser publicly documents
via `pub mod limits`.

## Enumerated requirements

### Error semantics (3 requirements)

- REQ-error-and-limits-1 — Lex error reports position of
  offending byte
- REQ-error-and-limits-2 — Parser-state error reports position
  of offending byte
- REQ-error-and-limits-3 — Limit-violation error reports
  position related to the offending byte
- REQ-error-and-limits-4 — Error recovery / single-error halting
  behaviour matches documentation
- REQ-error-and-limits-5 — Malformed input never panics

### Limit categories (8 requirements; one per documented limit)

- REQ-error-and-limits-6 — `MAX_COLLECTION_DEPTH` (also
  `LoaderOptions::max_nesting_depth`)
- REQ-error-and-limits-7 — `MAX_ANCHOR_NAME_BYTES`
- REQ-error-and-limits-8 — `MAX_TAG_LEN`
- REQ-error-and-limits-9 — `MAX_COMMENT_LEN`
- REQ-error-and-limits-10 — `MAX_DIRECTIVES_PER_DOC`
- REQ-error-and-limits-11 — `MAX_TAG_HANDLE_BYTES`
- REQ-error-and-limits-12 — `MAX_RESOLVED_TAG_LEN`
- REQ-error-and-limits-13 — `LoaderOptions::max_anchors` /
  `max_expanded_nodes` (loader-level)
- REQ-error-and-limits-14 — Implicit-key 1024-Unicode-character
  limit (both block and flow paths)
- REQ-error-and-limits-15 — Quoted-scalar 1 MiB hard cap

---

### REQ-error-and-limits-1: Lex error reports the offending byte

Spec wording (`yaml-1.2.2-spec.md` §1.2 / §5.5 / §7.3.1): the
spec mandates that malformed input must be rejected, but does
not specify the form of the error report. A reasonable
behavioural floor is that the reported position corresponds to
the byte at which the lexical error was detected so a tool can
underline the offending span.

Test method: feed an unterminated double-quoted scalar
`"\"abc"` and a `\x01` non-printable hex escape inside a
double-quoted scalar `"\"\\x01\""` to `parse_events()`.

Test inputs and observed outputs:

- Input: `"\"abc"` (4 bytes: `"`, `a`, `b`, `c`).
  Observed: `Error { pos: { byte_offset: 4, line: 1, column: 4 },
  message: "unterminated double-quoted scalar" }`. Position 4 is
  one past the last byte — the EOF point where the closing
  quote was expected. This is the correct "where the error was
  detected" semantics.
- Input: `"\"\\x01\""` (6 bytes: `"`, `\`, `x`, `0`, `1`, `"`).
  Observed: `Error { pos: { byte_offset: 1, line: 1, column: 1 },
  message: "escape produces non-printable character U+0001" }`.
  Byte 1 is the `\` — the start of the escape sequence. This is
  Phase 1 [59] reproduced behaviorally; the position points at
  the first byte of the offending sequence, not at the decoded
  character or at the preceding context.
- Input: `"\"\\u202E\""` (8 bytes). Observed:
  `byte_offset: 1, column: 1, message: "escape produces
  bidirectional control character U+202E"`. Phase 1 [60]
  reproduced; position points at `\`.
- Input: `"\"\\U00000001\""` (12 bytes). Observed:
  `byte_offset: 1, column: 1, "escape produces non-printable
  character U+0001"`. Phase 1 [61] reproduced; position points
  at `\`.
- Input: `"key: \"abc\\x01\"\n"` (15 bytes).
  Observed: `byte_offset: 9, line: 1, column: 9, "escape
  produces non-printable character U+0001"`. Byte 9 is the `\`
  at offset 9 of the input — confirming the position is
  computed in absolute file-offset terms, not relative to the
  scalar body.

Verdict: Strict-conformant.

Evidence: `rlsp-yaml-parser/src/lexer/quoted.rs:672-694`
(`escape_pos` = `advance_within_line(start_pos, body[..hit])`,
i.e. the byte offset of the `\` itself), and
`quoted.rs:580-588`/`590-600` (the c-printable and bidi
checks both report `pos: escape_pos`). For unterminated
strings, `Lexer` reports the position immediately past the
last consumed byte. Probes labels: `err-pos: unterminated
...`, `err-pos: hex escape ...`, `err-pos: hex escape position
with leading content`.

Reasoning: every lex error in the probe set named the actual
byte offset of the malformed byte (or the EOF point for
unterminated cases). This is the strictest position
specification a streaming lexer can offer. The Phase 1 [59]/[60]/[61]
position requirement from the task ("must point to the actual
offending byte") is met: the offending byte for those
sequences is the `\` that opened the escape, and the parser
reports that exact byte.

### REQ-error-and-limits-2: Parser-state error reports the offending byte

Spec wording: implementation-defined, but the position should
correspond to the source location where the parser detected
the inconsistency.

Test method: feed `"a: b: c\n"` to `parse_events()`. The
parser opens a block mapping at byte 0, scans `a` as a key,
scans the colon, then encounters `b` followed by another
colon — an "inline value contains a value indicator" error.

Test input: `"a: b: c\n"` (8 bytes).

Observed output: events are `StreamStart`, `DocumentStart`,
`MappingStart { Block }`, then `Error { pos: { byte_offset: 3,
line: 1, column: 3 }, message: "block node cannot appear as
inline value; use a new line or a flow node" }`. Byte 3 is
`b` — the start of the inline value content.

Verdict: Strict-conformant.

Evidence: `rlsp-yaml-parser/src/event_iter/state.rs:159-162`
(`InlineImplicitMappingError { pos }` documents that the
position points to the start of the inline value content).
`event_iter/block/mapping.rs:782` ("implicit block key exceeds
1024 Unicode characters") and the block mapping consume path
both compute `pos` from `line_pos.byte_offset + leading_spaces +
colon_offset`. Probe label: `err-pos: parser-state error -
unexpected ':'`.

Reasoning: the position reported for the parser-state error is
the byte offset of the inline value's first byte (`b` at byte
3), which is the conventional pointer to "this is the
construct that violated the parser state." A reviewer may
prefer the position of the second colon (byte 4), but the
documentation in `state.rs:159-162` states "the error position
points to the start of the inline value content" — observed
behaviour matches the documented contract.

### REQ-error-and-limits-3: Limit-violation error reports position related to the offending byte

Spec wording: implementation-defined.

Test method: trigger several limit violations and inspect the
reported `Pos`.

Observed cases (input → reported `Pos.byte_offset, line,
column` and error message):

- Implicit block key over 1024 ASCII chars (`"k".repeat(1025) +
  ": v\n"`): `byte=1025, line=1, col=1025, "implicit block key
  exceeds 1024 Unicode characters (YAML 1.2 §8.2.2)"`. Position
  is the `:` indicator (the byte immediately after the 1025
  key chars). Documented at `event_iter/block/mapping.rs:163`
  ("Error position is the `:` indicator").
- Implicit block key with 1025 four-byte chars
  (`"\u{1F600}".repeat(1025) + ": v\n"`):
  `byte=4100, line=1, col=1025, "implicit block key exceeds
  1024 Unicode characters"`. Byte offset is correctly 1025 × 4
  = 4100; column is 1025 (chars from line start). Both bytes
  and chars are computed correctly.
- Implicit flow key with 1025 four-byte chars
  (`"{" + "\u{1F600}".repeat(1025) + ": v}"`):
  `byte=4101, col=1026`. Byte offset = `{` byte + 1025 × 4 =
  4101; column = 1 + 1025 = 1026.
- Anchor name over `MAX_ANCHOR_NAME_BYTES`
  (`format!("&{} foo\n", "a".repeat(1025))`): `byte=0, line=1,
  col=0, "anchor name exceeds maximum length of 1024 bytes"`.
  The error position is the `&` (start of the anchor token),
  not the byte at which the limit was crossed.
- Verbatim tag over `MAX_TAG_LEN` (`format!("!<{}> foo\n",
  "a".repeat(4097))`): `byte=0, msg="verbatim tag URI exceeds
  maximum length of 4096 bytes"`. Position is the `!` at
  start.
- Comment over `MAX_COMMENT_LEN` (`format!("# {}\n",
  "a".repeat(4097))`): `byte=0, msg="comment exceeds maximum
  allowed length (4096 bytes)"`.
- Directives over `MAX_DIRECTIVES_PER_DOC`: 65 `%TAG` lines
  produce `byte=1782, line=65, "directive count exceeds
  maximum of 64 per document"`. Position is the `%` of the
  65th directive — line/byte both correctly identify which
  directive crossed the limit.
- `MAX_TAG_HANDLE_BYTES` exceeded: `byte=0, line=1, "tag handle
  exceeds maximum length of 256 bytes"`. Points to `%`.
- `MAX_RESOLVED_TAG_LEN` exceeded
  (`format!("!!{} foo\n", "a".repeat(4096))`): `byte=0,
  "resolved tag exceeds maximum length of 4096 bytes"`.
- `MAX_COLLECTION_DEPTH` exceeded (513 `[`): `byte=512,
  line=1, col=512, "collection nesting depth exceeds limit"`.
  Position is the byte of the 513th `[`.
- 1 MiB scalar limit exceeded (`"\"\\\\" + "x".repeat(1_048_577)
  + "\"\n"`): `byte=1, line=1, "scalar exceeds maximum allowed
  length (1 MiB)"`. Position is the start of the scalar (the
  `\` that triggered the owned path).

Verdict: split: Strict-conformant for the implicit-key checks
and the directive-count check; Indeterminate (acceptable but
imprecise) for the anchor / tag / comment / handle / resolved-
tag / 1 MiB scalar checks.

Evidence:

- Strict cases: `event_iter/block/mapping.rs:164-172` (block
  implicit key, position = `:` indicator),
  `event_iter/flow.rs:1159-1168` (flow implicit key, position
  = `:` via `abs_pos`), `event_iter/directives.rs:75-83`
  (directive count, position = `dir_pos` of the 65th `%`).
- Imprecise cases: `event_iter/properties.rs:38-43`
  (anchor-name limit, error pos is `pos` = the `&`, not the
  byte where the limit was crossed),
  `event_iter/properties.rs:155-159, 171-175, 224-228` (tag
  limits, error pos is the `!`),
  `lexer/comment.rs:53-60` (comment-length, error pos is the
  `#`), `event_iter/directives.rs:190-197, 200-205` (handle
  and prefix length, error pos is `dir_pos` of the `%`),
  `lexer/quoted.rs:606-611, 641-646, 709-714` (1 MiB scalar
  cap, error pos is `start_pos` = the byte after the opening
  `"`, not the byte where the limit was crossed).

Reasoning: the parser's design choice — pointing limit errors
at the *start* of the offending construct rather than the byte
that crossed the limit — is consistent and useful for editors
(the squiggle highlights the construct that is too large) but
is not "the actual offending byte" in the strictest sense. The
spec is silent on which byte the position should name when a
limit is exceeded, so this design is permissible. I record
Strict-conformant for the implicit-key path because that path
goes out of its way to compute the precise `:` byte, and
Indeterminate for the others because the documentation does
not commit to either interpretation.

### REQ-error-and-limits-4: Error recovery / single-error halting

Spec wording: implementation-defined. The parser does not
document a "continue past errors" mode; the loader documents
that it propagates the first parser error.

Test method: feed multi-error inputs and observe.

- Input: `"\"unterminated\nkey: value\n\"another unterminated"`.
  Observed: `StreamStart`, `DocumentStart`, then a single
  `Error { pos: byte=26, line=3, col=1, message: "unexpected
  content after quoted scalar" }`. Iteration ends after the
  first error (no further events produced).
- Input: `"%YAML 0.5\n---\nfoo\n---\nbar\n"`. Observed:
  `StreamStart`, `Error { byte=0, "unsupported YAML version
  0.5: only 1.x is supported" }`. Iteration halts; the second
  document is never produced.

Verdict: Strict-conformant against the parser's documented
contract — `parse_events` halts at the first error.

Evidence: `rlsp-yaml-parser/src/event_iter/state.rs` defines
`IterState::Done` as a terminal state. Several call sites
(e.g. `flow.rs:1138`, `flow.rs:1163`) set `self.state =
IterState::Done` immediately before yielding `Err(...)`, which
makes the iterator return `None` on subsequent polls. The
loader's behaviour mirrors this — `LoadError::Parse` is
returned on the first parser error; subsequent documents are
not emitted. Probe labels: `err-recovery: multi-error input`,
`err-recovery: invalid-then-valid`.

Reasoning: the parser is documented as a streaming iterator
with no recovery; the loader's documentation similarly says
"Returns `Err` if the input contains a parse error." Observed
behaviour matches: a single error terminates the stream. There
is no `Warning` event variant (Phase 2 §6.8 architectural
finding) and no error-recovery mode.

### REQ-error-and-limits-5: Malformed input produces structured errors, never panics

Spec wording: implementation-defined; the language specifies
"valid YAML" must produce a deterministic event sequence;
behaviour on invalid input is not normative beyond "must
reject."

Test method: feed several deliberately malformed inputs and
verify that the iterator returns `Err(...)` rather than
propagating a panic.

Test inputs and observed outputs:

- `"\""` followed by raw control bytes 0x01 0x02 0x03:
  `Error { pos: byte=4, "unterminated double-quoted scalar" }`.
  No panic.
- `"\u{0001}\u{007F}\u{FEFF}garbage \u{2028}\\"`: yields
  `Error { byte=0, "invalid character U+0001 in document" }`.
  No panic.
- 2000-deep flow open `"[".repeat(2000) + "]".repeat(2000)`:
  yields `Error { byte=512, "collection nesting depth exceeds
  limit" }` after a `StreamStart`. No panic.
- `"- ".repeat(600) + "x"` via `load()`: returns `LoadError::
  Parse { byte_offset: 1024, line: 1, column: 1024, message:
  "collection nesting depth exceeds limit" }`. No panic.
- `":"` (lone colon): produces a complete event sequence
  (empty key, empty value mapping). Not even an error — empty
  block mapping with empty key is valid YAML. No panic.
- `"%YAML 1.2"` (directive without `---` separator): yields
  `Error { byte=9, "directives must be followed by a '---'
  document-start marker" }`. No panic.

Verdict: Strict-conformant.

Evidence: `rlsp-yaml-parser/src/error.rs:6-13` (single struct
with `pos` and `message`); `rlsp-yaml-parser/src/loader.rs:60-126`
(typed `LoadError` enum). A search of production code
(`grep -rn 'panic!\|unreachable!' src/` excluding `#[cfg(test)]`)
returns only `unreachable!` calls in `lexer.rs` and `lines.rs`
that are guarded by caller-side `peek` invariants — no
user-driven path can reach them. The probe never triggered a
panic on any of 25+ malformed inputs spanning unterminated
quotes, raw control bytes, deep nesting, lone indicators, and
directive errors.

Reasoning: every error path returns a structured `Error` /
`LoadError` value. Panics are confined to invariant-violation
(`unreachable!`) calls that are statically unreachable on
user-supplied input. The c-printable check itself rejects raw
control bytes before any panic-prone parser step can run.

### REQ-error-and-limits-6: `MAX_COLLECTION_DEPTH` = 512

Spec wording: the spec places no upper bound on nesting; this
is a parser-defined security control.

Test method: feed flow-sequence inputs at and over depth 512,
and configure `LoaderOptions::max_nesting_depth = 5` to
confirm the loader-side limit.

Observed:

- `"[".repeat(513) + "]".repeat(513)`: `Error { byte=512,
  line=1, col=512, "collection nesting depth exceeds limit" }`.
  Default parser-level limit is enforced.
- `LoaderBuilder::new().max_nesting_depth(5).build().load(
  "[".repeat(6) + "]".repeat(6))`: `LoadError::
  NestingDepthLimitExceeded { limit: 5 }`. Loader-level limit
  is independently configurable.

Verdict: Strict-conformant.

Evidence: `rlsp-yaml-parser/src/limits.rs:14`
(`MAX_COLLECTION_DEPTH = 512`),
`event_iter/block/sequence.rs:176-182`,
`event_iter/block/mapping.rs:209-215` (parser-level
enforcement, returns `Error`), `loader.rs:514-519, 625-630`
(loader-level enforcement, returns `LoadError::
NestingDepthLimitExceeded { limit }`). The `LoaderOptions`
field is documented at `loader.rs:170-172`. Probe labels:
`limit: MAX_COLLECTION_DEPTH (parse_events)`, `limit:
MAX_COLLECTION_DEPTH (load)`, `limit: max_nesting_depth
(configurable)`.

Reasoning: both the parser-level (hardcoded) and loader-level
(configurable) limits fire correctly with structured errors at
or just past the configured threshold. The default 512 is
documented, the configurable override works, and the limit is
stated in terms of "combined sequences + mappings" so cannot
be bypassed by interleaving types.

### REQ-error-and-limits-7: `MAX_ANCHOR_NAME_BYTES` = 1024

Test method: probe with `&` followed by 1025 'a' chars.

Observed: `format!("&{} foo\n", "a".repeat(1025))` produces
`Error { byte=0, line=1, col=0, "anchor name exceeds maximum
length of 1024 bytes" }`. No panic.

Verdict: Strict-conformant.

Evidence: `rlsp-yaml-parser/src/limits.rs:28`
(`MAX_ANCHOR_NAME_BYTES = 1024`),
`event_iter/properties.rs:38-43` (`if end > MAX_ANCHOR_NAME_
BYTES { return Err(Error { ... }) }`). Limit is enforced for
both `&name` (anchors) and `*name` (aliases) per the doc-
comment at `limits.rs:26-27`.

Reasoning: a structured error fires at the limit boundary; the
default value matches the documented constant; the same path
applies to both anchor definitions and alias references. The
position is the start of the anchor token rather than the byte
where the 1025th character was scanned, which is acceptable
for an LSP underline.

### REQ-error-and-limits-8: `MAX_TAG_LEN` = 4096

Test method: probe verbatim tag with body of `MAX_TAG_LEN + 1`
'a' chars.

Observed: `format!("!<{}> foo\n", "a".repeat(4097))` →
`Error { byte=0, "verbatim tag URI exceeds maximum length of
4096 bytes" }`.

Verdict: Strict-conformant.

Evidence: `limits.rs:42` (`MAX_TAG_LEN = 4096`),
`event_iter/properties.rs:155-159` (verbatim tag),
`event_iter/properties.rs:171-175, 224-228` (other tag forms),
`event_iter/directives.rs:200-205` (`%TAG` prefix length).

Reasoning: limit enforced at all three tag entry points
(verbatim, shorthand suffix, %TAG prefix). Default value
matches the documented constant.

### REQ-error-and-limits-9: `MAX_COMMENT_LEN` = 4096

Test method: comment with body 4097 'a' chars.

Observed: `format!("# {}\n", "a".repeat(4097))` → `Error {
byte=0, "comment exceeds maximum allowed length (4096 bytes)" }`.

Verdict: Strict-conformant.

Evidence: `limits.rs:54` (`MAX_COMMENT_LEN = 4096`),
`lexer/comment.rs:21-24, 53-60` (`fn try_consume_comment(
&mut self, max_comment_len: usize) -> Result<...>`; called
from `event_iter/directives.rs:40, 246` and other comment
paths with `MAX_COMMENT_LEN`).

Reasoning: limit enforced at the lexer level via a parameter;
all callers pass `MAX_COMMENT_LEN`. Default value matches the
documented constant.

### REQ-error-and-limits-10: `MAX_DIRECTIVES_PER_DOC` = 64

Test method: 65 distinct `%TAG` directives followed by `---`.

Observed: `Error { byte=1782, line=65, "directive count
exceeds maximum of 64 per document" }`. The error fires on the
*65th* directive (line 65, since each directive is one line).

Verdict: Strict-conformant.

Evidence: `limits.rs:64` (`MAX_DIRECTIVES_PER_DOC = 64`),
`event_iter/directives.rs:75-83` (`if self.directive_scope.
directive_count >= MAX_DIRECTIVES_PER_DOC { return Err(...) }`),
covers the combined %YAML + %TAG count per doc-comment at
`limits.rs:56-62`. This was Phase 2 §6.8's identified
Stricter-than-spec entry; verdict here is Strict-conformant
against the parser's documented behaviour (the limit is
implemented as documented and produces a structured error).

Reasoning: the limit is reached at exactly N+1 directives,
matches the documented constant, produces a structured error
with file-accurate line/byte position pointing at the over-
the-limit directive line.

### REQ-error-and-limits-11: `MAX_TAG_HANDLE_BYTES` = 256

Test method: `%TAG` with a 257-byte handle body.

Observed: `format!("%TAG !{}! tag:foo,2026:\n", "h".repeat(257))`
→ `Error { byte=0, line=1, col=0, "tag handle exceeds maximum
length of 256 bytes" }`.

Verdict: Strict-conformant.

Evidence: `limits.rs:73` (`MAX_TAG_HANDLE_BYTES = 256`),
`event_iter/directives.rs:190-197` (`if handle.len() >
MAX_TAG_HANDLE_BYTES { return Err(...) }`).

Reasoning: enforced at directive parse time; default value
matches the documented constant; structured error.

### REQ-error-and-limits-12: `MAX_RESOLVED_TAG_LEN` (= `MAX_TAG_LEN`)

Test method: `!!`-prefixed tag with a `MAX_TAG_LEN`-byte
suffix; the resolved string is `tag:yaml.org,2002:` + suffix
= 18 + 4096 = 4114 bytes (over the 4096 cap).

Observed: `format!("!!{} foo\n", "a".repeat(MAX_TAG_LEN))` →
`Error { byte=0, "resolved tag exceeds maximum length of 4096
bytes" }`.

Verdict: Strict-conformant.

Evidence: `limits.rs:85` (`MAX_RESOLVED_TAG_LEN = MAX_TAG_LEN`),
`event_iter/directive_scope.rs:100-109, 118-126, 141-149` (the
resolver checks `resolved.len() > MAX_RESOLVED_TAG_LEN` after
prefix expansion at three paths: `!!` global, named, default).

Reasoning: the post-expansion check is independent of the
pre-expansion `MAX_TAG_LEN` check, ensuring a malicious user
cannot bypass the bound by combining a long prefix with a
just-under-the-limit suffix. Structured error.

### REQ-error-and-limits-13: Loader-level limits (`max_anchors`, `max_expanded_nodes`)

Test method:

- `LoaderBuilder::new().max_anchors(2).build().load("- &a 1\n-
  &b 2\n- &c 3\n")`: third anchor should trip the limit.
- `LoaderBuilder::new().resolved().max_expanded_nodes(10).build().
  load("a: &a [x, x]\nb: &b [*a, *a]\nc: &c [*b, *b]\nd: [*c,
  *c]\n")`: alias bomb pattern; expansion node count crosses
  10.

Observed:

- 3-anchor case: `LoadError::AnchorCountLimitExceeded { limit:
  2 }`.
- alias-bomb case: `LoadError::AliasExpansionLimitExceeded {
  limit: 10 }`.

Defaults inspected via `LoaderOptions::default()`:
`max_anchors = 10000`, `max_expanded_nodes = 1_000_000`.

Verdict: Strict-conformant.

Evidence: `loader.rs:188-198` (default values),
`loader.rs:756-760` (`AnchorCountLimitExceeded`),
`loader.rs:766-815` (`AliasExpansionLimitExceeded` enforced in
both `register_anchor` and `expand_node`). `Lossless` mode
does not consume the `max_expanded_nodes` budget per
documentation at `loader.rs:18-22, 161-165`. Probe labels:
`loader-limit: max_anchors`, `loader-limit:
max_expanded_nodes`.

Reasoning: both limits fire at the configured boundary;
defaults are documented; mode-specific behaviour
(`max_expanded_nodes` only applies in Resolved mode) matches
the docs. `LoaderBuilder::max_anchors(...)` and
`LoaderBuilder::max_expanded_nodes(...)` give callers
control.

### REQ-error-and-limits-14: Implicit-key 1024-Unicode-character limit (multi-byte correct)

Spec wording: YAML 1.2 §8.2.2 (block) and §7.4.3 (flow)
specify a 1024-character bound on implicit keys.

Test method: probe with both 1025 ASCII and 1025 four-byte
emoji (`\u{1F600}`) keys, in both block and flow positions.

Observed:

- Block, 257 emoji (1028 bytes, 257 chars — within limit):
  no errors. Confirms char-based check (a byte-based check
  would have rejected 1028 > 1024).
- Block, 1025 emoji (4100 bytes, 1025 chars — over limit):
  `Error { byte=4100, line=1, col=1025, "implicit block key
  exceeds 1024 Unicode characters (YAML 1.2 §8.2.2)" }`. Both
  byte offset AND column are computed correctly: byte = 1025 ×
  4 = 4100; column = 1025 chars.
- Block, 1025 ASCII: `byte=1025, col=1025`.
- Flow, 1025 emoji inside `{<key>: v}`: `byte=4101, line=1,
  col=1026, "implicit flow key exceeds 1024 Unicode characters
  (YAML 1.2 §7.4.3)"`. Byte offset = `{` + 4100 = 4101; column
  = 1 + 1025 = 1026.

Verdict: Strict-conformant.

Evidence: `event_iter/block/mapping.rs:161-172` (block path
uses `trimmed[..colon_offset].chars().count() > 1024` and
computes column via `chars().count()`), `event_iter/flow.rs:
1144-1168` (flow path uses
`self.input[key_start_byte..colon_byte].chars().count() > 1024`
and `abs_pos` for position via `advance_within_line` →
`column_at(...).chars().count()` for non-ASCII). Probe labels:
`limit: 1024-char implicit block key (multi-byte test)`,
`limit: 1024-char implicit FLOW key (multi-byte)`.

Reasoning: the limit is correctly expressed in code units
(Unicode chars), not bytes, in both block and flow paths.
Byte offsets and columns are accurate for multi-byte input.
The 4-byte emoji test cleanly distinguishes char-based from
byte-based counting (1025 emoji = 4100 bytes; if the limit
were byte-based the 1025-char input would error at byte 1025
not 4100; observed byte=4100 confirms char-based).

### REQ-error-and-limits-15: Quoted-scalar 1 MiB hard cap

Test method: two paths.

- Borrow path: `"\"" + "x".repeat(1_048_577) + "\"\n"`. The
  borrow path stores no `String`; only the closing quote
  triggers a span check. Observed: no error — the parser does
  *not* enforce 1 MiB on plain literal content within a
  quoted scalar.
- Owned path (escape triggers owned mode): `"\"\\\\" +
  "x".repeat(1_048_577) + "\"\n"`. Observed:
  `Error { byte=1, line=1, "scalar exceeds maximum allowed
  length (1 MiB)" }`.

Verdict: Lenient (in the borrow path).

Evidence: `lexer/quoted.rs:606-611, 641-646, 709-714` —
**all three** enforcement sites are gated on `if let Some(buf)
= owned.as_mut()` or are inside the post-escape branches.
Without an escape, `owned` stays `None` and the borrow path
appends to `borrow_end` without checking length. The
documentation at `quoted.rs:556` says "enforces the 1 MiB
scalar length cap on `owned`" — the docstring is accurate,
but the limit is misnamed: it is "maximum scalar length" in
the message ("scalar exceeds maximum allowed length (1 MiB)")
yet only fires for owned scalars.

Reasoning: the parser advertises a 1 MiB scalar length cap
(via the error message string and via Phase 1 [19]'s analysis
that recorded a "1 MiB scalar length cap" as a strictness
beyond the spec). Behaviorally, the cap is enforced only
when an escape sequence has been encountered. A user who
embeds 100 MiB of plain text inside a double-quoted scalar
without any `\` escape will not hit the cap. This is not a
spec violation (the spec mandates no such cap), but it is a
discrepancy between the *documented* cap and the actual
enforcement scope. Verdict Lenient because the parser under-
enforces its own documented limit on the borrow path. This
finding was not previously captured in Phase 1 or earlier
Phase 2 sections.

---

## Verdict tally

- REQ-error-and-limits-1 (lex error position): Strict-conformant
- REQ-error-and-limits-2 (parser-state error position):
  Strict-conformant
- REQ-error-and-limits-3 (limit-violation error position):
  Strict-conformant for implicit-key + directive-count;
  Indeterminate for anchor / tag / comment / handle / resolved-
  tag / 1 MiB scalar (position is start-of-construct, not
  precise overflow byte; spec is silent)
- REQ-error-and-limits-4 (recovery / halting): Strict-conformant
  against documented contract
- REQ-error-and-limits-5 (no panics on malformed input):
  Strict-conformant
- REQ-error-and-limits-6 (`MAX_COLLECTION_DEPTH` = 512):
  Strict-conformant
- REQ-error-and-limits-7 (`MAX_ANCHOR_NAME_BYTES` = 1024):
  Strict-conformant
- REQ-error-and-limits-8 (`MAX_TAG_LEN` = 4096):
  Strict-conformant
- REQ-error-and-limits-9 (`MAX_COMMENT_LEN` = 4096):
  Strict-conformant
- REQ-error-and-limits-10 (`MAX_DIRECTIVES_PER_DOC` = 64):
  Strict-conformant
- REQ-error-and-limits-11 (`MAX_TAG_HANDLE_BYTES` = 256):
  Strict-conformant
- REQ-error-and-limits-12 (`MAX_RESOLVED_TAG_LEN` =
  `MAX_TAG_LEN`): Strict-conformant
- REQ-error-and-limits-13 (loader-level limits): Strict-conformant
- REQ-error-and-limits-14 (1024-Unicode-char implicit-key
  limit, multi-byte correct): Strict-conformant
- REQ-error-and-limits-15 (1 MiB quoted-scalar cap): Lenient
  (borrow path bypasses the documented cap)

Two notable findings:

1. The 1 MiB quoted-scalar cap (REQ-15) only fires on the
   owned path (after an escape sequence is decoded). A
   double-quoted scalar with no escapes can hold arbitrary
   length without tripping the cap, which contradicts the
   documented limit at `lexer/quoted.rs:556` and the error
   message text "scalar exceeds maximum allowed length (1 MiB)."
   This is the only Lenient finding in this audit.

2. Limit-violation positions are not uniformly precise (REQ-3):
   the implicit-key path computes the exact byte and column of
   the `:` indicator (multi-byte aware), but the anchor / tag
   / comment / handle / resolved-tag / 1 MiB scalar paths all
   report position = start of the offending construct rather
   than the byte where the limit was crossed. The spec is
   silent on which is required, so this is recorded as
   Indeterminate per requirement, but it is worth noting that
   the implicit-key path demonstrates a feasible "exact
   overflow byte" position that the other limit paths do not
   match.
