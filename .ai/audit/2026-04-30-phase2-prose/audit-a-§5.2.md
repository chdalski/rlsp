---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: A
section: §5.2
date: 2026-04-30
---

# Phase 2 Behavioral Audit — §5.2 Character Encodings

Methodology: each requirement was exercised by constructing a small input,
running it through the parser via a standalone audit-probe project at
`/tmp/audit-probe/` (Cargo crate that depends on `rlsp-yaml-parser` via path
dependency, with a single binary `src/main.rs` that prints observed output for
each REQ scenario). The probe was executed via `cargo run --quiet`. No files
were added or modified inside `/workspace/rlsp-yaml-parser/`. The probe's
captured stdout is the source of every "Observed output" field below.

The parser exposes encoding handling through two public entry points:

- `rlsp_yaml_parser::encoding::detect_encoding(&[u8]) -> Encoding`
- `rlsp_yaml_parser::encoding::decode(&[u8]) -> Result<String, EncodingError>`
- `rlsp_yaml_parser::parse_events(&str)` and `rlsp_yaml_parser::load(&str)`,
  both of which take already-decoded UTF-8 `&str`.

The architectural split is significant: encoding detection and decoding to
UTF-8 happen in `encoding.rs` (caller-driven for byte-oriented inputs), while
the textual entry points (`parse_events`, `load`) accept `&str` and perform
their own BOM-stripping at line-buffer level (`lines.rs`).

---

### REQ-§5.2-1: BOM at stream start must be accepted as encoding signal, not as content

Spec requirement: "On input, a YAML processor must support the UTF-8 and
UTF-16 character encodings. […] If a character stream begins with a byte
order mark, the character encoding will be taken to be as indicated by the
byte order mark." (`§5.2`, lines 1521–1526 of the local spec). Combined with
production `[#] c-byte-order-mark ::= xFEFF` (line 1564), the BOM at stream
start is normatively a presentation artifact, not a content character.

Test method: Two-stage test. Stage 1: pass UTF-8 BOM bytes through
`encoding::decode` and inspect whether U+FEFF appears in the returned
`String`. Stage 2: feed the resulting `&str` to `parse_events` and read the
event sequence. The BOM must not appear as content and must not produce a
parse error.

Test input:

- Bytes: `[0xEF, 0xBB, 0xBF, 0x6B, 0x65, 0x79, 0x3A, 0x20, 0x76, 0x61, 0x6C, 0x75, 0x65, 0x0A]`
  (UTF-8 BOM followed by `key: value\n`).
- A separate `&str` test path: `"\u{FEFF}key: value\n"` passed directly to
  `parse_events`, exercising `lines.rs` stream-start BOM stripping.

Observed output:

- `detect_encoding(...) = Utf8`.
- `decode(...) = Ok("key: value\n")`. Result does not start with U+FEFF.
- `parse_events` over the decoded string yields no errors and emits scalars
  `["key", "value"]`.
- Existing test in `tests/encoding.rs:170-179` independently verifies the
  `&str` path with a leading U+FEFF.

Spec expectation: The BOM is consumed as the encoding signal and does not
appear in the parsed event stream.

Verdict: Strict-conformant

Evidence:

- BOM-strip in `decode_utf8` for byte-oriented inputs:
  `rlsp_yaml_parser/src/encoding.rs:104-108` — `s.strip_prefix('\u{FEFF}').unwrap_or(s)`.
- BOM-strip in `decode_utf16`: `rlsp_yaml_parser/src/encoding.rs:124-128`
  matches a leading `0xFEFF` u16 unit and slices it off.
- BOM-strip in `decode_utf32`: `rlsp_yaml_parser/src/encoding.rs:150,158-161`
  uses a `skip_bom` flag that strips one leading `0xFEFF` codepoint.
- BOM-strip at stream-start in `lines.rs:115-117`:
  `if is_first && remaining.starts_with('\u{FEFF}') { ... }` skips 3 bytes
  before line scanning.
- Probe execution: `cargo run` from `/tmp/audit-probe/`, REQ-§5.2-1 section
  of stdout reports `decoded: "key: value\n" (starts with FEFF? false)` and
  `events ok? true` with `scalars: ["key", "value"]`.

