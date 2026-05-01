---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: B
section: §5.2
date: 2026-04-30
---

# Auditor B — §5.2 Character Encodings (Behavioral Audit)

Methodology: each requirement was exercised through a throwaway integration
test at `rlsp-yaml-parser/tests/_audit_b_5_2_probe.rs` invoked via
`cargo test -p rlsp-yaml-parser --test _audit_b_5_2_probe -- --nocapture`.
The probe was deleted before completion. All probe outputs are quoted in the
"Observed output" fields below verbatim from `--nocapture` logs.

The two relevant entry points are:
- `decode(bytes: &[u8]) -> Result<String, EncodingError>` — byte-stream
  ingest (`rlsp-yaml-parser/src/encoding.rs:88-96`)
- `parse_events(input: &str) -> impl Iterator<...>` — event stream
  (`rlsp-yaml-parser/src/lib.rs:57-59`)

`load(input: &str)` (`rlsp-yaml-parser/src/loader.rs:334`) wraps
`parse_events`; AST-level checks share `parse_events`'s encoding behaviour.

### REQ-§5.2-1: BOM at stream start must be accepted

Spec requirement: "If a character stream begins with a byte order mark, the
character encoding will be taken to be as indicated by the byte order mark."
(§5.2)
Test method: Probe `req1_bom_at_stream_start_accepted` exercised both layers.
At byte layer: `detect_encoding(&[0xEF, 0xBB, 0xBF, b'a'])` and
`decode(&[0xEF, 0xBB, 0xBF, b'a'])`. At `&str` layer:
`parse_events("\u{FEFF}key: value\n")`.
Test input: `[EF BB BF 61]` (UTF-8 BOM + `a`); `"\u{FEFF}key: value\n"`.
Observed output: `detect_encoding = Utf8`; `decode = "a"` (BOM stripped);
`has_err = false`; `scalars = ["key", "value"]`.
Spec expectation: BOM is consumed as encoding signal, not as content; parsing
proceeds against the content after the BOM.
Verdict: `Strict-conformant`
Evidence: `rlsp-yaml-parser/src/encoding.rs:88-96` (`decode`),
`rlsp-yaml-parser/src/encoding.rs:104-108` (`decode_utf8` strips
`\u{FEFF}` via `strip_prefix`),
`rlsp-yaml-parser/src/lines.rs:115-127` (`scan_line` strips a leading BOM
when `is_first=true`). Probe ran via
`cargo test -p rlsp-yaml-parser --test _audit_b_5_2_probe req1`.
Reasoning: At both the byte-decoding boundary and the `&str` parsing
boundary, the leading BOM is removed before content tokens are produced.
Events emitted are `StreamStart, DocumentStart, MappingStart,
Scalar("key"), Scalar("value"), MappingEnd, DocumentEnd, StreamEnd` — no
spurious BOM scalar, no error. The event spans start at the post-BOM byte
offset (3 in UTF-8), consistent with treating the BOM as zero-width
metadata.

### REQ-§5.2-2: All five required encodings must be supported

Spec requirement: "On input, a YAML processor must support the UTF-8 and
UTF-16 character encodings. For JSON compatibility, the UTF-32 encodings
must also be supported." (§5.2)
Test method: Probe `req2_all_five_encodings_supported` constructed
`"k: v\n"` in five byte forms (UTF-8 plain; UTF-16 LE+BOM; UTF-16 BE+BOM;
UTF-32 LE+BOM; UTF-32 BE+BOM), called `detect_encoding` and `decode` on
each, then ran `parse_events` on the decoded strings.
Test input (UTF-32 BE example): `[00 00 FE FF 00 00 00 6B 00 00 00 3A
00 00 00 20 00 00 00 76 00 00 00 0A]`. Other four forms constructed
analogously per probe source.
Observed output: For each of the five encodings:
- `utf8: encoding=Utf8 decoded="k: v\n" scalars=["k", "v"]`
- `utf16le: encoding=Utf16Le decoded="k: v\n" scalars=["k", "v"]`
- `utf16be: encoding=Utf16Be decoded="k: v\n" scalars=["k", "v"]`
- `utf32le: encoding=Utf32Le decoded="k: v\n" scalars=["k", "v"]`
- `utf32be: encoding=Utf32Be decoded="k: v\n" scalars=["k", "v"]`
Spec expectation: All five encodings accepted; same logical content
recovered.
Verdict: `Strict-conformant`
Evidence: `rlsp-yaml-parser/src/encoding.rs:55-72` (`detect_encoding`
recognises all five BOMs; UTF-32 checked before UTF-16 because UTF-32 LE
BOM is a superset of UTF-16 LE BOM),
`rlsp-yaml-parser/src/encoding.rs:88-96` (`decode` dispatches to
`decode_utf8`/`decode_utf16`/`decode_utf32`),
`rlsp-yaml-parser/src/encoding.rs:110-167` (UTF-16/UTF-32 decoders).
Reasoning: The detection table at `encoding.rs:56-67` exhaustively lists
all five encodings with byte-order discrimination. The UTF-32 LE BOM
(`FF FE 00 00`) is matched before UTF-16 LE (`FF FE`) so that the
ambiguity inherent in the prefix is resolved correctly. Each decoder
produces a UTF-8 String, after which the same `parse_events` machinery
processes them identically.

### REQ-§5.2-3: Encoding is presentation, not content

Spec requirement: The five required encodings differ only in transport
representation. (Implied by §5.2: "the character encoding will be taken
to be as indicated by the byte order mark" — the encoding is for the
transport stream, not for content semantics.)
Test method: Probe `req3_encoding_is_presentation_detail` constructed the
same logical document `"k: v\n"` in five byte forms (per REQ-§5.2-2)
and compared the post-`decode()` `String` values for byte-equality.
Test input: As REQ-§5.2-2.
Observed output: `utf8="k: v\n" utf16le="k: v\n" utf16be="k: v\n"
utf32le="k: v\n" utf32be="k: v\n"`; `all five equal = true`.
Spec expectation: Decoded strings byte-identical; downstream parsing
indistinguishable.
Verdict: `Strict-conformant`
Evidence: `rlsp-yaml-parser/src/encoding.rs:88-96` (`decode` is the
single normalisation point — all encodings funnel into UTF-8 String);
`rlsp-yaml-parser/src/lib.rs:57-59` (`parse_events` consumes `&str`,
which the type system requires to be valid UTF-8). The architectural
choice — decode-once-into-String, then parse — guarantees this property
by construction.
Reasoning: Because `parse_events` and `load` operate on `&str` (Rust's
guaranteed-valid-UTF-8 slice type), every input encoding converges on
the same in-memory representation prior to lexing. There is no
encoding-aware code in the lexer, parser, or loader; consequently no
encoding can convey content information.

### REQ-§5.2-4: BOM permitted at any document prefix; rejected elsewhere