Reasoning: The spec's normative sentence "the character encoding will be
taken to be as indicated by the byte order mark" requires the BOM to be
treated as encoding metadata rather than content. The decoder strips the BOM
at decoding time (three independent code paths covering UTF-8, UTF-16, and
UTF-32). The lines layer additionally strips a residual U+FEFF when callers
hand in a pre-decoded `&str` whose first character is U+FEFF — this covers
the case where a UTF-8 file with BOM is read as text via standard library
functions that do not strip it. Both paths produced "key: value\n" as the
observable text and `["key", "value"]` as the scalar sequence, with the BOM
absent from the event stream. Spec satisfied.

---

### REQ-§5.2-2: Implementation must support UTF-8, UTF-16-LE, UTF-16-BE, UTF-32-LE, UTF-32-BE

Spec requirement: "On input, a YAML processor must support the UTF-8 and
UTF-16 character encodings. For JSON compatibility, the UTF-32 encodings
must also be supported." (`§5.2`, lines 1521–1523). The detection table at
lines 1542–1553 enumerates BOM and ASCII-prefix patterns for all five
forms.

Test method: For each of the five encodings, encoded the YAML document
`"k: 1\n"` with the appropriate BOM and verified two things: (1)
`detect_encoding` returned the matching `Encoding` variant, and (2)
`decode` produced the UTF-8 string `"k: 1\n"` without error.

Test input (BOMs prepended where shown):

- UTF-8 default: `[0x6B, 0x3A, 0x20, 0x31, 0x0A]`.
- UTF-16-LE with BOM: `[0xFF, 0xFE, 0x6B, 0x00, 0x3A, 0x00, 0x20, 0x00, 0x31, 0x00, 0x0A, 0x00]`.
- UTF-16-BE with BOM: `[0xFE, 0xFF, 0x00, 0x6B, 0x00, 0x3A, 0x00, 0x20, 0x00, 0x31, 0x00, 0x0A]`.
- UTF-32-LE with BOM: `[0xFF, 0xFE, 0x00, 0x00, 0x6B, 0x00, 0x00, 0x00, 0x3A, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x31, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00]`.
- UTF-32-BE with BOM: `[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, 0x6B, 0x00, 0x00, 0x00, 0x3A, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x31, 0x00, 0x00, 0x00, 0x0A]`.

Observed output:

- UTF-8: `detect=Utf8`, `decoded="k: 1\n"`.
- UTF-16-LE: `detect=Utf16Le`, `decoded="k: 1\n"`.
- UTF-16-BE: `detect=Utf16Be`, `decoded="k: 1\n"`.
- UTF-32-LE: `detect=Utf32Le`, `decoded="k: 1\n"`.
- UTF-32-BE: `detect=Utf32Be`, `decoded="k: 1\n"`.

Spec expectation: Each encoding round-trips to the same UTF-8 string and is
classified into the matching `Encoding` variant.

Verdict: Strict-conformant

Evidence:

- `Encoding` enum has five variants in `rlsp_yaml_parser/src/encoding.rs:5-16`
  (Utf8, Utf16Le, Utf16Be, Utf32Le, Utf32Be).
- `detect_encoding` matches BOM bytes for all five in `encoding.rs:55-72`,
  with UTF-32 BOMs checked before UTF-16 (the comment at lines 51-53
  explicitly states this — UTF-32-LE BOM `FF FE 00 00` is a superset of
  UTF-16-LE BOM `FF FE`).
- `decode` dispatches on detected encoding in `encoding.rs:88-96` and
  delegates to `decode_utf8`, `decode_utf16` (with `Endian` parameter), or
  `decode_utf32` (also `Endian`-parameterised).
- `decode_utf16` at `encoding.rs:110-143` decodes pairs and applies
  `char::decode_utf16` for surrogate handling.
- `decode_utf32` at `encoding.rs:145-167` decodes 4-byte chunks and uses
  `char::from_u32` for codepoint validation.
- Probe execution: REQ-§5.2-2 section of stdout shows all five
  detect/decode pairs reported correctly.

Reasoning: The spec's "must support UTF-8 and UTF-16" plus "UTF-32 encodings
must also be supported" requires five encodings at minimum. The parser
defines exactly those five variants in its `Encoding` enum and provides a
decode path for each. Every test input round-tripped to the canonical UTF-8
string `"k: 1\n"` without error, demonstrating that all five encodings are
not just declared but functionally supported. The ordering decision in
`detect_encoding` (UTF-32 before UTF-16) handles the BOM-prefix overlap
correctly. Spec satisfied.