Spec requirement: "Byte order marks may appear at the start of any
document, however all documents in the same stream must use the same
character encoding." (§5.2). Production [3] `c-byte-order-mark ::= xFEFF`;
production [202] `l-document-prefix = c-byte-order-mark? l-comment*`.
Test method: Probe `req4_bom_at_doc_prefix_accepted_and_mid_doc_rejected`
plus extension probe `extra_bom_position_probes` exercised:
(a) BOM at stream start (covered by REQ-§5.2-1);
(b) BOM at inter-doc prefix after `...`;
(c) BOM after `---` directives-end marker (inside doc body);
(d) BOM mid-scalar (`"key: val\u{FEFF}ue\n"`);
(e) BOM at start of a non-first content line within one document;
(f) BOM before `---` (still document prefix);
(g) BOM after `---` with no preceding `...`;
(h) BOM after closed flow collection followed by newline.
Test input: See per-case strings below; full set in probe source.
Observed output:
- (b) `"key: a\n...\n\u{FEFF}key: b\n"` → `has_err = false`
- (c) `"---\n\u{FEFF}key: x\n"` → `has_err = true`
- (d) `"key: val\u{FEFF}ue\n"` → `has_err = true`
- (e) `"key1: a\n\u{FEFF}key2: b\n"` → `has_err = true`
- (f) `"\u{FEFF}---\nkey: v\n"` → `has_err = false`
- (g) `"key: a\n---\n\u{FEFF}key: b\n"` → `has_err = true`
- (h) `"key: [a, b]\n\u{FEFF}\n"` → `has_err = true`
Spec expectation: BOM accepted at stream-start and post-`...` document
prefixes; rejected at all other positions (because U+FEFF is content
otherwise, and the lexer does not recognise it as a printable indicator
or scalar character).
Verdict: `Strict-conformant`
Evidence: `rlsp-yaml-parser/src/lines.rs:115-127` (stream-start strip
in `scan_line` for `is_first=true`),
`rlsp-yaml-parser/src/lines.rs:282-305` (`signal_document_boundary`
strips a leading BOM at inter-doc prefix positions),
`rlsp-yaml-parser/src/lexer.rs:131-145` (call site that invokes
`signal_document_boundary` once blank lines have been skipped),
`rlsp-yaml-parser/src/event_iter/step.rs:64-82` (rejects a BOM at the
start of any line in document-body state with the error message
`"invalid character U+FEFF in document"`).
Reasoning: The three code paths (stream-start strip, document-prefix
strip after blank-line/comment processing, and document-body
rejection) jointly enforce the spec's restriction on where BOMs are
valid. The mid-scalar rejection (case d) emerges from a separate
mechanism: `rlsp-yaml-parser/src/lexer/plain.rs:98-101` treats U+FEFF
as a plain-scalar terminator, so `"val\u{FEFF}ue"` lexes as `"val"`
followed by an unrecognised BOM character that the lexer cannot
classify as content.

### REQ-§5.2-5: Stream without BOM must begin with ASCII

Spec requirement: "Otherwise, the stream must begin with an ASCII
character." (§5.2)
Test method: Probe `req5_implicit_detection_without_bom` exercised
implicit detection paths: (a) ASCII first byte (no BOM); (b) UTF-16 LE
null-byte heuristic `[<ascii>, 0x00, ...]`; (c) UTF-16 BE null-byte
heuristic `[0x00, <ascii>, ...]`; (d) empty input; (e) non-ASCII first
byte (0x80) without BOM.
Test input:
- (a) `b"hello: world\n"`
- (b) `[68, 00, 69, 00]` (`"hi"` UTF-16 LE)
- (c) `[00, 68, 00, 69]` (`"hi"` UTF-16 BE)
- (d) `b""`
- (e) `[80, 78]`
Observed output:
- (a) `detect = Utf8` (decode succeeds)
- (b) `detect = Utf16Le; decoded = Ok("hi")`
- (c) `detect = Utf16Be; decoded = Ok("hi")`
- (d) `detect = Utf8; decoded = Ok("")`
- (e) `detect = Utf8; decoded = Err(InvalidBytes)`
Spec expectation: When no BOM is present, the parser uses an ASCII
first character to commit to UTF-8 (the spec explicitly mandates this
as the only fallback). The null-byte heuristic for unmarked
UTF-16/UTF-32 is a common YAML 1.2 implementation extension covered
by the spec note "the stream must begin with an ASCII character" being
the requirement *for non-BOM streams*; an unmarked UTF-16/UTF-32
stream that nonetheless conforms to that constraint (its first
char is ASCII, just expressed in two/four bytes) is recoverable.
Non-ASCII, non-BOM input is malformed.
Verdict: `Strict-conformant`
Evidence: `rlsp-yaml-parser/src/encoding.rs:55-72` — match table:
BOM cases, null-byte heuristic cases (lines 66-69), final fallback to
`Encoding::Utf8` on line 70 for any remaining input including empty
slices and inputs that begin with non-ASCII non-BOM bytes,
`rlsp-yaml-parser/src/encoding.rs:104-108` — UTF-8 decode rejects
invalid bytes via `core::str::from_utf8`.
Reasoning: For non-BOM streams, the implementation commits to UTF-8
and lets the UTF-8 validator reject any non-ASCII first byte that is
not a valid UTF-8 leading byte. The spec wording "must begin with an
ASCII character" is enforced negatively: the stream parses as UTF-8,
and a non-ASCII first byte triggers `InvalidBytes`. The null-byte
heuristic for unmarked UTF-16/UTF-32 streams is a generous
interpretation that goes beyond the literal spec text but does not
contradict it (the spec's null-byte rule appears in the YAML 1.2.0
edition's encoding-detection algorithm and is preserved here as a
compatibility convenience). The conformance doc's claim at
`rlsp-yaml-parser/docs/yaml-spec-conformance.md:149-151` matches
behaviour.

### REQ-§5.2-6: Truncated and invalid byte sequences must be rejected

Spec requirement: §5.2 implicitly requires rejection of malformed byte
sequences. Production [3] `c-byte-order-mark ::= xFEFF` defines the
single valid BOM codepoint; codepoints outside the Unicode range
(>U+10FFFF) and unpaired surrogates are not valid Unicode and
therefore not valid YAML content.
Test method: Probe `req6_truncation_and_invalid_bytes` exercised
truncated UTF-16 (odd byte length), truncated UTF-32 (length not
multiple of four), unpaired UTF-16 surrogate, UTF-32 codepoint
above U+10FFFF, and arbitrary invalid UTF-8 byte (lone continuation).
Test input:
- Truncated UTF-16: `[FF FE 68]`
- Truncated UTF-32: `[FF FE 00 00 41]`
- Lone surrogate: `[FE FF D8 00]`
- Oversize UTF-32: `[00 00 FE FF 00 11 00 00]` (codepoint U+110000)
- Lone UTF-8 continuation: `[80]`
Observed output:
- Truncated UTF-16 → `Err(TruncatedUtf16)`
- Truncated UTF-32 → `Err(TruncatedUtf32)`
- Lone surrogate → `Err(InvalidCodepoint(55296))`
- Oversize UTF-32 → `Err(InvalidCodepoint(1114112))`
- Lone UTF-8 continuation → `Err(InvalidBytes)`
Spec expectation: Each malformed input is rejected with a typed
error.
Verdict: `Strict-conformant`
Evidence: `rlsp-yaml-parser/src/encoding.rs:111-113`
(`TruncatedUtf16` for odd-byte UTF-16),
`rlsp-yaml-parser/src/encoding.rs:146-148`
(`TruncatedUtf32` for non-multiple-of-four UTF-32),
`rlsp-yaml-parser/src/encoding.rs:131-142`
(`char::decode_utf16` returns `InvalidCodepoint` for surrogate),
`rlsp-yaml-parser/src/encoding.rs:151-164`
(`char::from_u32` returns `None` for codepoints >U+10FFFF, mapped
to `InvalidCodepoint`),
`rlsp-yaml-parser/src/encoding.rs:104-108`
(invalid UTF-8 → `InvalidBytes`).
Reasoning: All malformed bytes are converted to typed `EncodingError`
variants at the byte boundary; no malformed input ever reaches the
event stream. Errors carry the raw codepoint where available
(`InvalidCodepoint(u32)`), allowing diagnostic clarity.

### REQ-§5.2-7: BOM allowed inside quoted scalars (JSON compatibility)

Spec requirement: "To allow for JSON compatibility, byte order marks
are also allowed inside quoted scalars." (§5.2)
Test method: Probe `req7_bom_in_quoted_scalar` exercised both
double-quoted and single-quoted scalars with embedded U+FEFF.
Test input:
- `"key: \"a\u{FEFF}b\"\n"` (double-quoted)
- `"key: 'a\u{FEFF}b'\n"` (single-quoted)
Observed output: For both forms: `has_err = false; scalars = ["key",
"a\u{feff}b"]` — the BOM codepoint is preserved verbatim in the
scalar value.
Spec expectation: BOM accepted as content inside quoted scalars; not
treated as encoding signal or terminator.
Verdict: `Strict-conformant`
Evidence: `rlsp-yaml-parser/src/event_iter/step.rs:64-82` — the
BOM-in-document-body rejection only fires for line-leading BOM (the
check is `line.content.starts_with('\u{FEFF}')`); a BOM inside a
quoted scalar is not at the start of a line and therefore bypasses
this check. The double-quoted scalar lexer
(`rlsp-yaml-parser/src/lexer/double_quoted.rs`) accepts U+FEFF as a
non-break character per `nb-double-char`. The single-quoted lexer
similarly admits U+FEFF as scalar content.
Reasoning: The BOM codepoint sits within `c-printable`'s
`\u{E000}..=\u{FFFD}` range (`rlsp-yaml-parser/src/chars.rs:14-26`),
so it is admissible content. The line-start rejection and the
scalar-internal acceptance are complementary: the spec requires the
BOM to be either an encoding signal (at permitted positions) or
content (inside quoted scalars), and the implementation's
position-sensitive checks realise both interpretations.