---

### REQ-§5.2-3: Encoding must not affect parse result (presentation invariance)

Spec requirement: "The character encoding is a presentation detail and must
not be used to convey content information." (`§5.2`, lines 1518-1519).

Test method: Encoded the same YAML document `"k: 1\n"` in all five
supported encodings (UTF-8, UTF-16-LE/BE, UTF-32-LE/BE) — each with its
BOM. Decoded each via `decode`, then ran `parse_events` over each decoded
string. Compared the resulting scalar sequences.

Test input: As listed in REQ-§5.2-2 above.

Observed output (scalar sequences from `parse_events` for each decoded
encoding form):

- UTF-8 decoded: `["k", "1"]`.
- UTF-16-LE decoded: `["k", "1"]`.
- UTF-16-BE decoded: `["k", "1"]`.
- UTF-32-LE decoded: `["k", "1"]`.
- UTF-32-BE decoded: `["k", "1"]`.
- Match-flags: `16LE=true 16BE=true 32LE=true 32BE=true` against the UTF-8
  baseline.

Spec expectation: All five encodings produce identical event sequences
because the encoding is purely presentational.

Verdict: Strict-conformant

Evidence:

- `decode` returns a single `String` regardless of input encoding
  (`encoding.rs:88-96`); downstream parsing operates on UTF-8 only.
- `parse_events` (`lib.rs:57-59`) takes `&str`, so once decoded, the input
  type erases the original encoding — there is no syntactic path by which
  the parser could re-introduce encoding-specific behaviour.
- Probe execution: REQ-§5.2-3 section of stdout reports the four
  `match=true` flags above.

Reasoning: The spec sentence "The character encoding […] must not be used
to convey content information" requires that content semantics be invariant
under encoding choice. The architectural choice to decode to UTF-8 before
the parser sees the input enforces this property at the type level: the
`parse_events`/`load` API takes `&str`, so the parser cannot observe which
encoding the bytes came from. The behavioural test confirms the property
holds: the same content yielded the same scalar sequence `["k", "1"]` across
all five forms. Spec satisfied.

---

### REQ-§5.2-4: BOM is accepted at any document prefix; rejected mid-document

Spec requirement: "Byte order marks may appear at the start of any
document, however all documents in the same stream must use the same
character encoding." (`§5.2`, lines 1531-1532). And from Example 5.3
"Invalid Byte Order Mark" (`§5.2`, lines 1587-1601), "A BOM must not appear
inside a document."

Test method: Four sub-scenarios via `parse_events`:

1. BOM immediately after `...` end-of-document marker (between docs).
2. BOM inside a plain scalar (mid-document).
3. BOM after a `---` directives-end marker (which begins a document body).
4. Two consecutive BOMs at a document prefix.

Test input:

- Inter-document: `"key: a\n...\n\u{FEFF}key: b\n"`.
- Mid-document: `"key: val\u{FEFF}ue\n"`.
- After `---`: `"key: a\n...\n---\n\u{FEFF}key: b\n"`.
- Double-BOM at prefix: `"key: a\n...\n\u{FEFF}\u{FEFF}key: b\n"`.

Observed output:

- Inter-document: no errors; scalars `["key", "a", "key", "b"]`.
- Mid-document: error — `"invalid character U+FEFF in plain scalar"`.
- After `---`: parse error.
- Double-BOM at prefix: parse error.

Spec expectation: BOM at a document-prefix position is accepted; BOM
anywhere inside a document body or in a position not covered by
`l-document-prefix` is rejected. Per the parser's own §5.2 doc
comment at `lines.rs:284-291`, only one BOM is stripped per prefix.

Verdict: Strict-conformant

Evidence:

- Inter-document BOM stripping in `signal_document_boundary` at
  `rlsp_yaml_parser/src/lines.rs:282-305`: strips at most one BOM from the
  primed-next line at each document-boundary signal.
- Mid-document BOM rejection in `event_iter/step.rs:64-82`: the
  step-in-document dispatcher checks for a leading U+FEFF on the next line
  and rejects with `"invalid character U+FEFF in document"` (the actual
  error variant emitted at the scalar level surfaces as
  `"invalid character U+FEFF in plain scalar"`, indicating the BOM check
  also fires in the scalar tokenizer path; both paths reject the BOM).
- Double-BOM rejection: the spec-doc-comment at `lines.rs:290-291` is
  explicit — "Only the first BOM is stripped; a second consecutive BOM in
  the same line is left as illegal content." The body-mode dispatcher at
  `step.rs:72-81` then rejects the second BOM as in-document.
- After-`---` rejection: per the spec, `l-document-prefix` (production 202)
  applies before `---` or at stream start; once `---` is consumed, the
  parser is in document body, where U+FEFF is not part of `c-printable`
  and is rejected by the same `step.rs:72-81` body check.
- Probe execution: REQ-§5.2-4 section of stdout reports the four expected
  outcomes (inter-doc accepted, the other three errors).

Reasoning: The spec sentence "Byte order marks may appear at the start of
any document" defines the legal positions for BOM as document-prefix only;
"however all documents in the same stream must use the same character
encoding" further restricts the valid set to encoding-consistent prefixes.
Combined with Example 5.3 ("A BOM must not appear inside a document"), this
yields a precise rule: BOM is legal only at a document-prefix position
where one BOM begins the prefix. The parser implements exactly that
discipline:

- Stream-start: handled by `lines.rs:115-117`.
- Inter-document prefix: handled by `signal_document_boundary` at
  `lines.rs:292-305`.
- Mid-document and post-`---` body positions: rejected by the body check
  at `step.rs:64-82`.
- Double-BOM: only one stripped per prefix; second is body content.

The behavioural test confirmed all four cases: legal prefix BOM was
accepted, illegal BOMs (mid-doc, post-`---`, double) all produced parse
errors. Spec satisfied.

---

### REQ-§5.2-5: BOM-less encoding detection from null-byte pattern

Spec requirement: "Otherwise, the stream must begin with an ASCII
character. This allows the encoding to be deduced by the pattern of null
(x00) characters." (`§5.2`, lines 1527-1529). The detection table at lines
1542-1553 specifies the exact byte patterns for every encoding, including
BOM-less rows:

- UTF-32-BE: `x00 x00 x00 any`
- UTF-32-LE: `any x00 x00 x00`
- UTF-16-BE: `x00 any`
- UTF-16-LE: `any x00`
- UTF-8 default: any other pattern

Test method: For each BOM-less form, encoded `"k: 1\n"` without a BOM and
called `detect_encoding` to check the classification, then `decode` to
verify what was actually decoded.

Test input:

- UTF-32-BE no-BOM (24 bytes): `[0x00, 0x00, 0x00, 0x6B, 0x00, 0x00, 0x00, 0x3A, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x31, 0x00, 0x00, 0x00, 0x0A]`.
- UTF-32-LE no-BOM: `[0x6B, 0x00, 0x00, 0x00, 0x3A, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x31, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00]`.
- UTF-16-BE no-BOM: `[0x00, 0x6B, 0x00, 0x3A, 0x00, 0x20, 0x00, 0x31, 0x00, 0x0A]`.
- UTF-16-LE no-BOM: `[0x6B, 0x00, 0x3A, 0x00, 0x20, 0x00, 0x31, 0x00, 0x0A, 0x00]`.
- UTF-8 default: `[0x61, 0x3A, 0x20, 0x62, 0x0A]` (`"a: b\n"`).

Observed output:

- UTF-32-BE no-BOM: `detect=Utf8`, decode succeeds and returns
  `"\0\0\0k\0\0\0:\0\0\0 \0\0\01\0\0\0\n"` — i.e., the bytes are decoded
  as if they were a UTF-8 stream filled with NUL characters interleaved
  with the ASCII content. This is **misclassification**: the spec table
  row "ASCII first character | x00 | x00 | x00 | any | UTF-32BE" applies,
  but the parser returned `Utf8`.
- UTF-32-LE no-BOM: `detect=Utf16Le`, decode returns
  `"k\0:\0 \01\0\n\0"` — i.e., misclassified as UTF-16-LE because the
  parser's heuristic at `encoding.rs:66-68` matches `[a, 0x00, ..]`
  before checking the four-byte UTF-32-LE pattern. Spec table row "ASCII
  first character | any | x00 | x00 | x00 | UTF-32LE" applies, but the
  parser returned `Utf16Le`.
- UTF-16-BE no-BOM: `detect=Utf16Be`, decode returns `"k: 1\n"` — correct.
- UTF-16-LE no-BOM: `detect=Utf16Le`, decode returns `"k: 1\n"` — correct.
- UTF-8 default ASCII: `detect=Utf8` — correct.