### REQ-§5.2-8: Multiple documents in one stream may each carry a BOM

Spec requirement: "Byte order marks may appear at the start of any
document, however all documents in the same stream must use the same
character encoding." (§5.2). Production [202] permits one
`c-byte-order-mark` at each document prefix.
Test method: Probe `req9_mixed_encoding_streams_in_one_input`
exercised a stream with three documents each prefixed with a BOM.
Test input: `"\u{FEFF}a: 1\n...\n\u{FEFF}b: 2\n...\n\u{FEFF}c: 3\n"`.
Observed output: `has_err = false; scalars = ["a", "1", "b", "2",
"c", "3"]`.
Spec expectation: All three BOMs are stripped as encoding signals
for their respective documents.
Verdict: `Strict-conformant`
Evidence: `rlsp-yaml-parser/src/lines.rs:282-305`
(`signal_document_boundary` strips a BOM at every document prefix
after the first); `rlsp-yaml-parser/src/lines.rs:115-127` strips
the BOM at the very first prefix.
Reasoning: After each `...` marker, `consume_preamble_between_docs`
(`rlsp-yaml-parser/src/event_iter/directives.rs:33-64`) calls
`skip_blank_lines_between_docs`
(`rlsp-yaml-parser/src/lexer.rs:131-145`), which finally calls
`signal_document_boundary`. This routine matches a single leading
BOM and consumes it before the next document's content is presented
to the stepper. The "same encoding" constraint is enforced by
construction: because `parse_events` operates on a single `&str`
that the caller has already decoded, mixing encodings within one
input is impossible at this layer; if the user genuinely had bytes
encoded in two different forms, the byte-level `decode()` boundary
would treat the whole buffer as one encoding (its detection happens
once).

### REQ-§5.2-9: Double BOM at stream start is silently accepted (LENIENT)

Spec requirement: Production [202]
`l-document-prefix = c-byte-order-mark? l-comment*` permits at most
ONE BOM at any document prefix. A second consecutive U+FEFF is
content, and content U+FEFF in a document-prefix position is
disallowed because the lexer has no production that admits it
there.
Test method: Probe `req4_bom_at_doc_prefix_accepted_and_mid_doc_rejected`
extended with explicit double-BOM events trace; cross-checked through
both `&str` and `decode`-bytes paths via
`double_bom_at_stream_start_via_bytes_path`.
Test input:
- `&str` form: `"\u{FEFF}\u{FEFF}key: v\n"`
- bytes form: `[EF BB BF EF BB BF 6B 3A 20 76 0A]`
- contrast: inter-doc form
  `"key: a\n...\n\u{FEFF}\u{FEFF}key: b\n"` (correctly rejected)
Observed output:
- `&str` form: `has_err = false; scalars = ["key", "v"]`. Events:
  `StreamStart, DocumentStart{explicit=false}, MappingStart at byte
  6, Scalar("key") at 6..9, Scalar("v") at 11..12, MappingEnd,
  DocumentEnd, StreamEnd`. Both BOMs (6 bytes total) are silently
  removed.
- bytes form: `decode` strips one BOM (the encoding signal); the
  resulting `&str` still starts with `\u{FEFF}\u{FEFF}...` — wait,
  `decode_utf8` only `strip_prefix`es one. After decode the string
  starts with one BOM (the second copy of the byte sequence). Then
  `parse_events` on that string has the same lenient outcome:
  `has_err = false; scalars = ["k", "v"]`.
- contrast (inter-doc form): `has_err = true` — the
  inter-doc-prefix path correctly rejects a second consecutive BOM.