Spec expectation: The detection table is a normative specification of the
five-row pattern match. Each row must be matched if the bytes match its
pattern; a stream with three leading NULs followed by an ASCII byte must
be identified as UTF-32-BE (not UTF-8), and a stream with `any 00 00 00`
must be identified as UTF-32-LE (not UTF-16-LE).

Verdict: Non-conformant

Evidence:

- `detect_encoding` at `rlsp_yaml_parser/src/encoding.rs:55-72` does not
  contain match arms for the BOM-less UTF-32 detection rows. The relevant
  arms are:
  - Lines 58-59: UTF-32 BOMs only (`[0x00, 0x00, 0xFE, 0xFF, ..]` /
    `[0xFF, 0xFE, 0x00, 0x00, ..]`).
  - Lines 61-62: UTF-16 BOMs.
  - Lines 66-69: UTF-16 null-byte heuristic only — `[a, 0x00, b, 0x00, ..]`,
    `[0x00, a, 0x00, b, ..]`, `[a, 0x00, ..]`, `[0x00, a, ..]`. There is no
    `[0x00, 0x00, 0x00, a, ..]` UTF-32-BE arm and no `[a, 0x00, 0x00, 0x00, ..]`
    UTF-32-LE arm.
  - Line 70: catch-all returns `Utf8`.
- Consequence for UTF-32-BE no-BOM (`[0x00, 0x00, 0x00, 0x6B, ...]`): no arm
  matches, falls through to `Utf8`. The bytes are then `from_utf8`-decoded;
  NUL is valid UTF-8, so decode returns a string with NULs.
- Consequence for UTF-32-LE no-BOM (`[0x6B, 0x00, 0x00, 0x00, ...]`): the
  fourth-from-last arm `[a, 0x00, ..]` matches before any UTF-32 arm could,
  returning `Utf16Le`. Decode then runs UTF-16-LE pair-decoding and
  produces a half-content half-NUL string.
- Probe execution: REQ-§5.2-5 section of stdout shows the misclassifications
  verbatim:
  - `UTF-32-BE no-BOM: detect=Utf8`
  - `UTF-32-LE no-BOM: detect=Utf16Le`

Reasoning: The spec's detection table is a numbered, ordered set of pattern
rules (rows checked top to bottom). Each row is a normative requirement: a
stream matching row N must be classified as the encoding listed in row N.
The parser's `detect_encoding` correctly implements the BOM rows (table
rows 1, 3, 5, 7, 9) but only partially implements the BOM-less rows (table
rows 2, 4, 6, 8): UTF-16-LE and UTF-16-BE BOM-less rows are present;
UTF-32-LE and UTF-32-BE BOM-less rows are absent. As a result:

- A valid YAML document encoded in UTF-32-BE without a BOM is decoded as
  UTF-8 with embedded NUL bytes. The downstream parser will reject those
  NULs (NUL is excluded from `c-printable`), so the parse fails — but the
  failure mode is "parse error in body" rather than "decode this as
  UTF-32-BE". The user's content is unrecoverable through this entry
  point.
- A valid YAML document encoded in UTF-32-LE without a BOM is decoded as
  UTF-16-LE, producing a corrupt string in which every fourth byte (the
  high-order NUL) is treated as a character pair separator. The parser
  may then reject the result for unrelated reasons, again destroying the
  user's content.

Note on real-world relevance: BOM-less UTF-32 input is rare in practice
(most UTF-32 producers emit a BOM, and the §5.2 normative rule "all
documents in the same stream must use the same character encoding" is
commonly enforced by the producer pinning a BOM). However, the spec
explicitly lists the BOM-less UTF-32 rows in the detection table, so the
absence of those arms is a normative gap regardless of frequency.
A user submitting a BOM-less UTF-32 stream — perhaps generated by a
tool that sets the encoding via filename or HTTP header instead of a
BOM — receives a confusing decode error or a corrupted parse rather
than the spec-promised classification. Spec is not satisfied.

To reach `Strict-conformant` for this REQ, two arms must be added before
the existing UTF-16 heuristic arms in `encoding.rs:55-72`:

- `[0x00, 0x00, 0x00, a, ..] if *a != 0 => Encoding::Utf32Be`
- `[a, 0x00, 0x00, 0x00, ..] if *a != 0 => Encoding::Utf32Le`