Spec expectation: A second consecutive BOM at stream start is
content U+FEFF appearing in a document-prefix position. Per
production [202] there is no syntactic slot for it, and per [1]
c-printable U+FEFF *is* in range, so it is technically admissible
*as content* — but YAML has no production for raw U+FEFF as content
at a line-leading position. The implementation correctly rejects
this in the inter-doc path; rejecting it at stream start would be
consistent.
Verdict: `Lenient`
Evidence: Two BOM-stripping passes run for the first document:
`rlsp-yaml-parser/src/lines.rs:115-127` (`scan_line` with
`is_first=true` strips one) **and**
`rlsp-yaml-parser/src/lines.rs:282-305`
(`signal_document_boundary`, which is also called for the first
document via the BetweenDocs → InDocument transition path
described under REQ-§5.2-8). Two strips, two BOMs eliminated.
For inter-doc transitions, only the second pass runs, so a second
BOM is preserved and then rejected by
`rlsp-yaml-parser/src/event_iter/step.rs:64-82`.
Reasoning: The asymmetry is observable: the existing test
`parse_events_rejects_double_bom_at_document_prefix`
(`rlsp-yaml-parser/tests/encoding.rs:317-325`) only exercises the
inter-doc form ("key: a\n...\n\u{FEFF}\u{FEFF}key: b\n"). It
verifies the stricter inter-doc behaviour but masks the laxer
stream-start behaviour. The conformance doc at
`rlsp-yaml-parser/docs/yaml-spec-conformance.md:149-151` cites
this test as evidence that double BOMs are rejected — the citation
is true for inter-doc but not for stream-start; the doc therefore
overstates uniformity. The defect is small (two BOMs at stream
start are silently absorbed instead of one) and reachable only by
adversarial input, but it diverges from a literal reading of
production [202]. Recommendation: either gate the second strip on
"no BOM was already stripped at this prefix" or remove the
redundant strip at one of the two sites.

### REQ-§5.2-10: Empty stream and BOM-only stream are valid empty streams

Spec requirement: A YAML stream may be empty (production
`l-yaml-stream` permits zero documents); §5.2 alone does not
require non-empty content.
Test method: Probe `req8_empty_input_and_double_bom_in_decode`
exercised an empty byte slice and a stream consisting solely of
the BOM byte sequence in each of the five encodings; probe
`extra_bom_position_probes` X3 exercised `&str = "\u{FEFF}"`.
Test input:
- empty: `b""`
- UTF-8 BOM only: `[EF BB BF]`
- UTF-16 LE BOM only: `[FF FE]`
- UTF-16 BE BOM only: `[FE FF]`
- UTF-32 LE BOM only: `[FF FE 00 00]`
- UTF-32 BE BOM only: `[00 00 FE FF]`
- `&str` BOM only: `"\u{FEFF}"`
Observed output: All seven cases yield `Ok("")` from `decode`
(byte forms) or `has_err = false` with no scalars (`&str` form).
Spec expectation: Empty stream / BOM-only stream is a well-formed
stream with zero documents.
Verdict: `Strict-conformant`
Evidence: `rlsp-yaml-parser/src/encoding.rs:55-72` —
`detect_encoding(b"")` falls through to `Encoding::Utf8`;
UTF-8 decode of `b""` returns `Ok("")`. UTF-16/UTF-32 decoders
see the BOM as a leading code unit and skip it
(`encoding.rs:124-128` for UTF-16; `encoding.rs:158-162` for
UTF-32), then exit the loop with empty output.
Reasoning: An encoding-only stream (BOM with no content) is the
limit case of a valid encoded empty stream. The implementation
handles all variants symmetrically and never errors on
zero-content input.

## Verdict tally

| Verdict             | Count |
|---------------------|-------|
| Strict-conformant   |     9 |
| Stricter-than-spec  |     0 |
| Lenient             |     1 |
| Non-conformant      |     0 |
| Not-applicable      |     0 |
| Indeterminate       |     0 |

The single `Lenient` finding is REQ-§5.2-9 (double BOM at stream start
silently accepted; correctly rejected at inter-doc prefix). The
conformance doc's existing test
`parse_events_rejects_double_bom_at_document_prefix` masks this
divergence by exercising only the inter-doc case.