These must precede the UTF-16 arms because `[a, 0x00, ..]` would otherwise
match a UTF-32-LE prefix first.

---

### REQ-§5.2-6: Truncated or invalid encoded streams must be rejected

Spec requirement: This is not directly normative in `§5.2` prose — the
section does not say "implementations must reject malformed UTF-16/UTF-32"
in those words. However, it follows from the requirement that the parser
"must support UTF-8 and UTF-16" (line 1521) — supporting an encoding
implies rejecting byte sequences that are not valid in that encoding,
because accepting them would silently mis-decode content. The taxonomy of
errors the parser surfaces (`EncodingError::TruncatedUtf16`,
`TruncatedUtf32`, `InvalidCodepoint`, `InvalidBytes` at
`encoding.rs:18-43`) implements this defensive contract.

Test method: Submitted four malformed inputs to `decode` and inspected the
returned `Result`.

Test input:

- UTF-16-LE BOM + 1 byte (odd length): `[0xFF, 0xFE, 0x68]`.
- UTF-32-BE BOM + 1 byte (length 5, not multiple of 4):
  `[0x00, 0x00, 0xFE, 0xFF, 0x00]`.
- UTF-16-BE BOM + lone high surrogate U+D800:
  `[0xFE, 0xFF, 0xD8, 0x00]`.
- UTF-32-BE BOM + codepoint U+110000 (above Unicode max):
  `[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x11, 0x00, 0x00]`.

Observed output:

- Truncated UTF-16: `Err(TruncatedUtf16)`.
- Truncated UTF-32: `Err(TruncatedUtf32)`.
- Unpaired surrogate: `Err(InvalidCodepoint(55296))` (= `0xD800`).
- Out-of-range codepoint: `Err(InvalidCodepoint(1114112))` (= `0x110000`).

Spec expectation: Malformed encoded inputs must surface an error rather
than be silently treated as valid content.

Verdict: Strict-conformant

Evidence:

- `decode_utf16` rejects odd byte length at `encoding.rs:111-113`.
- `decode_utf16` propagates `char::decode_utf16` surrogate errors at
  `encoding.rs:131-142` as `InvalidCodepoint`.
- `decode_utf32` rejects non-multiple-of-4 length at `encoding.rs:146-148`.
- `decode_utf32` rejects out-of-range codepoints at `encoding.rs:163` via
  `char::from_u32`.
- `decode_utf8` rejects invalid UTF-8 at `encoding.rs:104-105` via
  `core::str::from_utf8`.
- Probe execution: REQ-§5.2-6 section of stdout reports the four expected
  error variants.

Reasoning: The spec's "must support UTF-8 and UTF-16" is interpreted as
"must decode valid encoded streams correctly and reject invalid ones." The
parser implements all four required rejection paths: truncation of UTF-16
streams (odd byte count), truncation of UTF-32 streams (non-multiple-of-4),
unpaired UTF-16 surrogates, and UTF-32 codepoints outside `[0, 0x10FFFF]`
excluding surrogate range. The behavioural test confirmed every case
produces a typed `EncodingError` that callers can distinguish from
"content-level" parse errors. Spec satisfied.

---

### REQ-§5.2-7: BOM is allowed inside quoted scalars (JSON compatibility)

Spec requirement: "To allow for JSON compatibility, byte order marks are
also allowed inside quoted scalars. For readability, such content byte
order marks should be escaped on output." (`§5.2`, lines 1534-1536).

Test method: Submitted YAML inputs containing a literal U+FEFF inside a
double-quoted scalar value and inside a single-quoted scalar value, parsed
via `parse_events`, and inspected whether the BOM is preserved as scalar
content.

Test input:

- Double-quoted: `"key: \"a\u{FEFF}b\"\n"` (BOM between `a` and `b`).
- Single-quoted: `"key: 'a\u{FEFF}b'\n"`.

Observed output:

- Double-quoted: no errors; scalars `["key", "a\u{feff}b"]` (BOM
  preserved as scalar content).
- Single-quoted: no errors; scalars `["key", "a\u{feff}b"]` (BOM
  preserved as scalar content).

Spec expectation: BOM inside a quoted scalar must be accepted and preserved
as content (the "should be escaped on output" clause is a non-normative
recommendation for emitters, not a requirement on parsers).

Verdict: Strict-conformant

Evidence:

- The body-mode BOM check at `event_iter/step.rs:64-82` runs only on
  *unquoted* line starts. Once the tokenizer enters a quoted scalar, the
  body check no longer applies; the quoted-scalar tokenizer accepts U+FEFF
  as content.
- `tests/encoding.rs:184-188` documents the inverse case (BOM in plain
  scalar is rejected) — by exclusion, BOM in *quoted* scalars is the path
  that allows it.
- Probe execution: REQ-§5.2-7 section of stdout reports both quoted-scalar
  forms produce `["key", "a\u{feff}b"]` with no error.

Reasoning: The spec's "byte order marks are also allowed inside quoted
scalars" is a normative permission tied to JSON compatibility (JSON allows
U+FEFF in string content). The parser correctly distinguishes the two
positions: BOM at a non-prefix unquoted position is rejected (per
REQ-§5.2-4), but BOM inside a quoted-scalar tokenization path is accepted
and preserved as a content character. The behavioural test confirms the
exact byte (U+FEFF) survives the parse round-trip in both double-quoted
and single-quoted forms. The "should be escaped on output" clause is
emitter guidance and does not bear on parser conformance. Spec satisfied.

---

### REQ-§5.2-8: Loader (`load`) layer must round-trip multi-document streams with inter-document BOM

Spec requirement: Combination of `§5.2` lines 1531-1532 ("Byte order marks
may appear at the start of any document") and the implicit normative
expectation that the AST-loader API (`load`), which the project documents
as a co-equal entry point alongside `parse_events`
(see `/workspace/CLAUDE.md` "Crate Boundaries" / "API/Function" table),
must honour the same BOM placement rules.

Test method: Two-stage probe of the `load` API:

1. Multi-document stream with `...`/BOM-prefixed second document — verify
   the loader returns a two-document `Vec<Document>`.
2. UTF-16-LE-encoded source decoded via `decode`, then handed to `load` —
   verify a single document is built (`"k: 1\n"`).

Test input:

- Stream 1 (multi-doc): `"key: a\n...\n\u{FEFF}key: b\n"`.
- Stream 2 (after decode of UTF-16-LE bytes): the decoded `"k: 1\n"`.

Observed output:

- Stream 1: `Ok(docs)` with `docs.len() == 2`.
- Stream 2: `Ok(docs)` with `docs.len() == 1`.

Spec expectation: The `load` API must accept legal BOM placements without
error and produce a `Vec<Document>` whose length matches the number of
documents in the stream.

Verdict: Strict-conformant

Evidence:

- `load` is exposed at `lib.rs:27` as a public entry point.
- The loader consumes the same event stream produced by `parse_events`
  (per project convention `/workspace/CLAUDE.md` "One parser, one AST"),
  so the BOM-handling logic in `lines.rs:282-305` and `step.rs:64-82`
  applies transitively.
- `tests/encoding.rs:277-302` verifies the same multi-document load
  property; the probe re-confirmed it and additionally checked the
  decode-then-load pipeline for UTF-16-LE input.
- Probe execution: REQ-§5.2-8 section of stdout reports
  `load ok: 2 docs` and `load(UTF-16-LE-decoded) ok: 1 docs`.

Reasoning: The `load` API is a thin AST builder over the event stream. Any
BOM-related conformance gap in `load` would have to originate either in
the event stream (covered by REQ-§5.2-1, -4, -7) or in the AST construction
itself. The probe confirmed: legal inter-document BOM is handled (two-doc
parse), and a non-trivial decode-then-load pipeline (UTF-16-LE bytes →
`decode` → `&str` → `load`) produces a single document as expected. No
deviation observed at the loader layer. Spec satisfied at this layer; any
gap would propagate from `parse_events`, which itself satisfies the
relevant requirements (modulo REQ-§5.2-5 for BOM-less UTF-32 streams,
which is a `decode`-layer issue and never reaches `load`).

---

## Verdict tally

- REQ-§5.2-1: Strict-conformant
- REQ-§5.2-2: Strict-conformant
- REQ-§5.2-3: Strict-conformant
- REQ-§5.2-4: Strict-conformant
- REQ-§5.2-5: Non-conformant (BOM-less UTF-32 detection rows missing)
- REQ-§5.2-6: Strict-conformant
- REQ-§5.2-7: Strict-conformant
- REQ-§5.2-8: Strict-conformant

Strict-conformant: 7. Non-conformant: 1.
