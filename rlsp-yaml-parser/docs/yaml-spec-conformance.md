# YAML 1.2.2 Conformance Audit — rlsp-yaml-parser

## Methodology

### Scope

This document is a **parser-only, documentation-only** audit of `rlsp-yaml-parser`
against the YAML 1.2.2 specification. Every numbered production in §3–§10 of the spec
is classified using the strict entry format defined below.

**Out of scope:**

- `rlsp-yaml` (language server + formatter) and `rlsp-fmt` (generic pretty-printer).
- Remediation of any finding. Findings are recorded here; remediation is a follow-up
  decision in separate plans.
- Downstream ramifications of hypothetical parser fixes. Those belong in each
  remediation plan's Context, not in this audit.
- Expanding beyond YAML 1.2.2. YAML 1.1 compatibility diagnostics are out of scope.

### Reference Specification

- **URL:** <https://yaml.org/spec/1.2.2/>
- **Cached copy:** `.ai/references/yaml-1.2.2-spec.md`
  (source: `https://raw.githubusercontent.com/yaml/yaml-spec/main/spec/1.2.2/spec.md`,
  fetched 2026-04-21, 211 productions [1]–[211] across §5–§9; §10 uses tables only)

All spec quotes in this document are verbatim from the cached copy, with the
following normalizations:

- Markdown cross-reference brackets (`[…]`) and emphasis underscores (`_…_`)
  are stripped from quoted text so the rendered document reads cleanly.
- When a quote omits intervening spec text (e.g. skips a sentence between two
  quoted sentences), the omission is marked with an explicit ellipsis marker
  `[…]`. This applies whether the skipped text is mid-passage or at the end of
  the quoted passage.

All other characters are reproduced character-for-character from the cache.

### Strict Entry Format

Every production, regardless of classification, uses this format:

```
### [NNN] production-name

BNF: <exact BNF from the spec>

- Classification: Conformant | Lenient | Strict | Strict (security-hardened) | Not Implemented | Not Applicable (descriptive) | Not Applicable (meta-notation)
- Spec (§X.Y): "<verbatim quote of the normative text>"
- Implementation: <crate>/<path>:<line-range>
- Test coverage: <yaml-test-suite case ID(s)> | <project test path> | no direct test
- Discrepancy: <one-sentence gap — Lenient/Strict only; omit for other classes>
- Rationale: <one-sentence reference to the source comment, feature-log entry, or design doc that marks the divergence as deliberate — required for Strict (security-hardened); optional for other classifications>
```

For `Not Applicable` entries: the Spec quote is still required (it establishes that the
entry is descriptive / meta-notation); the Implementation and Test coverage fields carry
the explicit text `(no implementation obligation)`.

### Classification Decision Rules

| Spec says | Code does | Classification |
|-----------|-----------|----------------|
| requires X | does X | **Conformant** |
| requires X | does X **and also** Y (Y not permitted) | **Lenient** |
| permits X | rejects X | **Strict** |
| permits X | rejects X as part of a documented security policy | **Strict (security-hardened)** |
| requires X | does not implement X | **Not Implemented** |
| entry has no normative obligation on the implementation (purely descriptive) | — | **Not Applicable (descriptive)** |
| entry is meta-notation for the grammar itself | — | **Not Applicable (meta-notation)** |

The classification is the output of applying these rules to the spec quote and the
implementation fact recorded in the entry. A classification that does not follow from
the recorded evidence is a reviewer-rejectable defect.

`Strict (security-hardened)` is a sub-class of `Strict`: the parser still rejects
spec-permitted input, but the rejection is deliberate and documented rather than
incidental or undecided. A `Rationale` citation is mandatory for this sub-class —
without it the security motivation is unverifiable. Plain `Strict` (no marker) means
the rejection is a bug or the deliberateness has not yet been assessed.

### Test-Coverage Conventions

- **yaml-test-suite case ID** — four-character identifier (e.g., `6CA3`) when a test
  case exercises the production. Multiple IDs allowed.
- **project test path** — `rlsp-yaml-parser/tests/<file>.rs` plus test function name,
  if the production is exercised by a project test.
- **no direct test** — valid only when neither of the above applies. An explicit
  "no direct test" is itself a data point (coverage gap); silent omission is not
  permitted.

---

## §3

§3 (Processes and Models) is entirely prose — it defines the dump/load pipeline,
information models (representation graph, serialization tree, presentation stream),
and loading failure points.  There are no numbered BNF productions in §3.

### [§3] Not Applicable (descriptive)

BNF: (none — §3 contains no numbered BNF productions)

- Classification: Not Applicable (descriptive)
- Spec (§3): "YAML is both a text format and a method for presenting any native data structure in this format. Therefore, this specification defines two concepts: a class of data objects called YAML representations and a syntax for presenting YAML representations as a series of characters, called a YAML stream."
- Implementation: (no implementation obligation)
- Test coverage: (no implementation obligation)

## §4

§4 (Syntax Conventions) defines the BNF meta-notation used in subsequent chapters:
production syntax, parameter conventions, and naming prefixes.  There are no numbered
BNF productions in §4.

### [§4] Not Applicable (meta-notation)

BNF: (none — §4 contains no numbered BNF productions)

- Classification: Not Applicable (meta-notation)
- Spec (§4): "The following chapters formally define the syntax of YAML character streams, using parameterized BNF productions. Each BNF production is both named and numbered for easy reference."
- Implementation: (no implementation obligation)
- Test coverage: (no implementation obligation)

## §5

### [1] c-printable

BNF: `c-printable ::= x09 | x0A | x0D | [x20-x7E] | x85 | [xA0-xD7FF] | [xE000-xFFFD] | [x010000-x10FFFF]`

- Classification: Conformant
- Spec (§5.1): "To ensure readability, YAML streams use only the printable subset of the Unicode character set. The allowed character range explicitly excludes the C0 control block x00-x1F (except for TAB x09, LF x0A and CR x0D which are allowed), DEL x7F, the C1 control block x80-x9F (except for NEL x85 which is allowed), the surrogate block xD800-xDFFF, xFFFE and xFFFF. On input, a YAML processor must accept all characters in this printable subset."
- Implementation: `rlsp-yaml-parser/src/chars.rs:14–26` (`is_c_printable`)
- Test coverage: `rlsp-yaml-parser/src/chars.rs:241–258` (unit tests `c_printable_accepts`, `c_printable_rejects`)

### [2] nb-json

BNF: `nb-json ::= x09 | [x20-x10FFFF]`

- Classification: Conformant
- Spec (§5.1): "To ensure JSON compatibility, YAML processors must allow all non-C0 characters inside quoted scalars. To ensure readability, non-printable characters should be escaped on output, even inside such scalars."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (double-quoted scanner accepts tab and all non-C0 characters inside quoted scalars; `is_c_printable` gating applies to escape-decoded characters only, not to literal stream characters)
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (spec example 2.17 exercising tab and Unicode inside double-quoted scalars)

### [3] c-byte-order-mark

BNF: `c-byte-order-mark ::= xFEFF`

- Classification: Conformant
- Spec (§5.2): "If a character stream begins with a byte order mark, the character encoding will be taken to be as indicated by the byte order mark. Otherwise, the stream must begin with an ASCII character. […] Byte order marks may appear at the start of any document, however all documents in the same stream must use the same character encoding. To allow for JSON compatibility, byte order marks are also allowed inside quoted scalars."
- Implementation: `rlsp-yaml-parser/src/encoding.rs:88–96` (`decode` handles BOM at byte-stream level, before parsing); `rlsp-yaml-parser/src/lines.rs:292–303` (`signal_document_boundary()` strips a leading BOM from the already-primed next line at document-prefix positions); `rlsp-yaml-parser/src/lexer.rs:141` (calls `signal_document_boundary()` from `skip_blank_lines_between_docs()`); `rlsp-yaml-parser/src/event_iter/step.rs:64–79` (BOM inside document body after `---` or mid-stream is rejected as invalid)
- Test coverage: `rlsp-yaml-parser/tests/encoding.rs` (`decode_bom_stripping` cases `utf8_bom`, `utf16_le_bom`; `parse_events_accepts_bom_at_stream_start`; `parse_events_accepts_bom_immediately_after_document_end_marker`; `parse_events_accepts_bom_after_doc_end_then_blank_lines`; `parse_events_accepts_bom_after_doc_end_then_comment`; `parse_events_accepts_multiple_docs_each_with_bom`; `parse_events_rejects_bom_mid_scalar_regression`; `parse_events_bom_after_directives_end_marker_is_error`; `parse_events_rejects_double_bom_at_document_prefix`); `rlsp-yaml-parser/src/lines.rs` (`bom_stripped_after_document_boundary_signal`; `signal_document_boundary_strips_bom_from_primed_next_line`; `bom_stripped_line_offset_correct_after_boundary_signal`)

### [4] c-sequence-entry

BNF: `c-sequence-entry ::= '-'`

- Classification: Conformant
- Spec (§5.3): "\"-\" (x2D, hyphen) denotes a block sequence entry."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/sequence.rs:37` (`peek_sequence_entry` — `starts_with('-')` check)
- Test coverage: `tests/yaml-test-suite/src/229Q.yaml` and many other suite cases exercising block sequences

### [5] c-mapping-key

BNF: `c-mapping-key ::= '?'`

- Classification: Conformant
- Spec (§5.3): "\"?\" (x3F, question mark) denotes a mapping key."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/mapping.rs:52` (`peek_mapping_entry` — `strip_prefix('?')` for explicit key indicator)
- Test coverage: `tests/yaml-test-suite/src/229Q.yaml` (explicit mapping keys exercised in suite)

### [6] c-mapping-value

BNF: `c-mapping-value ::= ':'`

- Classification: Conformant
- Spec (§5.3): "\":\" (x3A, colon) denotes a mapping value."
- Implementation: `rlsp-yaml-parser/src/event_iter/line_mapping.rs:68–187` (`find_value_indicator_offset` locates the `:` separator; `:` recognised at line 177)
- Test coverage: `tests/yaml-test-suite/src/229Q.yaml` and the majority of the yaml-test-suite (mappings are ubiquitous)

### [7] c-collect-entry

BNF: `c-collect-entry ::= ','`

- Classification: Conformant
- Spec (§5.3): "\",\" (x2C, comma) ends a flow collection entry."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:652` (`','` branch in the flow scanner inner loop)
- Test coverage: `tests/yaml-test-suite/src/4ABK.yaml` and other flow-collection suite cases

### [8] c-sequence-start

BNF: `c-sequence-start ::= '['`

- Classification: Conformant
- Spec (§5.3): "\"[\" (x5B, left bracket) starts a flow sequence."
- Implementation: `rlsp-yaml-parser/src/chars.rs:58–60` (`is_c_flow_indicator`); `rlsp-yaml-parser/src/event_iter/flow.rs:388–459` (`'['` branch pushes `FlowFrame::Sequence` and emits `SequenceStart`)
- Test coverage: `tests/yaml-test-suite/src/4ABK.yaml` (flow sequences exercised)

### [9] c-sequence-end

BNF: `c-sequence-end ::= ']'`

- Classification: Conformant
- Spec (§5.3): "\"]\" (x5D, right bracket) ends a flow sequence."
- Implementation: `rlsp-yaml-parser/src/chars.rs:58–60` (`is_c_flow_indicator`); `rlsp-yaml-parser/src/event_iter/flow.rs:465–559` (`']'` branch pops `FlowFrame::Sequence` and emits `SequenceEnd`)
- Test coverage: `tests/yaml-test-suite/src/4ABK.yaml` (flow sequences exercised)

### [10] c-mapping-start

BNF: `c-mapping-start ::= '{'`

- Classification: Conformant
- Spec (§5.3): "\"{\" (x7B, left brace) starts a flow mapping."
- Implementation: `rlsp-yaml-parser/src/chars.rs:58–60` (`is_c_flow_indicator`); `rlsp-yaml-parser/src/event_iter/flow.rs:388–459` (`'{'` branch pushes `FlowFrame::Mapping` and emits `MappingStart`)
- Test coverage: `tests/yaml-test-suite/src/4ABK.yaml` (flow mappings exercised)

### [11] c-mapping-end

BNF: `c-mapping-end ::= '}'`

- Classification: Conformant
- Spec (§5.3): "\"}\" (x7D, right brace) ends a flow mapping."
- Implementation: `rlsp-yaml-parser/src/chars.rs:58–60` (`is_c_flow_indicator`); `rlsp-yaml-parser/src/event_iter/flow.rs:465–559` (`'}'` branch pops `FlowFrame::Mapping` and emits `MappingEnd`)
- Test coverage: `tests/yaml-test-suite/src/4ABK.yaml` (flow mappings exercised)

### [12] c-comment

BNF: `c-comment ::= '#'`

- Classification: Conformant
- Spec (§5.3): "\"#\" (x23, octothorpe, hash, sharp, pound, number sign) denotes a comment."
- Implementation: `rlsp-yaml-parser/src/lexer/comment.rs` — `'#'` triggers comment lexing
- Test coverage: `rlsp-yaml-parser/tests/smoke/comments.rs`

### [13] c-anchor

BNF: `c-anchor ::= '&'`

- Classification: Conformant
- Spec (§5.3): "\"&\" (x26, ampersand) denotes a node's anchor property."
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:22–45` (`scan_anchor_name` is invoked after `'&'` indicator)
- Test coverage: `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`

### [14] c-alias

BNF: `c-alias ::= '*'`

- Classification: Conformant
- Spec (§5.3): "\"*\" (x2A, asterisk) denotes an alias node."
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:22–45` (`scan_anchor_name` is also used after `'*'` for alias scanning)
- Test coverage: `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`

### [15] c-tag

BNF: `c-tag ::= '!'`

- Classification: Conformant
- Spec (§5.3): "The \"!\" (x21, exclamation) is used for specifying node tags. It is used to denote tag handles used in tag directives and tag properties; to denote local tags; and as the non-specific tag for non-plain scalars."
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:85–350` (`scan_tag` handles all `!`-introduced tag forms)
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`

### [16] c-literal

BNF: `c-literal ::= '|'`

- Classification: Conformant
- Spec (§5.3): "\"|\" (7C, vertical bar) denotes a literal block scalar."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:41–274` (`try_consume_literal_block_scalar` — `starts_with('|')` check at line 48)
- Test coverage: `tests/yaml-test-suite/src/A2M4.yaml` and other block-scalar suite cases

### [17] c-folded

BNF: `c-folded ::= '>'`

- Classification: Conformant
- Spec (§5.3): "\">\" (x3E, greater than) denotes a folded block scalar."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:288–351` (`try_consume_folded_block_scalar` — `starts_with('>')` check at line 294)
- Test coverage: `rlsp-yaml-parser/tests/smoke/folded_scalars.rs`

### [18] c-single-quote

BNF: `c-single-quote ::= "'"`

- Classification: Conformant
- Spec (§5.3): "\"'\" (x27, apostrophe, single quote) surrounds a single-quoted flow scalar."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:27–163` (`try_consume_single_quoted`)
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [19] c-double-quote

BNF: `c-double-quote ::= '"'`

- Classification: Conformant
- Spec (§5.3): "\"\"\" (x22, double quote) surrounds a double-quoted flow scalar."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (`try_consume_double_quoted`)
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [20] c-directive

BNF: `c-directive ::= '%'`

- Classification: Conformant
- Spec (§5.3): "\"%\" (x25, percent) denotes a directive line."
- Implementation: `rlsp-yaml-parser/src/lexer.rs:142–146` (`is_directive_line` — `starts_with('%')` check)
- Test coverage: `rlsp-yaml-parser/tests/smoke/directives.rs`

### [21] c-reserved

BNF: `c-reserved ::= '@' | '\`'`

- Classification: Conformant
- Spec (§5.3): "The \"@\" (x40, at) and \"`\" (x60, grave accent) are reserved for future use."
- Implementation: `rlsp-yaml-parser/src/chars.rs:33–55` (`is_c_indicator` includes `'@'` and `` '`' ``); `rlsp-yaml-parser/src/lexer/plain.rs:299` — `is_c_indicator` check causes reserved chars to be rejected as plain scalar starts
- Test coverage: `tests/yaml-test-suite/src/R4YG.yaml` (reserved indicator error case)

### [22] c-indicator

BNF: `c-indicator ::= c-sequence-entry | c-mapping-key | c-mapping-value | c-collect-entry | c-sequence-start | c-sequence-end | c-mapping-start | c-mapping-end | c-comment | c-anchor | c-alias | c-tag | c-literal | c-folded | c-single-quote | c-double-quote | c-directive | c-reserved`

- Classification: Conformant
- Spec (§5.3): "Indicators are characters that have special semantics."
- Implementation: `rlsp-yaml-parser/src/chars.rs:33–55` (`is_c_indicator`)
- Test coverage: `rlsp-yaml-parser/src/chars.rs:265–281` (unit tests `c_indicator_accepts_all_21_indicator_chars`, `c_indicator_rejects`)

### [23] c-flow-indicator

BNF: `c-flow-indicator ::= c-collect-entry | c-sequence-start | c-sequence-end | c-mapping-start | c-mapping-end`

- Classification: Conformant
- Spec (§5.3): "The \"[\", \"]\", \"{\", \"}\" and \",\" indicators denote structure in flow collections. They are therefore forbidden in some cases, to avoid ambiguity in several constructs. […]"
- Implementation: `rlsp-yaml-parser/src/chars.rs:58–60` (`is_c_flow_indicator`)
- Test coverage: `rlsp-yaml-parser/src/chars.rs:284–298` (unit tests `c_flow_indicator_accepts_exactly_five_chars`, `c_flow_indicator_rejects_non_flow_indicators`)

### [24] b-line-feed

BNF: `b-line-feed ::= x0A`

- Classification: Conformant
- Spec (§5.4): "YAML recognizes the following ASCII line break characters."
- Implementation: `rlsp-yaml-parser/src/lines.rs:98–101` (`detect_break` matches `'\n'`)
- Test coverage: `rlsp-yaml-parser/src/encoding.rs` (`normalize_line_breaks_cases` — lf-only case)

### [25] b-carriage-return

BNF: `b-carriage-return ::= x0D`

- Classification: Conformant
- Spec (§5.4): "YAML recognizes the following ASCII line break characters."
- Implementation: `rlsp-yaml-parser/src/lines.rs:94–97` (`detect_break` matches `'\r'`)
- Test coverage: `rlsp-yaml-parser/src/encoding.rs` (`normalize_line_breaks_cases` — lone-cr and crlf cases)

### [26] b-char

BNF: `b-char ::= b-line-feed | b-carriage-return`

- Classification: Conformant
- Spec (§5.4): "YAML recognizes the following ASCII line break characters."
- Implementation: `rlsp-yaml-parser/src/lines.rs:130–132` — `find(['\n', '\r'])` locates end of line content, matching exactly `b-char`
- Test coverage: `rlsp-yaml-parser/src/encoding.rs` (`normalize_line_breaks_cases`)

### [27] nb-char

BNF: `nb-char ::= c-printable - b-char - c-byte-order-mark`

- Classification: Conformant
- Spec (§5.4): "All other characters, including the form feed (x0C), are considered to be non-break characters. Note that these include the non-ASCII line breaks: next line (x85), line separator (x2028) and paragraph separator (x2029)."
- Implementation: `rlsp-yaml-parser/src/lines.rs:130–132` — the line splitter treats only `['\n', '\r']` as break characters, leaving x85, x2028, x2029 and all other c-printable non-BOM characters as non-break; no standalone `nb-char` predicate is defined (the invariant is maintained structurally)
- Test coverage: no direct test

### [28] b-break

BNF: `b-break ::= ( b-carriage-return b-line-feed ) | b-carriage-return | b-line-feed`

- Classification: Conformant
- Spec (§5.4): "Line breaks are interpreted differently by different systems and have multiple widely used formats."
- Implementation: `rlsp-yaml-parser/src/lines.rs:91–102` (`detect_break` — CRLF checked first, then bare CR, then LF)
- Test coverage: `rlsp-yaml-parser/src/encoding.rs` (`normalize_line_breaks_cases` covers CRLF, lone CR, LF)

### [29] b-as-line-feed

BNF: `b-as-line-feed ::= b-break`

- Classification: Conformant
- Spec (§5.4): "Line breaks inside scalar content must be normalized by the YAML processor. Each such line break must be parsed into a single line feed character. The original line break format is a presentation detail and must not be used to convey content information."
- Implementation: `rlsp-yaml-parser/src/encoding.rs:179–197` (`normalize_line_breaks` — CRLF and lone CR both become LF before the string is handed to the parser)
- Test coverage: `rlsp-yaml-parser/src/encoding.rs` (`normalize_line_breaks_cases`)

### [30] b-non-content

BNF: `b-non-content ::= b-break`

- Classification: Conformant
- Spec (§5.4): "Outside scalar content, YAML allows any line break to be used to terminate lines."
- Implementation: `rlsp-yaml-parser/src/lines.rs:91–102` — `detect_break` is called after `find(['\n', '\r'])` separates content from terminator; outside scalars the terminator is discarded (non-content)
- Test coverage: no direct test

### [31] s-space

BNF: `s-space ::= x20`

- Classification: Conformant
- Spec (§5.5): "YAML recognizes two white space characters: space and tab."
- Implementation: `rlsp-yaml-parser/src/lines.rs:142` — `ch == ' '` counts leading space characters for indentation; used as literal `' '` or `'\x20'` throughout the codebase
- Test coverage: no direct test (indirectly exercised by all indentation-sensitive yaml-test-suite cases)

### [32] s-tab

BNF: `s-tab ::= x09`

- Classification: Conformant
- Spec (§5.5): "YAML recognizes two white space characters: space and tab."
- Implementation: used as literal `'\t'` throughout `src/lexer/` and `src/event_iter/`; `rlsp-yaml-parser/src/chars.rs:67–76` — `is_ns_char` excludes `'\t'`
- Test coverage: `tests/yaml-test-suite/src/4ZYM.yaml` (tabs inside quoted scalars and block scalars)

### [33] s-white

BNF: `s-white ::= s-space | s-tab`

- Classification: Conformant
- Spec (§5.5): "YAML recognizes two white space characters: space and tab."
- Implementation: used as `[' ', '\t']` or `|' '| '\t'` patterns throughout `src/lexer/quoted.rs`, `src/event_iter/`
- Test coverage: `tests/yaml-test-suite/src/4ZYM.yaml` (spec example 6.4 exercises both spaces and tabs as white space)

### [34] ns-char

BNF: `ns-char ::= nb-char - s-white`

- Classification: Conformant
- Spec (§5.5): "The rest of the (printable) non-break characters are considered to be non-space characters."
- Implementation: `rlsp-yaml-parser/src/chars.rs:67–76` (`is_ns_char`)
- Test coverage: `rlsp-yaml-parser/src/chars.rs:304–319` (unit tests `ns_char_accepts`, `ns_char_rejects`)

### [35] ns-dec-digit

BNF: `ns-dec-digit ::= [x30-x39]`

- Classification: Conformant
- Spec (§5.6): "A decimal digit for numbers:"
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:568–592` — block scalar header matches indentation indicator digits via `'0'` (rejected as invalid at line 568) and `ch @ '1'..='9'` (accepted at line 579); range pattern `'1'..='9'` is Rust's equivalent to `[x31-x39]`
- Test coverage: `rlsp-yaml-parser/tests/smoke/block_scalars.rs` (block scalars with explicit indentation indicators)

### [36] ns-hex-digit

BNF: `ns-hex-digit ::= ns-dec-digit | [x41-x46] | [x61-x66]`

- Classification: Conformant
- Spec (§5.6): "A hexadecimal digit for escape sequences:"
- Implementation: `rlsp-yaml-parser/src/chars.rs:210` (`decode_hex_escape` — `.is_ascii_hexdigit()`); `rlsp-yaml-parser/src/event_iter/properties.rs:119–123` (percent-encoded URI validation via `.is_ascii_hexdigit()`)
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (`\xHH` and `\uHHHH` escapes exercised); `rlsp-yaml-parser/src/chars.rs:391–410` (unit tests for `decode_escape`)

### [37] ns-ascii-letter

BNF: `ns-ascii-letter ::= [x41-x5A] | [x61-x7A]`

- Classification: Conformant
- Spec (§5.6): "ASCII letter (alphabetic) characters:"
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:281–295` (`is_valid_tag_handle` uses `.is_ascii_alphanumeric()` which covers `ns-ascii-letter` as a subset)
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`

### [38] ns-word-char

BNF: `ns-word-char ::= ns-dec-digit | ns-ascii-letter | '-'`

- Classification: Conformant
- Spec (§5.6): "Word (alphanumeric) characters for identifiers:"
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:289` — tag handle validation uses `.is_ascii_alphanumeric() || c == '-'`; `rlsp-yaml-parser/src/chars.rs:89–113` (`is_ns_uri_char_single` includes alphanumeric and `-`)
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`

### [39] ns-uri-char

BNF: `ns-uri-char ::= ( '%' ns-hex-digit{2} ) | ns-word-char | '#' | ';' | '/' | '?' | ':' | '@' | '&' | '=' | '+' | '$' | ',' | '_' | '.' | '!' | '~' | '*' | "'" | '(' | ')' | '[' | ']'`

- Classification: Conformant
- Spec (§5.6): "URI characters for tags, as defined in the URI specification. By convention, any URI characters other than the allowed printable ASCII characters are first encoded in UTF-8 and then each byte is escaped using the \"%\" character. The YAML processor must not expand such escaped characters. Tag characters must be preserved and compared exactly as presented in the YAML stream, without any processing."
- Implementation: `rlsp-yaml-parser/src/chars.rs:88–113` (`is_ns_uri_char_single` for single-char form); percent-encoded form handled in `rlsp-yaml-parser/src/event_iter/properties.rs:100–130`
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`

### [40] ns-tag-char

BNF: `ns-tag-char ::= ns-uri-char - c-tag - c-flow-indicator`

- Classification: Conformant
- Spec (§5.6): "The \"!\" character is used to indicate the end of a named tag handle; hence its use in tag shorthands is restricted. In addition, such shorthands must not contain the \"[\", \"]\", \"{\", \"}\" and \",\" characters. These characters would cause ambiguity with flow collection structures."
- Implementation: `rlsp-yaml-parser/src/chars.rs:121–143` (`is_ns_tag_char_single`)
- Test coverage: `rlsp-yaml-parser/src/chars.rs:352–376` (unit tests `ns_tag_char_rejects_flow_indicators`, `ns_tag_char_accepts`, `ns_uri_char_accepts_exclamation_but_tag_char_does_not`)

### [41] c-escape

BNF: `c-escape ::= '\'`

- Classification: Conformant
- Spec (§5.7): "All non-printable characters must be escaped. YAML escape sequences use the \"\\\" notation common to most modern computer languages. Each escape sequence must be parsed into the appropriate Unicode character. The original escape sequence is a presentation detail and must not be used to convey content information."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:575–620` (`decode_and_push_escape` dispatches on `'\'` in double-quoted scanner)
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml`; `tests/yaml-test-suite/src/55WF.yaml` (invalid escape rejected)

### [42] ns-esc-null

BNF: `ns-esc-null ::= '0'`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII null (x00) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:177` (`decode_escape` — `'0' => Some(('\x00', 1))`)
- Test coverage: `rlsp-yaml-parser/src/chars.rs:383` (unit test `decode_escape_success` case `null_escape`)

### [43] ns-esc-bell

BNF: `ns-esc-bell ::= 'a'`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII bell (x07) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:178` (`decode_escape` — `'a' => Some(('\x07', 1))`)
- Test coverage: no direct test

### [44] ns-esc-backspace

BNF: `ns-esc-backspace ::= 'b'`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII backspace (x08) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:179` (`decode_escape` — `'b' => Some(('\x08', 1))`)
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (`\b` in control string)

### [45] ns-esc-horizontal-tab

BNF: `ns-esc-horizontal-tab ::= 't' | x09`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII horizontal tab (x09) character. This is useful at the start or the end of a line to force a leading or trailing tab to become part of the content."
- Implementation: `rlsp-yaml-parser/src/chars.rs:180` (`decode_escape` — `'t' | '\t' => Some(('\t', 1))`)
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (`\t` in control string)

### [46] ns-esc-line-feed

BNF: `ns-esc-line-feed ::= 'n'`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII line feed (x0A) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:181` (`decode_escape` — `'n' => Some(('\n', 1))`)
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (`\n` in control string); `rlsp-yaml-parser/src/chars.rs:384` (unit test `newline_escape`)

### [47] ns-esc-vertical-tab

BNF: `ns-esc-vertical-tab ::= 'v'`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII vertical tab (x0B) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:182` (`decode_escape` — `'v' => Some(('\x0B', 1))`)
- Test coverage: no direct test

### [48] ns-esc-form-feed

BNF: `ns-esc-form-feed ::= 'f'`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII form feed (x0C) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:183` (`decode_escape` — `'f' => Some(('\x0C', 1))`)
- Test coverage: no direct test

### [49] ns-esc-carriage-return

BNF: `ns-esc-carriage-return ::= 'r'`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII carriage return (x0D) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:184` (`decode_escape` — `'r' => Some(('\r', 1))`)
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (`\r` in hex-esc string)

### [50] ns-esc-escape

BNF: `ns-esc-escape ::= 'e'`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII escape (x1B) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:185` (`decode_escape` — `'e' => Some(('\x1B', 1))`)
- Test coverage: no direct test

### [51] ns-esc-space

BNF: `ns-esc-space ::= x20`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII space (x20) character. This is useful at the start or the end of a line to force a leading or trailing space to become part of the content."
- Implementation: `rlsp-yaml-parser/src/chars.rs:186` (`decode_escape` — `' ' => Some((' ', 1))`)
- Test coverage: no direct test

### [52] ns-esc-double-quote

BNF: `ns-esc-double-quote ::= '"'`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII double quote (x22)."
- Implementation: `rlsp-yaml-parser/src/chars.rs:187` (`decode_escape` — `'"' => Some(('"', 1))`)
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (`\"` in quoted-scalars example)

### [53] ns-esc-slash

BNF: `ns-esc-slash ::= '/'`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII slash (x2F), for JSON compatibility."
- Implementation: `rlsp-yaml-parser/src/chars.rs:188` (`decode_escape` — `'/' => Some(('/', 1))`)
- Test coverage: `tests/yaml-test-suite/src/3UYS.yaml` (escaped slash in double quotes)

### [54] ns-esc-backslash

BNF: `ns-esc-backslash ::= '\'`

- Classification: Conformant
- Spec (§5.7): "Escaped ASCII back slash (x5C)."
- Implementation: `rlsp-yaml-parser/src/chars.rs:189` (`decode_escape` — `'\\' => Some(('\\', 1))`)
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (`\\` in fun-with-backslashes example)

### [55] ns-esc-next-line

BNF: `ns-esc-next-line ::= 'N'`

- Classification: Conformant
- Spec (§5.7): "Escaped Unicode next line (x85) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:190` (`decode_escape` — `'N' => Some(('\u{85}', 1))`)
- Test coverage: `rlsp-yaml-parser/src/chars.rs:387` (unit test `nel_escape`)

### [56] ns-esc-non-breaking-space

BNF: `ns-esc-non-breaking-space ::= '_'`

- Classification: Conformant
- Spec (§5.7): "Escaped Unicode non-breaking space (xA0) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:191` (`decode_escape` — `'_' => Some(('\u{A0}', 1))`)
- Test coverage: `rlsp-yaml-parser/src/chars.rs:388` (unit test `nbsp_escape`)

### [57] ns-esc-line-separator

BNF: `ns-esc-line-separator ::= 'L'`

- Classification: Conformant
- Spec (§5.7): "Escaped Unicode line separator (x2028) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:192` (`decode_escape` — `'L' => Some(('\u{2028}', 1))`)
- Test coverage: `rlsp-yaml-parser/src/chars.rs:389` (unit test `line_sep_escape`)

### [58] ns-esc-paragraph-separator

BNF: `ns-esc-paragraph-separator ::= 'P'`

- Classification: Conformant
- Spec (§5.7): "Escaped Unicode paragraph separator (x2029) character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:193` (`decode_escape` — `'P' => Some(('\u{2029}', 1))`)
- Test coverage: `rlsp-yaml-parser/src/chars.rs:390` (unit test `para_sep_escape`)

### [59] ns-esc-8-bit

BNF: `ns-esc-8-bit ::= 'x' ns-hex-digit{2}`

- Classification: Strict (security-hardened)
- Spec (§5.7): "Escaped 8-bit Unicode character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:194` (`decode_escape` — `'x' => decode_hex_escape(input, 1, 2)`); `rlsp-yaml-parser/src/lexer/quoted.rs:596–605` — if the decoded character is not `c-printable`, the escape is rejected with an error
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (`\x0d\x0a` in hex-esc string); `rlsp-yaml-parser/src/chars.rs:391` (unit test `hex_2digit`)
- Discrepancy: The implementation rejects hex escapes whose decoded character falls outside `c-printable` (`quoted.rs:594-606`); it additionally rejects hex escapes whose decoded character is in the bidi-override range (U+202A–U+202E, U+2066–U+2069) via the bidi-control check at `quoted.rs:608-618`. Named escapes like `\0`, `\a`, `\e`, `\N` are exempt from the c-printable check by design — they produce well-known control characters and are documented as intentional in the source comment at `quoted.rs:594`.
- Rationale: Source comment at `quoted.rs:594`: "Security: for hex escapes (\x, \u, \U), the decoded character must be a YAML c-printable character. Named escapes (\0, \a, \b, …) produce well-known control chars and are exempt from this check." Source comment at `quoted.rs:608`: "Security: reject bidi override characters produced by numeric escapes (\u and \U can reach the bidi range; \x max is U+00FF)."

### [60] ns-esc-16-bit

BNF: `ns-esc-16-bit ::= 'u' ns-hex-digit{4}`

- Classification: Strict (security-hardened)
- Spec (§5.7): "Escaped 16-bit Unicode character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:195` (`decode_escape` — `'u' => decode_hex_escape(input, 1, 4)`); same non-printable rejection applies via `rlsp-yaml-parser/src/lexer/quoted.rs:596–605`
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (`☺`); `rlsp-yaml-parser/src/chars.rs:392` (unit test `hex_4digit`)
- Discrepancy: The implementation rejects hex escapes whose decoded character falls outside `c-printable` (`quoted.rs:594-606`); it additionally rejects hex escapes whose decoded character is in the bidi-override range (U+202A–U+202E, U+2066–U+2069) via the bidi-control check at `quoted.rs:608-618`. Named escapes like `\0`, `\a`, `\e`, `\N` are exempt from the c-printable check by design — they produce well-known control characters and are documented as intentional in the source comment at `quoted.rs:594`.
- Rationale: Source comment at `quoted.rs:594`: "Security: for hex escapes (\x, \u, \U), the decoded character must be a YAML c-printable character. Named escapes (\0, \a, \b, …) produce well-known control chars and are exempt from this check." Source comment at `quoted.rs:608`: "Security: reject bidi override characters produced by numeric escapes (\u and \U can reach the bidi range; \x max is U+00FF)."

### [61] ns-esc-32-bit

BNF: `ns-esc-32-bit ::= 'U' ns-hex-digit{8}`

- Classification: Strict (security-hardened)
- Spec (§5.7): "Escaped 32-bit Unicode character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:196` (`decode_escape` — `'U' => decode_hex_escape(input, 1, 8)`); same non-printable rejection applies via `rlsp-yaml-parser/src/lexer/quoted.rs:596–605`
- Test coverage: `rlsp-yaml-parser/src/chars.rs:393` (unit test `hex_8digit`); `rlsp-yaml-parser/src/chars.rs:394` (unit test `high_plane_codepoint`)
- Discrepancy: The implementation rejects hex escapes whose decoded character falls outside `c-printable` (`quoted.rs:594-606`); it additionally rejects hex escapes whose decoded character is in the bidi-override range (U+202A–U+202E, U+2066–U+2069) via the bidi-control check at `quoted.rs:608-618`. Named escapes like `\0`, `\a`, `\e`, `\N` are exempt from the c-printable check by design — they produce well-known control characters and are documented as intentional in the source comment at `quoted.rs:594`.
- Rationale: Source comment at `quoted.rs:594`: "Security: for hex escapes (\x, \u, \U), the decoded character must be a YAML c-printable character. Named escapes (\0, \a, \b, …) produce well-known control chars and are exempt from this check." Source comment at `quoted.rs:608`: "Security: reject bidi override characters produced by numeric escapes (\u and \U can reach the bidi range; \x max is U+00FF)."

### [62] c-ns-esc-char

BNF: `c-ns-esc-char ::= c-escape ( ns-esc-null | ns-esc-bell | ns-esc-backspace | ns-esc-horizontal-tab | ns-esc-line-feed | ns-esc-vertical-tab | ns-esc-form-feed | ns-esc-carriage-return | ns-esc-escape | ns-esc-space | ns-esc-double-quote | ns-esc-slash | ns-esc-backslash | ns-esc-next-line | ns-esc-non-breaking-space | ns-esc-line-separator | ns-esc-paragraph-separator | ns-esc-8-bit | ns-esc-16-bit | ns-esc-32-bit )`

- Classification: Conformant
- Spec (§5.7): "Note that escape sequences are only interpreted in double-quoted scalars. In all other scalar styles, the \"\\\" character has no special meaning and non-printable characters are not available."
- Implementation: `rlsp-yaml-parser/src/chars.rs:173–199` (`decode_escape`); invoked exclusively from the double-quoted scanner in `rlsp-yaml-parser/src/lexer/quoted.rs:575–620`
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml`; `tests/yaml-test-suite/src/55WF.yaml` (invalid escape code rejected); `rlsp-yaml-parser/src/chars.rs:382–410` (comprehensive unit tests)

## §6

### [63] s-indent(n)

BNF: `s-indent(0) ::= <empty>` / `s-indent(n+1) ::= s-space s-indent(n)`

- Classification: Conformant
- Spec (§6.1): "In YAML block styles, structure is determined by indentation. In general, indentation is defined as a zero or more space characters at the start of a line. To maintain portability, tab characters must not be used in indentation, since different systems treat tabs differently."
- Implementation: `rlsp-yaml-parser/src/lines.rs:142` — `ch == ' '` loop counts only leading space characters for `Line::indent`; tab characters are explicitly excluded from the indent count and the indent value is used throughout block structure comparisons
- Test coverage: `rlsp-yaml-parser/tests/smoke/block_scalars.rs`; `rlsp-yaml-parser/tests/smoke/mappings.rs`; `rlsp-yaml-parser/src/lines.rs:790–796` (unit tests `indent_counts_only_leading_spaces`, `leading_tab_does_not_count_toward_indent`, `tab_after_spaces_does_not_count`)

### [64] s-indent-less-than(n)

BNF: `s-indent-less-than(1) ::= <empty>` / `s-indent-less-than(n+1) ::= s-space s-indent-less-than(n) | <empty>`

- Classification: Conformant
- Spec (§6.1): "A block style construct is terminated when encountering a line which is less indented than the construct."
- Implementation: `rlsp-yaml-parser/src/lines.rs:340` — `line.indent <= base_indent` check in `peek_until_dedent` halts lookahead at the first non-blank line whose indent is not strictly greater than the base, implementing the less-than-n termination rule; same guard applied in block-sequence and block-mapping parsers
- Test coverage: `rlsp-yaml-parser/src/lines.rs:848–854` (unit test `peek_until_dedent_returns_lines_until_indent_le_base`)

### [65] s-indent-less-or-equal(n)

BNF: `s-indent-less-or-equal(0) ::= <empty>` / `s-indent-less-or-equal(n+1) ::= s-space s-indent-less-or-equal(n) | <empty>`

- Classification: Conformant
- Spec (§6.1): "The productions use the notation `s-indent-less-than(n)` and `s-indent-less-or-equal(n)` to express this."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:181–200` — `if next.indent >= content_indent` guard (line 181) determines whether a continuation line meets the content threshold; the complement (`< content_indent`) terminates the scalar; `is_content_line` (line 198) further constrains with `>= content_indent && !after_indent.is_empty()`; `rlsp-yaml-parser/src/event_iter/block/sequence.rs` and `mapping.rs` apply `<= n` guards for flow-key and block-key termination
- Test coverage: `rlsp-yaml-parser/tests/smoke/block_scalars.rs` (indentation-indicator cases)

### [66] s-separate-in-line

BNF: `s-separate-in-line ::= s-white+ | <start-of-line>`

- Classification: Conformant
- Spec (§6.2): "Outside indentation and scalar content, YAML uses white space characters for separation between tokens within a line. Note that such white space may safely include tab characters. Separation spaces are a presentation detail and must not be used to convey content information."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:110` — `trim_start_matches([' ', '\t'])` strips leading spaces and tabs before flow scalar continuation content (single-quoted); `rlsp-yaml-parser/src/lexer/quoted.rs:294` — same for double-quoted continuations; `rlsp-yaml-parser/src/event_iter/directives.rs:89–93` (`find([' ', '\t'])` separates directive name from parameters; whitespace trimmed with `trim_start_matches([' ', '\t'])`)
- Test coverage: `rlsp-yaml-parser/tests/smoke/directives.rs`; `rlsp-yaml-parser/tests/smoke/comments.rs`

### [67] s-line-prefix(n,c)

BNF: `s-line-prefix(n,BLOCK-OUT) ::= s-block-line-prefix(n)` / `s-line-prefix(n,BLOCK-IN) ::= s-block-line-prefix(n)` / `s-line-prefix(n,FLOW-OUT) ::= s-flow-line-prefix(n)` / `s-line-prefix(n,FLOW-IN) ::= s-flow-line-prefix(n)`

- Classification: Conformant
- Spec (§6.3): "Inside scalar content, each line begins with a non-content line prefix. This prefix always includes the indentation. For flow scalar styles it additionally includes all leading white space, which may contain tab characters. Line prefixes are a presentation detail and must not be used to convey content information."
- Implementation: Block context: `rlsp-yaml-parser/src/lexer/block.rs` — continuation lines validated against the block's indent level. Flow context: `rlsp-yaml-parser/src/lexer/quoted.rs:110` — `trim_start_matches([' ', '\t'])` strips both spaces and tabs as line prefix in flow scalar continuations
- Test coverage: `rlsp-yaml-parser/tests/smoke/block_scalars.rs`; `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [68] s-block-line-prefix(n)

BNF: `s-block-line-prefix(n) ::= s-indent(n)`

- Classification: Conformant
- Spec (§6.3): "Inside scalar content, each line begins with a non-content line prefix. This prefix always includes the indentation."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:41–274` — literal block scalar consumes continuation lines and validates that each non-empty line has indent >= the block's `content_indent`; the indent prefix itself is stripped structurally through the line buffer
- Test coverage: `rlsp-yaml-parser/tests/smoke/block_scalars.rs`

### [69] s-flow-line-prefix(n)

BNF: `s-flow-line-prefix(n) ::= s-indent(n) s-separate-in-line?`

- Classification: Conformant
- Spec (§6.3): "For flow scalar styles it additionally includes all leading white space, which may contain tab characters."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:110` — `trim_start_matches([' ', '\t'])` strips both spaces and tabs as flow line prefix on each continuation line; `rlsp-yaml-parser/src/lexer/quoted.rs:294` — same for double-quoted continuations
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [70] l-empty(n,c)

BNF: `l-empty(n,c) ::= ( s-line-prefix(n,c) | s-indent-less-than(n) ) b-as-line-feed`

- Classification: Conformant
- Spec (§6.4): "An empty line line consists of the non-content prefix followed by a line break. […] The semantics of empty lines depend on the scalar style they appear in."
- Implementation: `rlsp-yaml-parser/src/lexer.rs:103–116` (`skip_empty_lines` — consumes lines where `trim_start_matches([' ', '\t'])` is empty); `rlsp-yaml-parser/src/lexer/quoted.rs:112–116` — blank continuation lines inside single-quoted scalars push a literal `'\n'` into the value; `rlsp-yaml-parser/src/lexer/quoted.rs:311–319` — same for double-quoted scalars (counted as `pending_blanks`)
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `rlsp-yaml-parser/tests/smoke/block_scalars.rs`

### [71] b-l-trimmed(n,c)

BNF: `b-l-trimmed(n,c) ::= b-non-content l-empty(n,c)+`

- Classification: Conformant
- Spec (§6.5): "If a line break is followed by an empty line, it is trimmed; the first line break is discarded and the rest are retained as content."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:311–329` — in double-quoted continuations, blank lines are accumulated in `pending_blanks`; when a non-blank line follows, N blank lines produce N literal newlines in the output (the originating break is discarded, the empty-line breaks are retained)
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [72] b-as-space

BNF: `b-as-space ::= b-break`

- Classification: Conformant
- Spec (§6.5): "Otherwise (the following line is not empty), the line break is converted to a single space (x20)."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:331–332` — `owned.push(' ')` when `pending_blanks == 0` and `line_continuation` is false (normal non-blank fold); `rlsp-yaml-parser/src/lexer/quoted.rs:120–122` — same for single-quoted continuation folds
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [73] b-l-folded(n,c)

BNF: `b-l-folded(n,c) ::= b-l-trimmed(n,c) | b-as-space`

- Classification: Conformant
- Spec (§6.5): "A folded non-empty line may end with either of the above line breaks."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:274–340` (`collect_double_quoted_continuations`) — the `pending_blanks` counter selects between `b-l-trimmed` (N>0) and `b-as-space` (N==0) on each fold boundary; `rlsp-yaml-parser/src/lexer/quoted.rs:82–162` — same two-branch logic for single-quoted scalars
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [74] s-flow-folded(n)

BNF: `s-flow-folded(n) ::= s-separate-in-line? b-l-folded(n,FLOW-IN) s-flow-line-prefix(n)`

- Classification: Conformant
- Spec (§6.5): "Folding in flow styles provides more relaxed semantics. Flow styles typically depend on explicit indicators rather than indentation to convey structure. Hence spaces preceding or following the text in a line are a presentation detail and must not be used to convey content information. Once all such spaces have been discarded, all line breaks are folded without exception."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:79–80` — trailing whitespace trimmed from each partial line before fold; `rlsp-yaml-parser/src/lexer/quoted.rs:110` — leading whitespace trimmed from each continuation line; `rlsp-yaml-parser/src/lexer/quoted.rs:112–122` — fold space or newline inserted between
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [75] c-nb-comment-text

BNF: `c-nb-comment-text ::= c-comment nb-char*`

- Classification: Conformant
- Spec (§6.6): "An explicit comment is marked by a `#` indicator. Comments are a presentation detail and must not be used to convey content information. Comments must be separated from other tokens by white space characters."
- Implementation: `rlsp-yaml-parser/src/lexer/comment.rs:30–33` — `starts_with('#')` check after optional leading whitespace; `rlsp-yaml-parser/src/lexer/comment.rs:50–51` — text slice is everything after the `#` on the line
- Test coverage: `rlsp-yaml-parser/tests/smoke/comments.rs`; `rlsp-yaml-parser/src/lexer/comment.rs:108–121` (unit tests `happy_path_text`)

### [76] b-comment

BNF: `b-comment ::= b-non-content | <end-of-input>`

- Classification: Conformant
- Spec (§6.6): "Note: To ensure JSON compatibility, YAML processors must allow for the omission of the final comment line break of the input stream."
- Implementation: `rlsp-yaml-parser/src/lexer/comment.rs:66–76` — the consumed line's full content (up to but not including the terminator) is returned; end-of-input is handled by the `LineBuffer` returning `BreakType::Eof` for the final line, which is consumed and accepted
- Test coverage: `rlsp-yaml-parser/tests/smoke/comments.rs`

### [77] s-b-comment

BNF: `s-b-comment ::= ( s-separate-in-line c-nb-comment-text? )? b-comment`

- Classification: Conformant
- Spec (§6.6): "Comments must be separated from other tokens by white space characters."
- Implementation: `rlsp-yaml-parser/src/lexer.rs:353–382` (`handle_plain_scalar_inline`) — trailing comment handling for inline plain scalar on `---` marker lines: residual content after token value must start with `#` (preceded by implicit whitespace); residual that is non-empty and does not start with `#` is an error; `rlsp-yaml-parser/src/event_iter/directives.rs:126–133` — trailing content after YAML version checked for empty or `#` prefix
- Test coverage: `rlsp-yaml-parser/tests/smoke/comments.rs`; `rlsp-yaml-parser/tests/smoke/directives.rs`

### [78] l-comment

BNF: `l-comment ::= s-separate-in-line c-nb-comment-text? b-comment`

- Classification: Conformant
- Spec (§6.6): "Outside scalar content, comments may appear on a line of their own, independent of the indentation level. Note that outside scalar content, a line containing only white space characters is taken to be a comment line."
- Implementation: `rlsp-yaml-parser/src/lexer/comment.rs:30–31` — `trim_start_matches([' ', '\t'])` followed by `starts_with('#')` — whitespace-only lines return `None` (not a comment), not consumed as comments; `rlsp-yaml-parser/src/lexer.rs:519–524` (`is_blank_not_comment`) — blank-but-not-comment lines are distinguished from comment lines
- Test coverage: `rlsp-yaml-parser/tests/smoke/comments.rs`

### [79] s-l-comments

BNF: `s-l-comments ::= ( s-b-comment | <start-of-line> ) l-comment*`

- Classification: Conformant
- Spec (§6.6): "In most cases, when a line may end with a comment, YAML allows it to be followed by additional comment lines. The only exception is a comment ending a block scalar header."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:33–64` (`consume_preamble_between_docs`) — loops consuming blank and comment lines in sequence; `rlsp-yaml-parser/src/event_iter/directives.rs:237–256` (`skip_and_collect_comments_in_doc`) — same in-document loop; block scalar header explicitly stops at the comment on its header line and does not consume trailing comment lines (enforced in `rlsp-yaml-parser/src/lexer/block.rs`)
- Test coverage: `rlsp-yaml-parser/tests/smoke/comments.rs`

### [80] s-separate(n,c)

BNF: `s-separate(n,BLOCK-OUT) ::= s-separate-lines(n)` / `s-separate(n,BLOCK-IN) ::= s-separate-lines(n)` / `s-separate(n,FLOW-OUT) ::= s-separate-lines(n)` / `s-separate(n,FLOW-IN) ::= s-separate-lines(n)` / `s-separate(n,BLOCK-KEY) ::= s-separate-in-line` / `s-separate(n,FLOW-KEY) ::= s-separate-in-line`

- Classification: Conformant
- Spec (§6.7): "Implicit keys are restricted to a single line. In all other cases, YAML allows tokens to be separated by multi-line (possibly empty) comments."
- Implementation: Block context: `rlsp-yaml-parser/src/event_iter/directives.rs:237–256` — multi-line comment separation between block tokens; flow/key context: `rlsp-yaml-parser/src/event_iter/flow.rs:168` — single-line whitespace separation for flow keys
- Test coverage: `rlsp-yaml-parser/tests/smoke/comments.rs`; `rlsp-yaml-parser/tests/smoke/flow_collections.rs`

### [81] s-separate-lines(n)

BNF: `s-separate-lines(n) ::= ( s-l-comments s-flow-line-prefix(n) ) | s-separate-in-line`

- Classification: Conformant
- Spec (§6.7): "Note that structures following multi-line comment separation must be properly indented, even though there is no such restriction on the separation comment lines themselves."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:33–64` — comment-then-indent path; `rlsp-yaml-parser/src/lexer.rs:156–180` — inline whitespace path for single-line separation
- Test coverage: `rlsp-yaml-parser/tests/smoke/comments.rs`

### [82] l-directive

BNF: `l-directive ::= c-directive ( ns-yaml-directive | ns-tag-directive | ns-reserved-directive ) s-l-comments`

- Classification: Conformant
- Spec (§6.8): "Directives are instructions to the YAML processor. This specification defines two directives, `YAML` and `TAG`, and reserves all other directives for future use. There is no way to define private directives. […] Directives are a presentation detail and must not be used to convey content information."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:70–104` (`parse_directive`) — dispatches on directive name to `parse_yaml_directive`, `parse_tag_directive`, or ignores unknown directives; lexer: `rlsp-yaml-parser/src/lexer.rs:142–146` (`is_directive_line` — `starts_with('%')`)
- Test coverage: `rlsp-yaml-parser/tests/smoke/directives.rs`

### [83] ns-reserved-directive

BNF: `ns-reserved-directive ::= ns-directive-name ( s-separate-in-line ns-directive-parameter )*`

- Classification: Conformant
- Spec (§6.8): "Each directive is specified on a separate non-indented line starting with the `%` indicator, followed by the directive name and a list of parameters. The semantics of these parameters depends on the specific directive. A YAML processor should ignore unknown directives with an appropriate warning."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:97–103` — unknown directive names silently increment `directive_count` and return `Ok(())`; no warning is emitted (spec says "should", not "must")
- Test coverage: `rlsp-yaml-parser/tests/smoke/directives.rs`

### [84] ns-directive-name

BNF: `ns-directive-name ::= ns-char+`

- Classification: Conformant
- Spec (§6.8): "Each directive is specified on a separate non-indented line starting with the `%` indicator, followed by the directive name."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:88–92` — `find([' ', '\t'])` extracts the directive name as a contiguous run of non-whitespace characters after `%`
- Test coverage: `rlsp-yaml-parser/tests/smoke/directives.rs`

### [85] ns-directive-parameter

BNF: `ns-directive-parameter ::= ns-char+`

- Classification: Conformant
- Spec (§6.8): "Each directive is specified on a separate non-indented line starting with the `%` indicator, followed by the directive name and a list of parameters."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:93` — `trim_start_matches([' ', '\t'])` extracts `rest` (everything after the directive name); individual parameter extraction in `parse_yaml_directive` (`find('.')` splits version) and `parse_tag_directive` (`find([' ', '\t'])` splits handle from prefix)
- Test coverage: `rlsp-yaml-parser/tests/smoke/directives.rs`

### [86] ns-yaml-directive

BNF: `ns-yaml-directive ::= "YAML" s-separate-in-line ns-yaml-version`

- Classification: Conformant
- Spec (§6.8.1): "The `YAML` directive specifies the version of YAML the document conforms to. […] A version 1.2 YAML processor must accept documents with an explicit `%YAML 1.2` directive, as well as documents lacking a `YAML` directive. Such documents are assumed to conform to the 1.2 version specification."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:107–156` (`parse_yaml_directive`) — matches name `"YAML"`, splits on `.` for major/minor, accepts 1.x, rejects major ≥ 2
- Test coverage: `rlsp-yaml-parser/tests/smoke/directives.rs`

### [87] ns-yaml-version

BNF: `ns-yaml-version ::= ns-dec-digit+ '.' ns-dec-digit+`

- Classification: Conformant
- Spec (§6.8.1): "A version 1.2 YAML processor must also accept documents with an explicit `%YAML 1.1` directive."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:116–143` — `find('.')` splits on `.`; `parse::<u8>()` validates that major and minor are decimal digit sequences
- Test coverage: `rlsp-yaml-parser/tests/smoke/directives.rs`

### [88] ns-tag-directive

BNF: `ns-tag-directive ::= "TAG" s-separate-in-line c-tag-handle s-separate-in-line ns-tag-prefix`

- Classification: Conformant
- Spec (§6.8.2): "The `TAG` directive establishes a tag shorthand notation for specifying node tags. Each `TAG` directive associates a handle with a prefix. This allows for compact and readable tag notation."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:158–229` (`parse_tag_directive`) — splits on whitespace for handle and prefix, validates handle via `is_valid_tag_handle`, stores in `directive_scope.tag_handles`
- Test coverage: `rlsp-yaml-parser/tests/smoke/directives.rs`; `rlsp-yaml-parser/tests/smoke/tags.rs`

### [89] c-tag-handle

BNF: `c-tag-handle ::= c-named-tag-handle | c-secondary-tag-handle | c-primary-tag-handle`

- Classification: Conformant
- Spec (§6.8.2.1): "The tag handle exactly matches the prefix of the affected tag shorthand. There are three tag handle variants:"
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:281–295` (`is_valid_tag_handle`) — recognises `!`, `!!`, and `!word-chars!` forms
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`; `rlsp-yaml-parser/src/event_iter/properties.rs:461–513` (unit tests `is_valid_tag_handle_*`)

### [90] c-primary-tag-handle

BNF: `c-primary-tag-handle ::= '!'`

- Classification: Conformant
- Spec (§6.8.2.1): "The primary tag handle is a single `!` character. This allows using the most compact possible notation for a single "primary" name space. By default, the prefix associated with this handle is `!`. […]"
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:282–283` — `"!" => true` branch in `is_valid_tag_handle`
- Test coverage: `rlsp-yaml-parser/src/event_iter/properties.rs:461–463` (unit test `is_valid_tag_handle_primary`)

### [91] c-secondary-tag-handle

BNF: `c-secondary-tag-handle ::= "!!"`

- Classification: Conformant
- Spec (§6.8.2.1): "The secondary tag handle is written as `!!`. This allows using a compact notation for a single "secondary" name space. By default, the prefix associated with this handle is `tag:yaml.org,2002:`."
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:283` — `"!!" => true` branch in `is_valid_tag_handle`; `rlsp-yaml-parser/src/event_iter/directive_scope.rs:93–108` — `!!suffix` resolved using custom `"!!"` handle or default prefix `"tag:yaml.org,2002:"`
- Test coverage: `rlsp-yaml-parser/src/event_iter/properties.rs:465–467` (unit test `is_valid_tag_handle_secondary`)

### [92] c-named-tag-handle

BNF: `c-named-tag-handle ::= c-tag ns-word-char+ c-tag`

- Classification: Conformant
- Spec (§6.8.2.1): "A named tag handle surrounds a non-empty name with `!` characters. A handle name must not be used in a tag shorthand unless an explicit `TAG` directive has associated some prefix with it."
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:285–294` — named handle inner word validated with `.is_ascii_alphanumeric() || c == '-'`; matches `ns-word-char` exactly (`[a-zA-Z0-9] | '-'`). Note: inline tag suffixes (`!!my_type`) accept `_` because `ns-uri-char` explicitly includes it; the restriction applies only to `%TAG` directive handle names.
- Test coverage: `rlsp-yaml-parser/src/event_iter/properties.rs:482–533` (unit tests: `is_valid_tag_handle_named_with_hyphen`, `is_valid_tag_handle_rejects_named_with_underscore`, `is_valid_tag_handle_rejects_underscore_only`, `is_valid_tag_handle_rejects_trailing_underscore`, `is_valid_tag_handle_rejects_leading_underscore`); `rlsp-yaml-parser/tests/smoke/directives.rs:827–860` (Group N integration tests: N-1 `tag_handle_named_with_hyphen_is_accepted`, N-2 `tag_handle_named_with_underscore_is_rejected`, N-3 `inline_tag_suffix_with_underscore_is_accepted`)

### [93] ns-tag-prefix

BNF: `ns-tag-prefix ::= c-ns-local-tag-prefix | ns-global-tag-prefix`

- Classification: Conformant
- Spec (§6.8.2.2): "There are two tag prefix variants."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:172–215` — prefix extracted by whitespace split; validated for control characters and length but not strictly checked against local vs global tag prefix grammar (both forms accepted)
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`; `rlsp-yaml-parser/tests/smoke/directives.rs`

### [94] c-ns-local-tag-prefix

BNF: `c-ns-local-tag-prefix ::= c-tag ns-uri-char*`

- Classification: Conformant
- Spec (§6.8.2.2): "If the prefix begins with a `!` character, shorthands using the handle are expanded to a local tag."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:172–215` — prefix stored as-is; `rlsp-yaml-parser/src/event_iter/directive_scope.rs:134–151` — `!suffix` local-tag shorthand expansion preserves local prefix verbatim
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`

### [95] ns-global-tag-prefix

BNF: `ns-global-tag-prefix ::= ns-tag-char ns-uri-char*`

- Classification: Conformant
- Spec (§6.8.2.2): "If the prefix begins with a character other than `!`, it must be a valid URI prefix, and should contain at least the scheme. Shorthands using the associated handle are expanded to globally unique URI tags."
- Implementation: `rlsp-yaml-parser/src/event_iter/directive_scope.rs:92–132` — `!!suffix` (lines 93–109) and named-handle (lines 112–132) expansions concatenate the stored URI prefix with the percent-decoded suffix; URI validity is enforced by control-character rejection in `parse_tag_directive`
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`

### [96] c-ns-properties(n,c)

BNF: `c-ns-properties(n,c) ::= ( c-ns-tag-property ( s-separate(n,c) c-ns-anchor-property )? ) | ( c-ns-anchor-property ( s-separate(n,c) c-ns-tag-property )? )`

- Classification: Conformant
- Spec (§6.9): "Each node may have two optional properties, anchor and tag, in addition to its content. Node properties may be specified in any order before the node's content. Either or both may be omitted."
- Implementation: `rlsp-yaml-parser/src/event_iter/` — `pending_tag` and `pending_anchor` fields accumulate both properties in either order; both are emitted before the node scalar/sequence/mapping event
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`; `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`

### [97] c-ns-tag-property

BNF: `c-ns-tag-property ::= c-verbatim-tag | c-ns-shorthand-tag | c-non-specific-tag`

- Classification: Conformant
- Spec (§6.9.1): "The tag property identifies the type of the native data structure presented by the node. A tag is denoted by the `!` indicator."
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:85–233` (`scan_tag`) — dispatches on character after `!`: `<` → verbatim, `!` → secondary/primary shorthand, tag-chars → named/secondary shorthand, empty/non-tag → non-specific
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`

### [98] c-verbatim-tag

BNF: `c-verbatim-tag ::= "!<" ns-uri-char+ '>'`

- Classification: Conformant
- Spec (§6.9.1): "A tag may be written verbatim by surrounding it with the `<` and `>` characters. In this case, the YAML processor must deliver the verbatim tag as-is to the application. In particular, verbatim tags are not subject to tag resolution. A verbatim tag must either begin with a `!` (a local tag) or be a valid URI (a global tag)."
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:91–164` — `strip_prefix('<')` branch scans URI body byte-by-byte validating against `is_ns_uri_char_single` and `%HH` sequences; empty URI rejected
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`; `rlsp-yaml-parser/src/event_iter/properties.rs:592–769` (unit tests `scan_tag_verbatim_*`)

### [99] c-ns-shorthand-tag

BNF: `c-ns-shorthand-tag ::= c-tag-handle ns-tag-char+`

- Classification: Conformant
- Spec (§6.9.1): "A tag shorthand consists of a valid tag handle followed by a non-empty suffix. The tag handle must be associated with a prefix, either by default or by using a `TAG` directive."
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:166–233` — primary `!!suffix`, named `!handle!suffix`, and secondary `!suffix` branches all scan via `scan_tag_suffix` which validates against `is_ns_tag_char_single` and `%HH`
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`; `rlsp-yaml-parser/src/event_iter/properties.rs:541–588` (unit tests `scan_tag_secondary_*`, `scan_tag_named_handle*`)

### [100] c-non-specific-tag

BNF: `c-non-specific-tag ::= '!'`

- Classification: Conformant
- Spec (§6.9.1): "If a node has no tag property, it is assigned a non-specific tag that needs to be resolved to a specific one. This non-specific tag is `!` for non-plain scalars and `?` for all other nodes."
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:184–189` — when `scan_tag_suffix` returns 0 and content does not start with `<` or `!`, the tag is the bare `!` one-byte slice from `tag_start`
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs`; `rlsp-yaml-parser/src/event_iter/properties.rs:529–537` (unit tests `scan_tag_non_specific_*`)

### [101] c-ns-anchor-property

BNF: `c-ns-anchor-property ::= c-anchor ns-anchor-name`

- Classification: Conformant
- Spec (§6.9.2): "An anchor is denoted by the `&` indicator. It marks a node for future reference. An alias node can then be used to indicate additional inclusions of the anchored node. […] Anchor names must not contain the `[`, `]`, `{`, `}` and `,` characters."
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:22–45` (`scan_anchor_name`) — called after `&` indicator; scans `ns-anchor-char` characters until whitespace, flow indicator, or end
- Test coverage: `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`

### [102] ns-anchor-char

BNF: `ns-anchor-char ::= ns-char - c-flow-indicator`

- Classification: Conformant
- Spec (§6.9.2): "Anchor names must not contain the `[`, `]`, `{`, `}` and `,` characters. These characters would cause ambiguity with flow collection structures."
- Implementation: `rlsp-yaml-parser/src/chars.rs:149–159` (`is_ns_anchor_char`) — `ns-char` range excluding `c-flow-indicator` characters `[`, `]`, `{`, `}`, `,`
- Test coverage: `rlsp-yaml-parser/src/chars.rs:322–348` (unit tests `ns_anchor_char_accepts`, `ns_anchor_char_rejects_flow_indicators`, `ns_anchor_char_rejects`)

### [103] ns-anchor-name

BNF: `ns-anchor-name ::= ns-anchor-char+`

- Classification: Conformant
- Spec (§6.9.2): "An alias node can then be used to indicate additional inclusions of the anchored node. An anchored node need not be referenced by any alias nodes; in particular, it is valid for all nodes to be anchored."
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:27–31` — `.take_while(|&(_, ch)| is_ns_anchor_char(ch))` scans one or more `ns-anchor-char`; empty result → error
- Test coverage: `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`; `rlsp-yaml-parser/src/event_iter/properties.rs:310–389` (unit tests `scan_anchor_name_*`)

## §7

### [104] c-ns-alias-node

BNF: `c-ns-alias-node ::= c-alias ns-anchor-name`

- Classification: Conformant
- Spec (§7.1): "An alias node is denoted by the \"*\" indicator. The alias refers to the most recent preceding node having the same anchor. It is an error for an alias node to use an anchor that does not previously occur in the document. […] Note that an alias node must not specify any properties or content, as these were already specified at the first occurrence of the node."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:1286–1356` (flow context: `*` consumed, `scan_anchor_name` called, `Event::Alias` pushed; tag/anchor-on-alias rejected as errors); `rlsp-yaml-parser/src/event_iter/properties.rs:22–45` (block context alias scanning via `scan_anchor_name`); `rlsp-yaml-parser/src/loader.rs:661–762` (`resolve_alias` handles lossless and resolved modes; undefined alias → `LoadError::UndefinedAlias`)
- Test coverage: `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`; `tests/yaml-test-suite/src/3GZX.yaml` (Spec Example 7.1. Alias Nodes)

### [105] e-scalar

BNF: `e-scalar ::= ""`

- Classification: Conformant
- Spec (§7.2): "YAML allows the node content to be omitted in many cases. Nodes with empty content are interpreted as if they were plain scalars with an empty value. Such nodes are commonly resolved to a \"null\" value."
- Implementation: `rlsp-yaml-parser/src/lib.rs:167–176` (`empty_scalar_event()` builds `Event::Scalar { value: Cow::Borrowed(""), style: Plain, … }`); emitted at `rlsp-yaml-parser/src/event_iter/flow.rs:502–507` (flow: `}` in `explicit_key_pending` state), `rlsp-yaml-parser/src/event_iter/flow.rs:1152` (flow: empty value after `:`), `rlsp-yaml-parser/src/event_iter/block/mapping.rs:590,641,673` (block mapping empty values), `rlsp-yaml-parser/src/event_iter/block/sequence.rs:244,477` (block sequence bare `-`), `rlsp-yaml-parser/src/event_iter/base.rs:57–178` (document-root empty nodes)
- Test coverage: `tests/yaml-test-suite/src/WZ62.yaml` (Spec Example 7.2. Empty Content); `tests/yaml-test-suite/src/FRK4.yaml` (Spec Example 7.3. Completely Empty Flow Nodes)

### [106] e-node

BNF: `e-node ::= e-scalar`

- Classification: Conformant
- Spec (§7.2): "Both the node's properties and node content are optional. This allows for a completely empty node. Completely empty nodes are only valid when following some explicit indication for their existence."
- Implementation: `rlsp-yaml-parser/src/lib.rs:167–176` (`empty_scalar_event()` — `e-node` collapses to `e-scalar`; the parser emits it at all sites listed for [105])
- Test coverage: `tests/yaml-test-suite/src/FRK4.yaml` (Spec Example 7.3. Completely Empty Flow Nodes)

### [107] nb-double-char

BNF: `nb-double-char ::= c-ns-esc-char | ( nb-json - c-escape - c-double-quote )`

- Classification: Conformant
- Spec (§7.3.1): "The double-quoted style is specified by surrounding '\"' indicators. This is the only style capable of expressing arbitrary strings, by using '\\' escape sequences. This comes at the cost of having to escape the '\\' and '\"' characters."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (`try_consume_double_quoted` — `memchr2` scans for `\` and `"`, escape sequences decoded via `decode_escape`; all `nb-json` characters other than `\` and `"` pass through unmodified)
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/7A4E.yaml` (Spec Example 7.6. Double Quoted Lines)

### [108] ns-double-char

BNF: `ns-double-char ::= nb-double-char - s-white`

- Classification: Conformant
- Spec (§7.3.1): "The double-quoted style is specified by surrounding '\"' indicators. This is the only style capable of expressing arbitrary strings, by using '\\' escape sequences."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (whitespace trimming of leading/trailing spaces on each continuation line implements `ns-double-char` in multi-line context; single-space folding via `s-flow-folded`)
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/7A4E.yaml` (Spec Example 7.6. Double Quoted Lines)

### [109] c-double-quoted(n,c)

BNF: `c-double-quoted(n,c) ::= c-double-quote nb-double-text(n,c) c-double-quote`

- Classification: Conformant
- Spec (§7.3.1): "The double-quoted style is specified by surrounding '\"' indicators."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (`try_consume_double_quoted` — opening `"` detected, body consumed via `nb-double-text` logic, closing `"` required)
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/LQZ7.yaml` (Spec Example 7.4. Double Quoted Implicit Keys)

### [110] nb-double-text(n,c)

BNF: `nb-double-text(n,FLOW-OUT) ::= nb-double-multi-line(n)` / `nb-double-text(n,FLOW-IN) ::= nb-double-multi-line(n)` / `nb-double-text(n,BLOCK-KEY) ::= nb-double-one-line` / `nb-double-text(n,FLOW-KEY) ::= nb-double-one-line`

- Classification: Conformant
- Spec (§7.3.1): "Double-quoted scalars are restricted to a single line when contained inside an implicit key."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (multi-line path taken when a closing `"` is not found on the first line; implicit-key context enforces single-line via the block and flow parsers' key-detection logic)
- Test coverage: `tests/yaml-test-suite/src/LQZ7.yaml` (Spec Example 7.4. Double Quoted Implicit Keys — single-line); `tests/yaml-test-suite/src/7A4E.yaml` (Spec Example 7.6. Double Quoted Lines — multi-line)

### [111] nb-double-one-line

BNF: `nb-double-one-line ::= nb-double-char*`

- Classification: Conformant
- Spec (§7.3.1): "Double-quoted scalars are restricted to a single line when contained inside an implicit key."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (single-line fast path: scanning stops at closing `"` without consuming a newline)
- Test coverage: `tests/yaml-test-suite/src/LQZ7.yaml` (Spec Example 7.4. Double Quoted Implicit Keys)

### [112] s-double-escaped(n)

BNF: `s-double-escaped(n) ::= s-white* c-escape b-non-content l-empty(n,FLOW-IN)* s-flow-line-prefix(n)`

- Classification: Conformant
- Spec (§7.3.1): "It is also possible to escape the line break character. In this case, the escaped line break is excluded from the content and any trailing white space characters that precede the escaped line break are preserved."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (escaped-newline handling: `\` at end of line with optional trailing whitespace before it is consumed, newline is excluded from the value, leading whitespace on the next line is preserved)
- Test coverage: `tests/yaml-test-suite/src/NP9H.yaml` (Spec Example 7.5. Double Quoted Line Breaks)

### [113] s-double-break(n)

BNF: `s-double-break(n) ::= s-double-escaped(n) | s-flow-folded(n)`

- Classification: Conformant
- Spec (§7.3.1): "In a multi-line double-quoted scalar, line breaks are subject to flow line folding, which discards any trailing white space characters."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (both branches: `\\\n` escape handled as `s-double-escaped`; plain newline handled as `s-flow-folded` — trailing whitespace stripped, leading whitespace stripped on next line, blank lines become literal `\n`)
- Test coverage: `tests/yaml-test-suite/src/NP9H.yaml` (Spec Example 7.5. Double Quoted Line Breaks); `tests/yaml-test-suite/src/7A4E.yaml` (Spec Example 7.6. Double Quoted Lines)

### [114] nb-ns-double-in-line

BNF: `nb-ns-double-in-line ::= ( s-white* ns-double-char )*`

- Classification: Conformant
- Spec (§7.3.1): "All leading and trailing white space characters on each line are excluded from the content. Each continuation line must therefore contain at least one non-space character."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (inner-line scanning: whitespace between non-whitespace characters preserved; trailing whitespace excluded when line ends)
- Test coverage: `tests/yaml-test-suite/src/7A4E.yaml` (Spec Example 7.6. Double Quoted Lines)

### [115] s-double-next-line(n)

BNF: `s-double-next-line(n) ::= s-double-break(n) ( ns-double-char nb-ns-double-in-line ( s-double-next-line(n) | s-white* ) )?`

- Classification: Conformant
- Spec (§7.3.1): "All leading and trailing white space characters on each line are excluded from the content. Each continuation line must therefore contain at least one non-space character. Empty lines, if any, are consumed as part of the line folding."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (multi-line loop: each new line after a break is checked for non-space content; empty lines accumulated as folded newlines)
- Test coverage: `tests/yaml-test-suite/src/7A4E.yaml` (Spec Example 7.6. Double Quoted Lines)

### [116] nb-double-multi-line(n)

BNF: `nb-double-multi-line(n) ::= nb-ns-double-in-line ( s-double-next-line(n) | s-white* )`

- Classification: Conformant
- Spec (§7.3.1): "All leading and trailing white space characters on each line are excluded from the content. Each continuation line must therefore contain at least one non-space character. Empty lines, if any, are consumed as part of the line folding."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–371` (multi-line double-quoted path: `nb-ns-double-in-line` on first line, then continuation via `s-double-break` loop)
- Test coverage: `tests/yaml-test-suite/src/7A4E.yaml` (Spec Example 7.6. Double Quoted Lines)

### [117] c-quoted-quote

BNF: `c-quoted-quote ::= "''"`

- Classification: Conformant
- Spec (§7.3.2): "The single-quoted style is specified by surrounding \"'\" indicators. Therefore, within a single-quoted scalar, such characters need to be repeated. This is the only form of escaping performed in single-quoted scalars."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:27–163` (`try_consume_single_quoted` — `scan_single_quoted_line` detects `''` as an escaped `'` and includes one `'` in the output)
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/4GC6.yaml` (Spec Example 7.7. Single Quoted Characters)

### [118] nb-single-char

BNF: `nb-single-char ::= c-quoted-quote | ( nb-json - c-single-quote )`

- Classification: Conformant
- Spec (§7.3.2): "The single-quoted style is specified by surrounding \"'\" indicators. Therefore, within a single-quoted scalar, such characters need to be repeated. This is the only form of escaping performed in single-quoted scalars. In particular, the '\\' and '\"' characters may be freely used. This restricts single-quoted scalars to printable characters."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:27–163` (`try_consume_single_quoted` — body scanning: all `nb-json` chars except `'` pass through; `''` decoded to `'`)
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/4GC6.yaml` (Spec Example 7.7. Single Quoted Characters)

### [119] ns-single-char

BNF: `ns-single-char ::= nb-single-char - s-white`

- Classification: Conformant
- Spec (§7.3.2): "In addition, it is only possible to break a long single-quoted line where a space character is surrounded by non-spaces."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:27–163` (continuation-line scanning: leading/trailing whitespace stripped; only non-whitespace characters initiate next-line content)
- Test coverage: `tests/yaml-test-suite/src/PRH3.yaml` (Spec Example 7.9. Single Quoted Lines)

### [120] c-single-quoted(n,c)

BNF: `c-single-quoted(n,c) ::= c-single-quote nb-single-text(n,c) c-single-quote`

- Classification: Conformant
- Spec (§7.3.2): "The single-quoted style is specified by surrounding \"'\" indicators."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:27–163` (`try_consume_single_quoted` — opening `'` detected, body consumed, closing `'` required)
- Test coverage: `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/87E4.yaml` (Spec Example 7.8. Single Quoted Implicit Keys)

### [121] nb-single-text(n,c)

BNF: `nb-single-text(FLOW-OUT) ::= nb-single-multi-line(n)` / `nb-single-text(FLOW-IN) ::= nb-single-multi-line(n)` / `nb-single-text(BLOCK-KEY) ::= nb-single-one-line` / `nb-single-text(FLOW-KEY) ::= nb-single-one-line`

- Classification: Conformant
- Spec (§7.3.2): "Single-quoted scalars are restricted to a single line when contained inside a implicit key."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:27–163` (multi-line path taken when closing `'` not found on first line; implicit-key context enforced by flow/block parsers)
- Test coverage: `tests/yaml-test-suite/src/87E4.yaml` (Spec Example 7.8. Single Quoted Implicit Keys — single-line); `tests/yaml-test-suite/src/PRH3.yaml` (Spec Example 7.9. Single Quoted Lines — multi-line)

### [122] nb-single-one-line

BNF: `nb-single-one-line ::= nb-single-char*`

- Classification: Conformant
- Spec (§7.3.2): "Single-quoted scalars are restricted to a single line when contained inside a implicit key."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:27–163` (single-line fast path: scanning stops at closing `'` without consuming a newline)
- Test coverage: `tests/yaml-test-suite/src/87E4.yaml` (Spec Example 7.8. Single Quoted Implicit Keys)

### [123] nb-ns-single-in-line

BNF: `nb-ns-single-in-line ::= ( s-white* ns-single-char )*`

- Classification: Conformant
- Spec (§7.3.2): "All leading and trailing white space characters are excluded from the content. Each continuation line must therefore contain at least one non-space character. Empty lines, if any, are consumed as part of the line folding."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:27–163` (inner-line whitespace between non-whitespace characters preserved; trailing whitespace excluded)
- Test coverage: `tests/yaml-test-suite/src/PRH3.yaml` (Spec Example 7.9. Single Quoted Lines)

### [124] s-single-next-line(n)

BNF: `s-single-next-line(n) ::= s-flow-folded(n) ( ns-single-char nb-ns-single-in-line ( s-single-next-line(n) | s-white* ) )?`

- Classification: Conformant
- Spec (§7.3.2): "All leading and trailing white space characters are excluded from the content. Each continuation line must therefore contain at least one non-space character. Empty lines, if any, are consumed as part of the line folding."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:27–163` (multi-line loop: each continuation line after `s-flow-folded` folding checked for non-space content)
- Test coverage: `tests/yaml-test-suite/src/PRH3.yaml` (Spec Example 7.9. Single Quoted Lines)

### [125] nb-single-multi-line(n)

BNF: `nb-single-multi-line(n) ::= nb-ns-single-in-line ( s-single-next-line(n) | s-white* )`

- Classification: Conformant
- Spec (§7.3.2): "All leading and trailing white space characters are excluded from the content. Each continuation line must therefore contain at least one non-space character. Empty lines, if any, are consumed as part of the line folding."
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:27–163` (multi-line single-quoted path: `nb-ns-single-in-line` on first line, then continuation via `s-flow-folded` loop)
- Test coverage: `tests/yaml-test-suite/src/PRH3.yaml` (Spec Example 7.9. Single Quoted Lines)

### [126] ns-plain-first(c)

BNF: `ns-plain-first(c) ::= ( ns-char - c-indicator ) | ( ( c-mapping-key | c-mapping-value | c-sequence-entry ) [ lookahead = ns-plain-safe(c) ] )`

- Classification: Conformant
- Spec (§7.3.3): "Plain scalars must not begin with most indicators, as this would cause ambiguity with other YAML constructs. However, the ':', '?' and '-' indicators may be used as the first character if followed by a non-space 'safe' character, as this causes no ambiguity."
- Implementation: `rlsp-yaml-parser/src/lexer/plain.rs:298–313` (`ns_plain_first_block` — `is_c_indicator` check; `?`, `:`, `-` allowed if followed by `ns_plain_safe_block`); flow context: `scan_plain_line_flow` at `rlsp-yaml-parser/src/lexer/plain.rs:442–513` (same first-char policy, callers enforce flow indicator exclusion)
- Test coverage: `tests/yaml-test-suite/src/DBG4.yaml` (Spec Example 7.10. Plain Characters); `rlsp-yaml-parser/src/lexer/plain.rs:563–576` (unit tests `scan_plain_line_block_cases`)

### [127] ns-plain-safe(c)

BNF: `ns-plain-safe(FLOW-OUT) ::= ns-plain-safe-out` / `ns-plain-safe(FLOW-IN) ::= ns-plain-safe-in` / `ns-plain-safe(BLOCK-KEY) ::= ns-plain-safe-out` / `ns-plain-safe(FLOW-KEY) ::= ns-plain-safe-in`

- Classification: Conformant
- Spec (§7.3.3): "Plain scalars must never contain the ': ' and ' #' character combinations. […] In addition, inside flow collections, or when used as implicit keys, plain scalars must not contain the '[', ']', '{', '}' and ',' characters."
- Implementation: `rlsp-yaml-parser/src/lexer/plain.rs:315–319` (`ns_plain_safe_block` — any `ns-char` for block/BLOCK-KEY context); `scan_plain_line_flow` at `rlsp-yaml-parser/src/lexer/plain.rs:442–513` (flow: additionally stops at `,`, `[`, `]`, `{`, `}`)
- Test coverage: `tests/yaml-test-suite/src/DBG4.yaml` (Spec Example 7.10. Plain Characters — inside and outside flow)

### [128] ns-plain-safe-out

BNF: `ns-plain-safe-out ::= ns-char`

- Classification: Conformant
- Spec (§7.3.3): "Plain scalars must never contain the ': ' and ' #' character combinations. Such combinations would cause ambiguity with mapping key/value pairs and comments."
- Implementation: `rlsp-yaml-parser/src/lexer/plain.rs:315–319` (`ns_plain_safe_block` delegates to `is_ns_char`)
- Test coverage: `tests/yaml-test-suite/src/DBG4.yaml` (Spec Example 7.10. Plain Characters)

### [129] ns-plain-safe-in

BNF: `ns-plain-safe-in ::= ns-char - c-flow-indicator`

- Classification: Conformant
- Spec (§7.3.3): "In addition, inside flow collections, or when used as implicit keys, plain scalars must not contain the '[', ']', '{', '}' and ',' characters. These characters would cause ambiguity with flow collection structures."
- Implementation: `rlsp-yaml-parser/src/lexer/plain.rs:442–513` (`scan_plain_line_flow` — terminates at `,`, `[`, `]`, `{`, `}` in addition to block-context terminators)
- Test coverage: `tests/yaml-test-suite/src/DBG4.yaml` (Spec Example 7.10. Plain Characters — inside flow collection)

### [130] ns-plain-char(c)

BNF: `ns-plain-char(c) ::= ( ns-plain-safe(c) - c-mapping-value - c-comment ) | ( [ lookbehind = ns-char ] c-comment ) | ( c-mapping-value [ lookahead = ns-plain-safe(c) ] )`

- Classification: Conformant
- Spec (§7.3.3): "Plain scalars must never contain the ': ' and ' #' character combinations."
- Implementation: `rlsp-yaml-parser/src/lexer/plain.rs:322–341` (`ns_plain_char_block` — `#` allowed only when NOT preceded by whitespace; `:` allowed only when followed by `ns_plain_safe_block`); `scan_plain_line_flow` uses same logic at `rlsp-yaml-parser/src/lexer/plain.rs:442–513`
- Test coverage: `tests/yaml-test-suite/src/DBG4.yaml` (Spec Example 7.10. Plain Characters); `rlsp-yaml-parser/src/lexer/plain.rs:563–576` (unit tests `scan_plain_line_block_cases`)

### [131] ns-plain(n,c)

BNF: `ns-plain(n,FLOW-OUT) ::= ns-plain-multi-line(n,FLOW-OUT)` / `ns-plain(n,FLOW-IN) ::= ns-plain-multi-line(n,FLOW-IN)` / `ns-plain(n,BLOCK-KEY) ::= ns-plain-one-line(BLOCK-KEY)` / `ns-plain(n,FLOW-KEY) ::= ns-plain-one-line(FLOW-KEY)`

- Classification: Conformant
- Spec (§7.3.3): "Plain scalars are further restricted to a single line when contained inside an implicit key."
- Implementation: `rlsp-yaml-parser/src/lexer/plain.rs:31–154` (`try_consume_plain_scalar` for block multi-line; `scan_plain_line_flow` for flow single-line in key/flow context; multi-line in FLOW-OUT/FLOW-IN via `collect_plain_continuations`)
- Test coverage: `tests/yaml-test-suite/src/L9U5.yaml` (Spec Example 7.11. Plain Implicit Keys); `tests/yaml-test-suite/src/HS5T.yaml` (Spec Example 7.12. Plain Lines)

### [132] nb-ns-plain-in-line(c)

BNF: `nb-ns-plain-in-line(c) ::= ( s-white* ns-plain-char(c) )*`

- Classification: Conformant
- Spec (§7.3.3): "In addition to a restricted character set, a plain scalar must not be empty or contain leading or trailing white space characters."
- Implementation: `rlsp-yaml-parser/src/lexer/plain.rs:351–427` (`scan_plain_line_block` — inner loop: whitespace between tokens preserved; trailing whitespace excluded via `committed_end`); `scan_plain_line_flow` at `rlsp-yaml-parser/src/lexer/plain.rs:442–513` (same pattern, flow terminator set)
- Test coverage: `tests/yaml-test-suite/src/DBG4.yaml` (Spec Example 7.10. Plain Characters)

### [133] ns-plain-one-line(c)

BNF: `ns-plain-one-line(c) ::= ns-plain-first(c) nb-ns-plain-in-line(c)`

- Classification: Conformant
- Spec (§7.3.3): "Plain scalars are further restricted to a single line when contained inside an implicit key."
- Implementation: `rlsp-yaml-parser/src/lexer/plain.rs:253–283` (`peek_plain_scalar_first_line` — first char checked via `ns_plain_first_block`, remaining scanned via `scan_plain_line_block`); same pattern in `scan_plain_line_flow` for flow-key context
- Test coverage: `tests/yaml-test-suite/src/L9U5.yaml` (Spec Example 7.11. Plain Implicit Keys)

### [134] s-ns-plain-next-line(n,c)

BNF: `s-ns-plain-next-line(n,c) ::= s-flow-folded(n) ns-plain-char(c) nb-ns-plain-in-line(c)`

- Classification: Conformant
- Spec (§7.3.3): "All leading and trailing white space characters are excluded from the content. Each continuation line must therefore contain at least one non-space character. Empty lines, if any, are consumed as part of the line folding."
- Implementation: `rlsp-yaml-parser/src/lexer/plain.rs:160–241` (`collect_plain_continuations` — blank lines accumulated as pending newlines; non-empty continuation line must have `scan_plain_line_block` produce a non-empty result)
- Test coverage: `tests/yaml-test-suite/src/HS5T.yaml` (Spec Example 7.12. Plain Lines)

### [135] ns-plain-multi-line(n,c)

BNF: `ns-plain-multi-line(n,c) ::= ns-plain-one-line(c) s-ns-plain-next-line(n,c)*`

- Classification: Conformant
- Spec (§7.3.3): "It is only possible to break a long plain line where a space character is surrounded by non-spaces."
- Implementation: `rlsp-yaml-parser/src/lexer/plain.rs:31–154` (`try_consume_plain_scalar` — first line via `peek_plain_scalar_first_line`, then zero or more continuation lines via `collect_plain_continuations`)
- Test coverage: `tests/yaml-test-suite/src/HS5T.yaml` (Spec Example 7.12. Plain Lines)

### [136] in-flow(n,c)

BNF: `in-flow(n,FLOW-OUT) ::= ns-s-flow-seq-entries(n,FLOW-IN)` / `in-flow(n,FLOW-IN) ::= ns-s-flow-seq-entries(n,FLOW-IN)` / `in-flow(n,BLOCK-KEY) ::= ns-s-flow-seq-entries(n,FLOW-KEY)` / `in-flow(n,FLOW-KEY) ::= ns-s-flow-seq-entries(n,FLOW-KEY)`

- Classification: Not Applicable (descriptive)
- Spec (§7.4): "A flow collection may be nested within a block collection (FLOW-OUT context), nested within another flow collection (FLOW-IN context) or be a part of an implicit key (FLOW-KEY context or BLOCK-KEY context). Flow collection entries are terminated by the ',' indicator."
- Implementation: (no implementation obligation)
- Test coverage: (no implementation obligation)

### [137] c-flow-sequence(n,c)

BNF: `c-flow-sequence(n,c) ::= c-sequence-start s-separate(n,c)? in-flow(n,c)? c-sequence-end`

- Classification: Conformant
- Spec (§7.4.1): "Flow sequence content is denoted by surrounding '[' and ']' characters."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:388–459` (`'['` branch pushes `FlowFrame::Sequence`, emits `SequenceStart`; `rlsp-yaml-parser/src/event_iter/flow.rs:465–559` (`']'` branch pops `FlowFrame::Sequence`, emits `SequenceEnd`)
- Test coverage: `tests/yaml-test-suite/src/5KJE.yaml` (Spec Example 7.13. Flow Sequence)

### [138] ns-s-flow-seq-entries(n,c)

BNF: `ns-s-flow-seq-entries(n,c) ::= ns-flow-seq-entry(n,c) s-separate(n,c)? ( c-collect-entry s-separate(n,c)? ns-s-flow-seq-entries(n,c)? )?`

- Classification: Conformant
- Spec (§7.4.1): "Sequence entries are separated by a ',' character."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:652–730` (`,` branch inside `FlowFrame::Sequence` — advances `has_value`, permits trailing comma before `]`)
- Test coverage: `tests/yaml-test-suite/src/5KJE.yaml` (Spec Example 7.13. Flow Sequence); `tests/yaml-test-suite/src/8UDB.yaml` (Spec Example 7.14. Flow Sequence Entries)

### [139] ns-flow-seq-entry(n,c)

BNF: `ns-flow-seq-entry(n,c) ::= ns-flow-pair(n,c) | ns-flow-node(n,c)`

- Classification: Conformant
- Spec (§7.4.1): "Any flow node may be used as a flow sequence entry. In addition, YAML provides a compact notation for the case where a flow sequence entry is a mapping with a single key/value pair."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:388–559` (sequence item dispatch: scalars, nested collections, and single-pair implicit mappings via `:` detection within `FlowFrame::Sequence`)
- Test coverage: `tests/yaml-test-suite/src/8UDB.yaml` (Spec Example 7.14. Flow Sequence Entries)

### [140] c-flow-mapping(n,c)

BNF: `c-flow-mapping(n,c) ::= c-mapping-start s-separate(n,c)? ns-s-flow-map-entries(n,in-flow(c))? c-mapping-end`

- Classification: Conformant
- Spec (§7.4.2): "Flow mappings are denoted by surrounding '{' and '}' characters."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:388–459` (`'{'` branch pushes `FlowFrame::Mapping`, emits `MappingStart`); `rlsp-yaml-parser/src/event_iter/flow.rs:465–559` (`'}'` branch pops `FlowFrame::Mapping`, emits `MappingEnd`)
- Test coverage: `tests/yaml-test-suite/src/5C5M.yaml` (Spec Example 7.15. Flow Mappings)

### [141] ns-s-flow-map-entries(n,c)

BNF: `ns-s-flow-map-entries(n,c) ::= ns-flow-map-entry(n,c) s-separate(n,c)? ( c-collect-entry s-separate(n,c)? ns-s-flow-map-entries(n,c)? )?`

- Classification: Conformant
- Spec (§7.4.2): "Mapping entries are separated by a ',' character."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:652–730` (`,` branch inside `FlowFrame::Mapping` — resets to Key phase, permits trailing comma before `}`)
- Test coverage: `tests/yaml-test-suite/src/5C5M.yaml` (Spec Example 7.15. Flow Mappings); `tests/yaml-test-suite/src/DFF7.yaml` (Spec Example 7.16. Flow Mapping Entries)

### [142] ns-flow-map-entry(n,c)

BNF: `ns-flow-map-entry(n,c) ::= ( c-mapping-key s-separate(n,c) ns-flow-map-explicit-entry(n,c) ) | ns-flow-map-implicit-entry(n,c)`

- Classification: Conformant
- Spec (§7.4.2): "If the optional '?' mapping key indicator is specified, the rest of the entry may be completely empty."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:1011–1043` (`'?'` explicit-key indicator branch inside `FlowFrame::Mapping`; implicit-key entry falls through to the scalar/collection dispatch)
- Test coverage: `tests/yaml-test-suite/src/DFF7.yaml` (Spec Example 7.16. Flow Mapping Entries)

### [143] ns-flow-map-explicit-entry(n,c)

BNF: `ns-flow-map-explicit-entry(n,c) ::= ns-flow-map-implicit-entry(n,c) | ( e-node e-node )`

- Classification: Conformant
- Spec (§7.4.2): "If the optional '?' mapping key indicator is specified, the rest of the entry may be completely empty."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:495–511` (when `}` arrives with `explicit_key_pending = true` in Key phase, two `empty_scalar_event()` pushed — empty key and empty value)
- Test coverage: `tests/yaml-test-suite/src/DFF7.yaml` (Spec Example 7.16. Flow Mapping Entries)

### [144] ns-flow-map-implicit-entry(n,c)

BNF: `ns-flow-map-implicit-entry(n,c) ::= ns-flow-map-yaml-key-entry(n,c) | c-ns-flow-map-empty-key-entry(n,c) | c-ns-flow-map-json-key-entry(n,c)`

- Classification: Conformant
- Spec (§7.4.2): "Normally, YAML insists the ':' mapping value indicator be separated from the value by white space."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:730–1200` (dispatch on current char: `:` alone → empty key entry; quoted scalar → JSON-key entry; plain scalar or nested collection → YAML-key entry)
- Test coverage: `tests/yaml-test-suite/src/4ABK.yaml`; `tests/yaml-test-suite/src/DFF7.yaml` (Spec Example 7.16. Flow Mapping Entries)

### [145] ns-flow-map-yaml-key-entry(n,c)

BNF: `ns-flow-map-yaml-key-entry(n,c) ::= ns-flow-yaml-node(n,c) ( ( s-separate(n,c)? c-ns-flow-map-separate-value(n,c) ) | e-node )`

- Classification: Conformant
- Spec (§7.4.2): "Normally, YAML insists the ':' mapping value indicator be separated from the value by white space. A benefit of this restriction is that the ':' character can be used inside plain scalars, as long as it is not followed by white space."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:730–1200` (plain scalar or nested collection as key in `FlowFrame::Mapping` Key phase; `:` with trailing space or flow indicator consumed in Value phase)
- Test coverage: `tests/yaml-test-suite/src/4ABK.yaml`; `tests/yaml-test-suite/src/DFF7.yaml` (Spec Example 7.16. Flow Mapping Entries)

### [146] c-ns-flow-map-empty-key-entry(n,c)

BNF: `c-ns-flow-map-empty-key-entry(n,c) ::= e-node c-ns-flow-map-separate-value(n,c)`

- Classification: Conformant
- Spec (§7.4.2): "Note that the value may be completely empty since its existence is indicated by the ':'."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:1109–1165` (`:` at start of key position in `FlowFrame::Mapping` → `empty_scalar_event()` pushed for empty key, then value consumed)
- Test coverage: `tests/yaml-test-suite/src/4ABK.yaml`; `tests/yaml-test-suite/src/DFF7.yaml` (Spec Example 7.16. Flow Mapping Entries)

### [147] c-ns-flow-map-separate-value(n,c)

BNF: `c-ns-flow-map-separate-value(n,c) ::= c-mapping-value [ lookahead ≠ ns-plain-safe(c) ] ( ( s-separate(n,c) ns-flow-node(n,c) ) | e-node )`

- Classification: Conformant
- Spec (§7.4.2): "Normally, YAML insists the ':' mapping value indicator be separated from the value by white space. A benefit of this restriction is that the ':' character can be used inside plain scalars, as long as it is not followed by white space. This allows for unquoted URLs and timestamps."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:730–850` (`:` in Value phase checked for trailing space/flow-indicator via `ns_plain_safe_block` lookahead; `:x` treated as plain-scalar content not a separator)
- Test coverage: `tests/yaml-test-suite/src/4ABK.yaml` (flow mapping separate values); `tests/yaml-test-suite/src/DFF7.yaml` (Spec Example 7.16. Flow Mapping Entries)

### [148] c-ns-flow-map-json-key-entry(n,c)

BNF: `c-ns-flow-map-json-key-entry(n,c) ::= c-flow-json-node(n,c) ( ( s-separate(n,c)? c-ns-flow-map-adjacent-value(n,c) ) | e-node )`

- Classification: Conformant
- Spec (§7.4.2): "To ensure JSON compatibility, if a key inside a flow mapping is JSON-like, YAML allows the following value to be specified adjacent to the ':'. This causes no ambiguity, as all JSON-like keys are surrounded by indicators."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:730–1200` (quoted scalar as key in `FlowFrame::Mapping` Key phase; `:` without mandatory preceding space allowed immediately after closing `"` or `'`)
- Test coverage: `tests/yaml-test-suite/src/C2DT.yaml` (Spec Example 7.18. Flow Mapping Adjacent Values)

### [149] c-ns-flow-map-adjacent-value(n,c)

BNF: `c-ns-flow-map-adjacent-value(n,c) ::= c-mapping-value ( ( s-separate(n,c)? ns-flow-node(n,c) ) | e-node )`

- Classification: Conformant
- Spec (§7.4.2): "To ensure JSON compatibility, if a key inside a flow mapping is JSON-like, YAML allows the following value to be specified adjacent to the ':'."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:730–850` (`:` in Value phase after a JSON-like key: space before value is optional; value may be omitted → `empty_scalar_event()`)
- Test coverage: `tests/yaml-test-suite/src/C2DT.yaml` (Spec Example 7.18. Flow Mapping Adjacent Values)

### [150] ns-flow-pair(n,c)

BNF: `ns-flow-pair(n,c) ::= ( c-mapping-key s-separate(n,c) ns-flow-map-explicit-entry(n,c) ) | ns-flow-pair-entry(n,c)`

- Classification: Conformant
- Spec (§7.4.3): "A more compact notation is usable inside flow sequences, if the mapping contains a single key/value pair. This notation does not require the surrounding '{' and '}' characters. Note that it is not possible to specify any node properties for the mapping in this case."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:1359–1620` (`:` value separator inside `FlowFrame::Sequence` triggers single-pair implicit mapping: `MappingStart` inserted before the key, `MappingEnd` emitted before the next `,` or `]`)
- Test coverage: `tests/yaml-test-suite/src/QF4Y.yaml` (Spec Example 7.19. Single Pair Flow Mappings); `tests/yaml-test-suite/src/CT4Q.yaml` (Spec Example 7.20. Single Pair Explicit Entry)

### [151] ns-flow-pair-entry(n,c)

BNF: `ns-flow-pair-entry(n,c) ::= ns-flow-pair-yaml-key-entry(n,c) | c-ns-flow-map-empty-key-entry(n,c) | c-ns-flow-pair-json-key-entry(n,c)`

- Classification: Conformant
- Spec (§7.4.3): "A more compact notation is usable inside flow sequences, if the mapping contains a single key/value pair."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:1359–1620` (dispatch within `FlowFrame::Sequence` after `:` detected: plain/alias key, empty-key, or quoted-JSON key)
- Test coverage: `tests/yaml-test-suite/src/9MMW.yaml` (Single Pair Implicit Entries)

### [152] ns-flow-pair-yaml-key-entry(n,c)

BNF: `ns-flow-pair-yaml-key-entry(n,c) ::= ns-s-implicit-yaml-key(FLOW-KEY) c-ns-flow-map-separate-value(n,c)`

- Classification: Conformant
- Spec (§7.4.3): "A more compact notation is usable inside flow sequences, if the mapping contains a single key/value pair."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:1359–1620` (plain scalar key in sequence entry followed by `:` separator)
- Test coverage: `tests/yaml-test-suite/src/9MMW.yaml` (Single Pair Implicit Entries)

### [153] c-ns-flow-pair-json-key-entry(n,c)

BNF: `c-ns-flow-pair-json-key-entry(n,c) ::= c-s-implicit-json-key(FLOW-KEY) c-ns-flow-map-adjacent-value(n,c)`

- Classification: Conformant
- Spec (§7.4.3): "A more compact notation is usable inside flow sequences, if the mapping contains a single key/value pair."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:1359–1620` (quoted scalar as key in sequence entry, adjacent `:` separator)
- Test coverage: `tests/yaml-test-suite/src/9MMW.yaml` (Single Pair Implicit Entries — `{JSON: like}:adjacent` case)

### [154] ns-s-implicit-yaml-key(c)

BNF: `ns-s-implicit-yaml-key(c) ::= ns-flow-yaml-node(0,c) s-separate-in-line? /* At most 1024 characters altogether */`

- Classification: Conformant
- Spec (§7.4.3): "To limit the amount of lookahead required, the ':' indicator must appear at most 1024 Unicode characters beyond the start of the key. In addition, the key is restricted to a single line."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:1124–1149` (single-line restriction and 1024-Unicode-character limit both enforced; plain YAML-key and quoted JSON-key forms share the same check via `key_start_byte` tracking)
- Test coverage: `rlsp-yaml-parser/tests/implicit_key_length.rs` (groups A–N and H5–H8, 48 cases)

### [155] c-s-implicit-json-key(c)

BNF: `c-s-implicit-json-key(c) ::= c-flow-json-node(0,c) s-separate-in-line? /* At most 1024 characters altogether */`

- Classification: Conformant
- Spec (§7.4.3): "To limit the amount of lookahead required, the ':' indicator must appear at most 1024 Unicode characters beyond the start of the key."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:1124–1149` (quoted JSON-key start byte recorded at `flow.rs:1616`; the shared 1024-char check at the `:` separator covers both plain and quoted implicit keys)
- Test coverage: `rlsp-yaml-parser/tests/implicit_key_length.rs` (groups A–N and H5–H8, 48 cases)

### [156] ns-flow-yaml-content(n,c)

BNF: `ns-flow-yaml-content(n,c) ::= ns-plain(n,c)`

- Classification: Conformant
- Spec (§7.5): "JSON-like flow styles all have explicit start and end indicators. The only flow style that does not have this property is the plain scalar."
- Implementation: `rlsp-yaml-parser/src/lexer/plain.rs:429–513` (`scan_plain_line_flow` for flow context); `rlsp-yaml-parser/src/lexer/plain.rs:31–154` (block context plain scalars)
- Test coverage: `tests/yaml-test-suite/src/Q88A.yaml` (Spec Example 7.23. Flow Content — plain case)

### [157] c-flow-json-content(n,c)

BNF: `c-flow-json-content(n,c) ::= c-flow-sequence(n,c) | c-flow-mapping(n,c) | c-single-quoted(n,c) | c-double-quoted(n,c)`

- Classification: Conformant
- Spec (§7.5): "JSON-like flow styles all have explicit start and end indicators."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:388–1620` (all four: `[`, `{`, `'`, `"` dispatch to their respective handlers)
- Test coverage: `tests/yaml-test-suite/src/Q88A.yaml` (Spec Example 7.23. Flow Content)

### [158] ns-flow-content(n,c)

BNF: `ns-flow-content(n,c) ::= ns-flow-yaml-content(n,c) | c-flow-json-content(n,c)`

- Classification: Conformant
- Spec (§7.5): "JSON-like flow styles all have explicit start and end indicators. The only flow style that does not have this property is the plain scalar."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:388–1620` (unified dispatch: all flow-content forms handled in the main character-dispatch loop)
- Test coverage: `tests/yaml-test-suite/src/Q88A.yaml` (Spec Example 7.23. Flow Content)

### [159] ns-flow-yaml-node(n,c)

BNF: `ns-flow-yaml-node(n,c) ::= c-ns-alias-node | ns-flow-yaml-content(n,c) | ( c-ns-properties(n,c) ( ( s-separate(n,c) ns-flow-yaml-content(n,c) ) | e-scalar ) )`

- Classification: Conformant
- Spec (§7.5): "A complete flow node also has optional node properties, except for alias nodes which refer to the anchored node properties."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:1242–1357` (alias at `*`, anchor/tag properties at `&`/`!`, then plain scalar or nested collection; empty scalar when properties present but no content follows)
- Test coverage: `tests/yaml-test-suite/src/LE5A.yaml` (Spec Example 7.24. Flow Nodes)

### [160] c-flow-json-node(n,c)

BNF: `c-flow-json-node(n,c) ::= ( c-ns-properties(n,c) s-separate(n,c) )? c-flow-json-content(n,c)`

- Classification: Conformant
- Spec (§7.5): "A complete flow node also has optional node properties, except for alias nodes which refer to the anchored node properties."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:1199–1240` (tag/anchor properties scanned before `"`, `'`, `[`, `{` dispatch)
- Test coverage: `tests/yaml-test-suite/src/LE5A.yaml` (Spec Example 7.24. Flow Nodes — `!!str "a"`, `&anchor "c"` cases)

### [161] ns-flow-node(n,c)

BNF: `ns-flow-node(n,c) ::= c-ns-alias-node | ns-flow-content(n,c) | ( c-ns-properties(n,c) ( ( s-separate(n,c) ns-flow-content(n,c) ) | e-scalar ) )`

- Classification: Conformant
- Spec (§7.5): "A complete flow node also has optional node properties, except for alias nodes which refer to the anchored node properties."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:388–1620` (top-level dispatch in the flow parser loop: alias, properties + content, or bare content; empty scalar when properties present but content absent)
- Test coverage: `tests/yaml-test-suite/src/LE5A.yaml` (Spec Example 7.24. Flow Nodes)

## §8

### [162] c-b-block-header(t)

BNF: `c-b-block-header(t) ::= ( ( c-indentation-indicator c-chomping-indicator(t) ) | ( c-chomping-indicator(t) c-indentation-indicator ) ) s-b-comment`

- Classification: Conformant
- Spec (§8.1.1): "Block scalars are controlled by a few indicators given in a header preceding the content itself. This header is followed by a non-content line break with an optional comment. This is the only case where a comment must not be followed by additional comment lines."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:506–625` (`parse_block_header` parses either order of indicators, validates that only optional whitespace + comment follow, and enforces no trailing non-comment content)
- Test coverage: `tests/yaml-test-suite/src/P2AD.yaml` (Spec Example 8.1. Block Scalar Header); `rlsp-yaml-parser/src/lexer/block.rs:726–759` (unit tests H-A: header parsing happy path)

### [163] c-indentation-indicator

BNF: `c-indentation-indicator ::= [x31-x39]    # 1-9`

- Classification: Conformant
- Spec (§8.1.1.1): "If a block scalar has an indentation indicator, then the content indentation level of the block scalar is equal to the indentation level of the block scalar plus the integer value of the indentation indicator character. […] It is an error if any non-empty line does not begin with a number of spaces greater than or equal to the content indentation level."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:579–593` (digits `'1'..='9'` map to explicit indent; `'0'` is rejected as invalid)
- Test coverage: `tests/yaml-test-suite/src/R4YG.yaml` (Spec Example 8.2. Block Indentation Indicator); `rlsp-yaml-parser/src/lexer/block.rs:870–893` (unit tests H-E: explicit indent indicator)

### [164] c-chomping-indicator(t)

BNF: `c-chomping-indicator(STRIP) ::= '-'` / `c-chomping-indicator(KEEP) ::= '+'` / `c-chomping-indicator(CLIP) ::= ""`

- Classification: Conformant
- Spec (§8.1.1.2): "Stripping is specified by the '-' chomping indicator. […] Clipping is the default behavior used if no explicit chomping indicator is specified. […] Keeping is specified by the '+' chomping indicator."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:538–565` (`'+'` → `Chomp::Keep`; `'-'` → `Chomp::Strip`; absent → `Chomp::Clip` default at line 624)
- Test coverage: `tests/yaml-test-suite/src/A6F9.yaml` (Spec Example 8.4. Chomping Final Line Break); `rlsp-yaml-parser/src/lexer/block.rs:726–759` (unit tests H-A)

### [165] b-chomped-last(t)

BNF: `b-chomped-last(STRIP) ::= b-non-content | <end-of-input>` / `b-chomped-last(CLIP) ::= b-as-line-feed | <end-of-input>` / `b-chomped-last(KEEP) ::= b-as-line-feed | <end-of-input>`

- Classification: Conformant
- Spec (§8.1.1.2): "The interpretation of the final line break of a block scalar is controlled by the chomping indicator specified in the block scalar header."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:640–670` (`apply_chomping`: Strip removes the trailing `\n`; Clip preserves exactly one `\n`; Keep preserves the `\n` from the last content line before appending blank lines)
- Test coverage: `tests/yaml-test-suite/src/A6F9.yaml` (Spec Example 8.4. Chomping Final Line Break); `rlsp-yaml-parser/src/lexer/block.rs:836–865` (unit tests H-D)

### [166] l-chomped-empty(n,t)

BNF: `l-chomped-empty(n,STRIP) ::= l-strip-empty(n)` / `l-chomped-empty(n,CLIP) ::= l-strip-empty(n)` / `l-chomped-empty(n,KEEP) ::= l-keep-empty(n)`

- Classification: Conformant
- Spec (§8.1.1.2): "The interpretation of the trailing empty lines following a block scalar is also controlled by the chomping indicator specified in the block scalar header."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:640–670` (`apply_chomping`: Strip and Clip discard trailing blank lines (`trailing_blank_count` ignored); Keep appends `trailing_blank_count` newlines via `repeat_n`)
- Test coverage: `tests/yaml-test-suite/src/F8F9.yaml` (Spec Example 8.5. Chomping Trailing Lines); `tests/yaml-test-suite/src/K858.yaml` (Spec Example 8.6. Empty Scalar Chomping)

### [167] l-strip-empty(n)

BNF: `l-strip-empty(n) ::= ( s-indent-less-or-equal(n) b-non-content )* l-trail-comments(n)?`

- Classification: Conformant
- Spec (§8.1.1.2): "The interpretation of the trailing empty lines following a block scalar is also controlled by the chomping indicator specified in the block scalar header."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:246–260` (whitespace-only blank lines below `content_indent` are consumed and counted in `trailing_newlines`; they are discarded by `apply_chomping` for Strip/Clip)
- Test coverage: `tests/yaml-test-suite/src/F8F9.yaml` (Spec Example 8.5. Chomping Trailing Lines)

### [168] l-keep-empty(n)

BNF: `l-keep-empty(n) ::= l-empty(n,BLOCK-IN)* l-trail-comments(n)?`

- Classification: Conformant
- Spec (§8.1.1.2): "Keeping is specified by the '+' chomping indicator. In this case, the final line break and any trailing empty lines are considered to be part of the scalar's content."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:246–260` (blank lines counted into `trailing_newlines`); `rlsp-yaml-parser/src/lexer/block.rs:664–668` (`Chomp::Keep` branch in `apply_chomping` appends all trailing newlines)
- Test coverage: `tests/yaml-test-suite/src/F8F9.yaml` (Spec Example 8.5. Chomping Trailing Lines); `tests/yaml-test-suite/src/K858.yaml` (Spec Example 8.6. Empty Scalar Chomping)

### [169] l-trail-comments(n)

BNF: `l-trail-comments(n) ::= s-indent-less-than(n) c-nb-comment-text b-comment l-comment*`

- Classification: Conformant
- Spec (§8.1.1.2): "Explicit comment lines may follow the trailing empty lines. To prevent ambiguity, the first such comment line must be less indented than the block scalar content. Additional comment lines, if any, are not so restricted. This is the only case where the indentation of comment lines is constrained."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:246–260` (dedented non-whitespace terminates the scalar; comments at dedented positions are not consumed as scalar content — they terminate via the non-whitespace dedent branch); trailing comment extraction via `rlsp-yaml-parser/src/lexer/comment.rs`
- Test coverage: `tests/yaml-test-suite/src/F8F9.yaml` (Spec Example 8.5. Chomping Trailing Lines — comment lines following block scalar)

### [170] c-l+literal(n)

BNF: `c-l+literal(n) ::= c-literal c-b-block-header(t) l-literal-content(n+m,t)`

- Classification: Conformant
- Spec (§8.1.2): "The literal style is denoted by the '|' indicator. It is the simplest, most restricted and most readable scalar style."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:41–274` (`try_consume_literal_block_scalar`: dispatches on `|`, parses header via `parse_block_header`, collects literal content lines)
- Test coverage: `tests/yaml-test-suite/src/M9B4.yaml` (Spec Example 8.7. Literal Scalar); `tests/yaml-test-suite/src/DWX9.yaml` (Spec Example 8.8. Literal Content); `rlsp-yaml-parser/tests/smoke/block_scalars.rs`

### [171] l-nb-literal-text(n)

BNF: `l-nb-literal-text(n) ::= l-empty(n,BLOCK-IN)* s-indent(n) nb-char+`

- Classification: Conformant
- Spec (§8.1.2): "Inside literal scalars, all (indented) characters are considered to be content, including white space characters. Note that all line break characters are normalized."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:181–244` (content line detection: `indent >= content_indent` and non-empty after stripping indent prefix; leading blank lines (`l-empty`) accumulated before first real content)
- Test coverage: `tests/yaml-test-suite/src/DWX9.yaml` (Spec Example 8.8. Literal Content); `rlsp-yaml-parser/src/lexer/block.rs:798–817` (unit tests H-C: clip content collection)

### [172] b-nb-literal-next(n)

BNF: `b-nb-literal-next(n) ::= b-as-line-feed l-nb-literal-text(n)`

- Classification: Conformant
- Spec (§8.1.2): "Inside literal scalars, all (indented) characters are considered to be content, including white space characters. Note that all line break characters are normalized."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:228–244` (each content line adds `\n` via `out.push('\n')` when `break_type != BreakType::Eof`, then the next line is collected as literal text)
- Test coverage: `tests/yaml-test-suite/src/DWX9.yaml` (Spec Example 8.8. Literal Content)

### [173] l-literal-content(n,t)

BNF: `l-literal-content(n,t) ::= ( l-nb-literal-text(n) b-nb-literal-next(n)* b-chomped-last(t) )? l-chomped-empty(n,t)`

- Classification: Conformant
- Spec (§8.1.2): "In addition, empty lines are not folded, though final line breaks and trailing empty lines are chomped."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:117–274` (full content collection loop); `rlsp-yaml-parser/src/lexer/block.rs:263–267` (`apply_chomping` call applying the chomping rules to the assembled content)
- Test coverage: `tests/yaml-test-suite/src/DWX9.yaml` (Spec Example 8.8. Literal Content); `tests/yaml-test-suite/src/A6F9.yaml` (Spec Example 8.4. Chomping Final Line Break)

### [174] c-l+folded(n)

BNF: `c-l+folded(n) ::= c-folded c-b-block-header(t) l-folded-content(n+m,t)`

- Classification: Conformant
- Spec (§8.1.3): "The folded style is denoted by the '>' indicator. It is similar to the literal style; however, folded scalars are subject to line folding."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:288–351` (`try_consume_folded_block_scalar`: dispatches on `>`, parses header via `parse_block_header`, collects folded content via `collect_folded_lines`)
- Test coverage: `tests/yaml-test-suite/src/G992.yaml` (Spec Example 8.9. Folded Scalar); `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.10–8.13. Folded Lines — Final Empty Lines); `rlsp-yaml-parser/tests/smoke/folded_scalars.rs`

### [175] s-nb-folded-text(n)

BNF: `s-nb-folded-text(n) ::= s-indent(n) ns-char nb-char*`

- Classification: Conformant
- Spec (§8.1.3): "Folding allows long lines to be broken anywhere a single space character separates two non-space characters."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:412–413` (`is_content_line`: content line requires `indent >= content_indent` and non-whitespace content — `!after_indent.trim_end_matches(' ').is_empty()`)
- Test coverage: `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.10. Folded Lines)

### [176] l-nb-folded-lines(n)

BNF: `l-nb-folded-lines(n) ::= s-nb-folded-text(n) ( b-l-folded(n,BLOCK-IN) s-nb-folded-text(n) )*`

- Classification: Conformant
- Spec (§8.1.3): "Folding allows long lines to be broken anywhere a single space character separates two non-space characters."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:415–452` (equally-indented non-spaced consecutive content lines are joined with a single space `out.push(' ')`)
- Test coverage: `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.10. Folded Lines)

### [177] s-nb-spaced-text(n)

BNF: `s-nb-spaced-text(n) ::= s-indent(n) s-white nb-char*`

- Classification: Conformant
- Spec (§8.1.3): "Lines starting with white space characters (more-indented lines) are not folded."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:419–421` (`is_more_indented`: `next.indent > content_indent || after_indent.starts_with([' ', '\t'])` — lines whose content after the indent prefix starts with whitespace are classified as spaced/more-indented and not folded)
- Test coverage: `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.11. More Indented Lines)

### [178] b-l-spaced(n)

BNF: `b-l-spaced(n) ::= b-as-line-feed l-empty(n,BLOCK-IN)*`

- Classification: Conformant
- Spec (§8.1.3): "Lines starting with white space characters (more-indented lines) are not folded."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:423–435` (when preceding or current line is `is_more_indented`, the break is preserved as `\n` rather than folded to a space)
- Test coverage: `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.11. More Indented Lines)

### [179] l-nb-spaced-lines(n)

BNF: `l-nb-spaced-lines(n) ::= s-nb-spaced-text(n) ( b-l-spaced(n) s-nb-spaced-text(n) )*`

- Classification: Conformant
- Spec (§8.1.3): "Lines starting with white space characters (more-indented lines) are not folded."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:415–452` (spaced lines: consecutive `is_more_indented` content lines joined with `\n`, not space)
- Test coverage: `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.11. More Indented Lines)

### [180] l-nb-same-lines(n)

BNF: `l-nb-same-lines(n) ::= l-empty(n,BLOCK-IN)* ( l-nb-folded-lines(n) | l-nb-spaced-lines(n) )`

- Classification: Conformant
- Spec (§8.1.3): "Line breaks and empty lines separating folded and more-indented lines are also not folded."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:415–452` (empty lines between content blocks accumulated in `trailing_newlines`; when a content line is reached after blank lines, the classification of the surrounding lines determines folding vs preservation)
- Test coverage: `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.12. Empty Separation Lines)

### [181] l-nb-diff-lines(n)

BNF: `l-nb-diff-lines(n) ::= l-nb-same-lines(n) ( b-as-line-feed l-nb-same-lines(n) )*`

- Classification: Conformant
- Spec (§8.1.3): "Line breaks and empty lines separating folded and more-indented lines are also not folded."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:415–452` (the full `collect_folded_lines` loop handles sequences of same-group blocks separated by blank lines, with `trailing_newlines + extra` logic for transitions between folded and spaced groups)
- Test coverage: `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.12. Empty Separation Lines)

### [182] l-folded-content(n,t)

BNF: `l-folded-content(n,t) ::= ( l-nb-diff-lines(n) b-chomped-last(t) )? l-chomped-empty(n,t)`

- Classification: Conformant
- Spec (§8.1.3): "The final line break and trailing empty lines if any, are subject to chomping and are never folded."
- Implementation: `rlsp-yaml-parser/src/lexer/block.rs:340–351` (`collect_folded_lines` returns `(content, trailing_newlines)`; `apply_chomping` applies the chomp indicator to the assembled content)
- Test coverage: `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.13. Final Empty Lines)

### [183] l+block-sequence(n)

BNF: `l+block-sequence(n) ::= ( s-indent(n+1+m) c-l-block-seq-entry(n+1+m) )+`

- Classification: Conformant
- Spec (§8.2.1): "A block sequence is simply a series of nodes, each denoted by a leading '-' indicator. The '-' indicator must be separated from the node by white space."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/sequence.rs:129–218` (`handle_sequence_entry`: opens a new `CollectionEntry::Sequence` when `dash_indent > parent_col`, requiring sequences to be strictly more indented than their enclosing block node)
- Test coverage: `tests/yaml-test-suite/src/JQ4R.yaml` (Spec Example 8.14. Block Sequence); `rlsp-yaml-parser/tests/smoke/sequences.rs`

### [184] c-l-block-seq-entry(n)

BNF: `c-l-block-seq-entry(n) ::= c-sequence-entry [ lookahead ≠ ns-char ] s-l+block-indented(n,BLOCK-IN)`

- Classification: Conformant
- Spec (§8.2.1): "A block sequence is simply a series of nodes, each denoted by a leading '-' indicator. The '-' indicator must be separated from the node by white space. This allows '-' to be used as the first character in a plain scalar if followed by a non-space character (e.g. '-42')."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/sequence.rs:37–53` (`peek_sequence_entry`: requires the character after `-` to be empty, space, or tab — rejects `-` followed by non-space so that `-42` is a plain scalar, not a sequence entry)
- Test coverage: `tests/yaml-test-suite/src/W42U.yaml` (Spec Example 8.15. Block Sequence Entry Types); `tests/yaml-test-suite/src/JQ4R.yaml` (Spec Example 8.14. Block Sequence)

### [185] s-l+block-indented(n,c)

BNF: `s-l+block-indented(n,c) ::= ( s-indent(m) ( ns-l-compact-sequence(n+1+m) | ns-l-compact-mapping(n+1+m) ) ) | s-l+block-node(n,c) | ( e-node s-l-comments )`

- Classification: Conformant
- Spec (§8.2.1): "The entry node may be either completely empty, be a nested block node or use a compact in-line notation. The compact notation may be used when the entry is itself a nested block collection."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/sequence.rs:276–487` (`consume_sequence_dash` + `handle_sequence_entry` continuation: inline compact mapping/sequence dispatched via subsequent `step_in_document` calls; empty scalar emitted when no inline content and next line is not more indented)
- Test coverage: `tests/yaml-test-suite/src/W42U.yaml` (Spec Example 8.15. Block Sequence Entry Types)

### [186] ns-l-compact-sequence(n)

BNF: `ns-l-compact-sequence(n) ::= c-l-block-seq-entry(n) ( s-indent(n) c-l-block-seq-entry(n) )*`

- Classification: Conformant
- Spec (§8.2.1): "The compact notation may be used when the entry is itself a nested block collection. In this case, both the '-' indicator and the following spaces are considered to be part of the indentation of the nested collection. Note that it is not possible to specify node properties for such a collection."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/sequence.rs:129–218` (when a `-` appears as inline content after another `-`, the compact sequence opens at the column of the nested `-`)
- Test coverage: `tests/yaml-test-suite/src/W42U.yaml` (Spec Example 8.15. Block Sequence Entry Types — compact sequence case)

### [187] l+block-mapping(n)

BNF: `l+block-mapping(n) ::= ( s-indent(n+1+m) ns-l-block-map-entry(n+1+m) )+`

- Classification: Conformant
- Spec (§8.2.2): "A Block mapping is a series of entries, each presenting a key/value pair."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/mapping.rs:361–535` (`handle_mapping_entry`: opens a new `CollectionEntry::Mapping` when not already in a mapping at this indent)
- Test coverage: `tests/yaml-test-suite/src/TE2A.yaml` (Spec Example 8.16. Block Mappings); `rlsp-yaml-parser/tests/smoke/mappings.rs`

### [188] ns-l-block-map-entry(n)

BNF: `ns-l-block-map-entry(n) ::= c-l-block-map-explicit-entry(n) | ns-l-block-map-implicit-entry(n)`

- Classification: Conformant
- Spec (§8.2.2): "If the '?' indicator is specified, the optional value node must be specified on a separate line, denoted by the ':' indicator. Note that YAML allows here the same compact in-line notation described above for block sequence entries."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/mapping.rs:34–70` (`peek_mapping_entry`: recognises both explicit `?` key and implicit `key: value` forms)
- Test coverage: `tests/yaml-test-suite/src/5WE3.yaml` (Spec Example 8.17. Explicit Block Mapping Entries); `tests/yaml-test-suite/src/S3PD.yaml` (Spec Example 8.18. Implicit Block Mapping Entries)

### [189] c-l-block-map-explicit-entry(n)

BNF: `c-l-block-map-explicit-entry(n) ::= c-l-block-map-explicit-key(n) ( l-block-map-explicit-value(n) | e-node )`

- Classification: Conformant
- Spec (§8.2.2): "If the '?' indicator is specified, the optional value node must be specified on a separate line, denoted by the ':' indicator."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/mapping.rs:107–151` (explicit key branch: `?` followed by optional inline key content; absent value produces `e-node` / empty scalar)
- Test coverage: `tests/yaml-test-suite/src/5WE3.yaml` (Spec Example 8.17. Explicit Block Mapping Entries)

### [190] c-l-block-map-explicit-key(n)

BNF: `c-l-block-map-explicit-key(n) ::= c-mapping-key s-l+block-indented(n,BLOCK-OUT)`

- Classification: Conformant
- Spec (§8.2.2): "If the '?' indicator is specified, the optional value node must be specified on a separate line, denoted by the ':' indicator."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/mapping.rs:115–151` (`?` followed by whitespace or end-of-line is parsed as explicit key; inline key content is prepended as a synthetic line for `s-l+block-indented` handling)
- Test coverage: `tests/yaml-test-suite/src/5WE3.yaml` (Spec Example 8.17. Explicit Block Mapping Entries)

### [191] l-block-map-explicit-value(n)

BNF: `l-block-map-explicit-value(n) ::= s-indent(n) c-mapping-value s-l+block-indented(n,BLOCK-OUT)`

- Classification: Conformant
- Spec (§8.2.2): "If the '?' indicator is specified, the optional value node must be specified on a separate line, denoted by the ':' indicator."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/mapping.rs:789–836` (`consume_explicit_value_line`: a line that is solely a `:` value indicator advances the mapping to Value phase; inline value content is prepended as a synthetic line)
- Test coverage: `tests/yaml-test-suite/src/5WE3.yaml` (Spec Example 8.17. Explicit Block Mapping Entries)

### [192] ns-l-block-map-implicit-entry(n)

BNF: `ns-l-block-map-implicit-entry(n) ::= ( ns-s-block-map-implicit-key | e-node ) c-l-block-map-implicit-value(n)`

- Classification: Conformant
- Spec (§8.2.2): "If the '?' indicator is omitted, parsing needs to see past the implicit key, in the same way as in the single key/value pair flow mapping. Hence, such keys are subject to the same restrictions; they are limited to a single line and must not span more than 1024 Unicode characters."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/mapping.rs:161–172` (`consume_mapping_entry`: 1024-Unicode-character limit checked against `trimmed[..colon_offset]` before the key span is built; returns `ConsumedMapping::ImplicitKeyTooLongError` on violation)
- Test coverage: `rlsp-yaml-parser/tests/implicit_key_length.rs` (groups A–N and H5–H8, 48 cases)

### [193] ns-s-block-map-implicit-key

BNF: `ns-s-block-map-implicit-key ::= c-s-implicit-json-key(BLOCK-KEY) | ns-s-implicit-yaml-key(BLOCK-KEY)`

- Classification: Conformant
- Spec (§8.2.2): "Hence, such keys are subject to the same restrictions; they are limited to a single line and must not span more than 1024 Unicode characters."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/mapping.rs:161–172` (the 1024-char check precedes key extraction at `mapping.rs:214–259`; both plain YAML-key and quoted JSON-key forms are covered by the same guard)
- Test coverage: `rlsp-yaml-parser/tests/implicit_key_length.rs` (groups A–N and H5–H8, 48 cases)

### [194] c-l-block-map-implicit-value(n)

BNF: `c-l-block-map-implicit-value(n) ::= c-mapping-value ( s-l+block-node(n,BLOCK-OUT) | ( e-node s-l-comments ) )`

- Classification: Conformant
- Spec (§8.2.2): "In this case, the value may be specified on the same line as the implicit key. Note however that in block mappings the value must never be adjacent to the ':', as this greatly reduces readability and is not required for JSON compatibility (unlike the case in flow mappings). There is no compact notation for in-line values. Also, while both the implicit key and the value following it may be empty, the ':' indicator is mandatory. This prevents a potential ambiguity with multi-line plain scalars."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/mapping.rs:260–300` (value content after `: ` / `:\t` is prepended as a synthetic inline line; absent value content produces empty scalar via `e-node` path)
- Test coverage: `tests/yaml-test-suite/src/S3PD.yaml` (Spec Example 8.18. Implicit Block Mapping Entries)

### [195] ns-l-compact-mapping(n)

BNF: `ns-l-compact-mapping(n) ::= ns-l-block-map-entry(n) ( s-indent(n) ns-l-block-map-entry(n) )*`

- Classification: Conformant
- Spec (§8.2.2): "A compact in-line notation is also available. This compact notation may be nested inside block sequences and explicit block mapping entries. Note that it is not possible to specify node properties for such a nested mapping."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/mapping.rs:361–535` (compact mapping opened when a `key: value` pair appears inline after `-` or after `? ` indicator)
- Test coverage: `tests/yaml-test-suite/src/V9D5.yaml` (Spec Example 8.19. Compact Block Mappings); `tests/yaml-test-suite/src/W42U.yaml` (Spec Example 8.15. Block Sequence Entry Types — compact mapping case)

### [196] s-l+block-node(n,c)

BNF: `s-l+block-node(n,c) ::= s-l+block-in-block(n,c) | s-l+flow-in-block(n)`

- Classification: Conformant
- Spec (§8.3): "YAML allows flow nodes to be embedded inside block collections (but not vice-versa). Flow nodes must be indented by at least one more space than the parent block collection. Note that flow nodes may begin on a following line."
- Implementation: `rlsp-yaml-parser/src/event_iter/step.rs:25–` (`step_in_document` dispatches to flow or block handling based on first character of the next line: `[`, `{`, `'`, `"` → flow; `|`, `>` → block scalar; plain/mapping/sequence → block collection)
- Test coverage: `tests/yaml-test-suite/src/735Y.yaml` (Spec Example 8.20. Block Node Types)

### [197] s-l+flow-in-block(n)

BNF: `s-l+flow-in-block(n) ::= s-separate(n+1,FLOW-OUT) ns-flow-node(n+1,FLOW-OUT) s-l-comments`

- Classification: Conformant
- Spec (§8.3): "YAML allows flow nodes to be embedded inside block collections (but not vice-versa). Flow nodes must be indented by at least one more space than the parent block collection."
- Implementation: `rlsp-yaml-parser/src/event_iter/flow.rs:388–1620` (flow nodes dispatched from `step_in_document` when first character is a flow indicator; indentation relative to parent enforced by `close_collections_at_or_above`)
- Test coverage: `tests/yaml-test-suite/src/735Y.yaml` (Spec Example 8.20. Block Node Types — `"flow in block"` case)

### [198] s-l+block-in-block(n,c)

BNF: `s-l+block-in-block(n,c) ::= s-l+block-scalar(n,c) | s-l+block-collection(n,c)`

- Classification: Conformant
- Spec (§8.3): "The block node's properties may span across several lines. In this case, they must be indented by at least one more space than the block collection, regardless of the indentation of the block collection entries."
- Implementation: `rlsp-yaml-parser/src/event_iter/step.rs:25–` (dispatch: `|` / `>` first character → `try_consume_literal_block_scalar` / `try_consume_folded_block_scalar`; otherwise → block collection handling in `step_in_document`)
- Test coverage: `tests/yaml-test-suite/src/735Y.yaml` (Spec Example 8.20. Block Node Types); `tests/yaml-test-suite/src/M5C3.yaml` (Spec Example 8.21. Block Scalar Nodes)

### [199] s-l+block-scalar(n,c)

BNF: `s-l+block-scalar(n,c) ::= s-separate(n+1,c) ( c-ns-properties(n+1,c) s-separate(n+1,c) )? ( c-l+literal(n) | c-l+folded(n) )`

- Classification: Conformant
- Spec (§8.3): "The block node's properties may span across several lines. In this case, they must be indented by at least one more space than the block collection, regardless of the indentation of the block collection entries."
- Implementation: `rlsp-yaml-parser/src/event_iter/step.rs:25–` (tag/anchor properties scanned before `|`/`>` dispatch via `step_in_document` property-handling path; `rlsp-yaml-parser/src/lexer/block.rs:41–351` handles literal and folded)
- Test coverage: `tests/yaml-test-suite/src/M5C3.yaml` (Spec Example 8.21. Block Scalar Nodes); `tests/yaml-test-suite/src/Z67P.yaml` (Spec Example 8.21. Block Scalar Nodes [1.3])

### [200] s-l+block-collection(n,c)

BNF: `s-l+block-collection(n,c) ::= ( s-separate(n+1,c) c-ns-properties(n+1,c) )? s-l-comments ( seq-space(n,c) | l+block-mapping(n) )`

- Classification: Conformant
- Spec (§8.3): "Since people perceive the '-' indicator as indentation, nested block sequences may be indented by one less space to compensate, except, of course, if nested inside another block sequence (BLOCK-OUT context versus BLOCK-IN context)."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/sequence.rs:120–144` (`seq-space` rule: `CollectionEntry::Mapping(col, MappingPhase::Value, _)` allows a sequence to open at `dash_indent >= col`, implementing the one-less-indent compensation for `BLOCK-OUT` context)
- Test coverage: `tests/yaml-test-suite/src/57H4.yaml` (Spec Example 8.22. Block Collection Nodes)

### [201] seq-space(n,c)

BNF: `seq-space(n,BLOCK-OUT) ::= l+block-sequence(n-1)` / `seq-space(n,BLOCK-IN) ::= l+block-sequence(n)`

- Classification: Conformant
- Spec (§8.3): "Since people perceive the '-' indicator as indentation, nested block sequences may be indented by one less space to compensate, except, of course, if nested inside another block sequence (BLOCK-OUT context versus BLOCK-IN context)."
- Implementation: `rlsp-yaml-parser/src/event_iter/block/sequence.rs:120–144` (`seq-space` is implemented implicitly: when `MappingPhase::Value`, `dash_indent >= col` is accepted (n-1 case); when `CollectionEntry::Sequence`, only `dash_indent > parent_col` opens a new sequence (n case))
- Test coverage: `tests/yaml-test-suite/src/57H4.yaml` (Spec Example 8.22. Block Collection Nodes); `rlsp-yaml-parser/tests/smoke/sequences.rs`

## §9

### [202] l-document-prefix

BNF: `l-document-prefix ::= c-byte-order-mark? l-comment*`

- Classification: Conformant
- Spec (§9.1.1): "A document may be preceded by a prefix specifying the character encoding and optional comment lines. Note that all documents in a stream must use the same character encoding. However it is valid to re-specify the encoding using a byte order mark for each document in the stream."
- Implementation: `rlsp-yaml-parser/src/lines.rs:292–303` (`signal_document_boundary()` strips a leading BOM from the already-primed next line at document-prefix positions, implementing the optional `c-byte-order-mark?` at the start of each document); `rlsp-yaml-parser/src/lexer.rs:141` (calls `signal_document_boundary()` from `skip_blank_lines_between_docs()`); `rlsp-yaml-parser/src/event_iter/directives.rs:33–63` (`consume_preamble_between_docs` processes the `l-comment*` portion of the prefix)
- Test coverage: `rlsp-yaml-parser/tests/encoding.rs` (`parse_events_accepts_bom_immediately_after_document_end_marker`; `parse_events_accepts_bom_after_doc_end_then_blank_lines`; `parse_events_accepts_bom_after_doc_end_then_comment`; `parse_events_accepts_multiple_docs_each_with_bom`; `load_multidoc_with_bom_between_docs_produces_correct_ast`); `rlsp-yaml-parser/src/lines.rs` (`bom_stripped_after_document_boundary_signal`; `signal_document_boundary_strips_bom_from_primed_next_line`)

### [203] c-directives-end

BNF: `c-directives-end ::= "---"`

- Classification: Conformant
- Spec (§9.1.2): "The solution is the use of two special marker lines to control the processing of directives, one at the start of a document and one at the end. At the start of a document, lines beginning with a \"%\" character are assumed to be directives. The (possibly empty) list of directives is terminated by a directives end marker line."
- Implementation: `rlsp-yaml-parser/src/event_iter/step.rs:137–178` — `lexer.is_directives_end()` detects `---` at column 0; `lexer.consume_marker_line(false)` consumes the line and captures any inline scalar
- Test coverage: `tests/yaml-test-suite/src/FTA2.yaml` (Single block sequence with anchor and explicit document start); `tests/yaml-test-suite/src/2LFX.yaml` (directive + `---` marker); `rlsp-yaml-parser/tests/smoke/directives.rs:459–471` (explicit_document_start_span_covers_dashes)

### [204] c-document-end

BNF: `c-document-end ::= "..."    # (not followed by non-ws char)`

- Classification: Conformant
- Spec (§9.1.2): "At the end of a document, a document end marker line is used to signal the parser to begin scanning for directives again. The existence of this optional document suffix does not necessarily indicate the existence of an actual following document."
- Implementation: `rlsp-yaml-parser/src/event_iter/step.rs:118–136` — `lexer.is_document_end()` detects `...` at column 0 (not followed by non-ws content enforced by `consume_marker_line(true)` which sets `marker_inline_error` for inline content after `...`)
- Test coverage: `tests/yaml-test-suite/src/3HFZ.yaml` (error: bad footer); `rlsp-yaml-parser/tests/smoke/directives.rs:706–717` (yaml_directive_followed_by_document_end_returns_error)

### [205] l-document-suffix

BNF: `l-document-suffix ::= c-document-end s-l-comments`

- Classification: Conformant
- Spec (§9.1.2): "At the end of a document, a document end marker line is used to signal the parser to begin scanning for directives again. The existence of this optional document suffix does not necessarily indicate the existence of an actual following document."
- Implementation: `rlsp-yaml-parser/src/event_iter/step.rs:118–136` — `...` marker detected and consumed via `consume_marker_line(true)`; trailing comments after `...` drained by `drain_trailing_comment()` before emitting `DocumentEnd { explicit: true }`
- Test coverage: `tests/yaml-test-suite/src/2LFX.yaml` (document with suffix); `rlsp-yaml-parser/tests/smoke/directives.rs` (multi-doc stream tests)

### [206] c-forbidden

BNF: `c-forbidden ::= <start-of-line> ( c-directives-end | c-document-end ) ( b-char | s-white | <end-of-input> )`

- Classification: Conformant
- Spec (§9.1.2): "Obviously, the actual content lines are therefore forbidden to begin with either of these markers."
- Implementation: `rlsp-yaml-parser/src/event_iter/step.rs:110–111` — comment: "Document markers (`---`/`...`) must be at column 0 (YAML 1.2 §9.1). Any line with indent > 0 cannot be a marker — skip the function call." `step.rs:118` and `step.rs:137` — both marker checks require `peeked_indent == 0`. `rlsp-yaml-parser/src/lexer.rs:is_directives_end` and `is_document_end` check for column-0 `---`/`...` not followed by non-ws content.
- Test coverage: `tests/yaml-test-suite/src/N782.yaml` (Invalid document markers in flow style — `---` inside flow collection is not at start-of-line); `rlsp-yaml-parser/tests/smoke/directives.rs`

### [207] l-bare-document

BNF: `l-bare-document ::= s-l+block-node(-1,BLOCK-IN)  /* Excluding c-forbidden content */`

- Classification: Conformant
- Spec (§9.1.3): "A bare document does not begin with any directives or marker lines. Such documents are very \"clean\" as they contain nothing other than the content. In this case, the first non-comment line may not start with a \"%\" first character."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:338–355` — in `step_between_docs`, when no directives were accumulated and the next token is not `---` or `...`, a `DocumentStart { explicit: false }` event is emitted and state transitions to `InDocument`; `step_in_document` then parses the block node content
- Test coverage: `tests/yaml-test-suite/src/9KBC.yaml` (bare document, error); `rlsp-yaml-parser/tests/smoke/directives.rs:395–407` (bare_document_sets_explicit_false)

### [208] l-explicit-document

BNF: `l-explicit-document ::= c-directives-end ( l-bare-document | ( e-node s-l-comments ) )`

- Classification: Conformant
- Spec (§9.1.4): "An explicit document begins with an explicit directives end marker line but no directives. Since the existence of the document is indicated by this marker, the document itself may be completely empty."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:287–307` — `step_between_docs`: when `lexer.is_directives_end()`, emits `DocumentStart { explicit: true, version, tag_directives }` and transitions to `InDocument`; the empty-document case (`e-node`) is handled when `step_in_document` immediately hits EOF or `...`
- Test coverage: `tests/yaml-test-suite/src/FTA2.yaml` (explicit document start); `rlsp-yaml-parser/tests/smoke/directives.rs:383–393` (explicit_document_marker_sets_explicit_true); `rlsp-yaml-parser/src/loader.rs` (UT-D2, UT-D6)

### [209] l-directive-document

BNF: `l-directive-document ::= l-directive+ l-explicit-document`

- Classification: Conformant
- Spec (§9.1.5): "A directives document begins with some directives followed by an explicit directives end marker line."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:33–63` — `consume_preamble_between_docs` accumulates `%YAML`/`%TAG` directives into `self.directive_scope`; `directives.rs:272–337` — if directives were accumulated but no `---` follows (EOF, `...`, or bare content), an error is returned ("directives must be followed by a '---' document-start marker"); when `---` does follow, `DocumentStart` includes the accumulated `version` and `tag_directives`
- Test coverage: `tests/yaml-test-suite/src/B63P.yaml` (directive without document — error); `tests/yaml-test-suite/src/2LFX.yaml` (directive + document); `rlsp-yaml-parser/tests/smoke/directives.rs:289–337` (directive scope per-document)

### [210] l-any-document

BNF: `l-any-document ::= l-directive-document | l-explicit-document | l-bare-document`

- Classification: Conformant
- Spec (§9.2): "A YAML stream consists of zero or more documents. Subsequent documents require some sort of separation marker line. If a document is not terminated by a document end marker line, then the following document must begin with a directives end marker line."
- Implementation: `rlsp-yaml-parser/src/event_iter/directives.rs:259–355` — `step_between_docs` implements the three-way dispatch: if directives were seen, require `---` (directive document); if `---` is next without directives, emit explicit document; otherwise emit bare document start. The ordering of checks in `step_between_docs` implements the priority of `l-directive-document` > `l-explicit-document` > `l-bare-document`
- Test coverage: `rlsp-yaml-parser/tests/smoke/directives.rs:342–403` (multi-doc stream, bare, explicit document variants)

### [211] l-yaml-stream

BNF: `l-yaml-stream ::= l-document-prefix* l-any-document? ( ( l-document-suffix+ l-document-prefix* l-any-document? ) | c-byte-order-mark | l-comment | l-explicit-document )*`

- Classification: Conformant
- Spec (§9.2): "A YAML stream consists of zero or more documents. Subsequent documents require some sort of separation marker line. If a document is not terminated by a document end marker line, then the following document must begin with a directives end marker line. […] A sequence of bytes is a well-formed stream if, taken as a whole, it complies with the above l-yaml-stream production."
- Implementation: `rlsp-yaml-parser/src/event_iter/base.rs:491–514` — state machine: `BeforeStream` → emits `StreamStart`; `BetweenDocs` → `step_between_docs`; `InDocument` → `step_in_document`; `Done` → emits nothing. The stream structure (prefix, documents, suffixes, inter-doc comments) is correctly dispatched. The `c-byte-order-mark` alternative in the outer `(...)*` loop of `l-yaml-stream` is handled via `signal_document_boundary()` in `lines.rs:292–303`, called from `skip_blank_lines_between_docs()` in `lexer.rs:141` — a BOM at a document-prefix position is stripped before the stream parser sees it.
- Test coverage: `rlsp-yaml-parser/tests/smoke/stream.rs` (full stream lifecycle: empty, whitespace, multi-doc, comment-only); `rlsp-yaml-parser/tests/conformance/stream.rs` (yaml-test-suite parameterized suite); `rlsp-yaml-parser/tests/encoding.rs` (`parse_events_accepts_bom_immediately_after_document_end_marker`; `parse_events_accepts_multiple_docs_each_with_bom`; `parse_events_bom_after_directives_end_marker_is_error`)

## §10

### Failsafe Schema — tag resolution for `!` non-specific tag

- Classification: Conformant
- Spec (§10.1.2): "All [nodes] with the "`!`" non-specific tag are [resolved], by the standard [convention], to "`tag:yaml.org,2002:seq`", "`tag:yaml.org,2002:map`" or "`tag:yaml.org,2002:str`", according to their [kind]."
- Implementation: Schema resolution is provided by `rlsp-yaml-parser/src/schema.rs` via `resolve_scalar` and `resolve_collection`. When `Schema::Failsafe` is active, `!`-tagged scalars resolve to `tag:yaml.org,2002:str`, `!`-tagged sequences resolve to `tag:yaml.org,2002:seq`, and `!`-tagged mappings resolve to `tag:yaml.org,2002:map`. The schema is applied in the loader (`rlsp-yaml-parser/src/loader.rs:280` — `Loader::load`) when a schema is configured via `LoaderBuilder::schema(Schema::Failsafe)` or `load_with_schema(input, Schema::Failsafe)`. All three schema paths (`Schema::Failsafe`, `Schema::Json`, `Schema::Core`) resolve `!` non-specific tags.
- Test coverage: `rlsp-yaml-parser/tests/schema_resolution.rs` — `failsafe_bare_excl_on_scalar_resolves_to_str`, `failsafe_bare_excl_on_sequence_resolves_to_seq`, `core_bare_excl_on_plain_scalar_resolves_to_str`, `core_bare_excl_on_bool_value_resolves_to_str`, `core_bare_excl_on_sequence_resolves_to_seq`, `core_bare_excl_on_mapping_resolves_to_map`.

### Failsafe Schema — `?` non-specific tag left unresolved

- Classification: Conformant
- Spec (§10.1.2): "All [nodes] with the "`?`" non-specific tag are left [unresolved]. This constrains the [application] to deal with a [partial representation]."
- Implementation: Nodes with no explicit tag in the source have `tag: None` in the event stream and AST. The parser never synthesises a `?` tag token nor resolves it. Untagged scalars, sequences, and mappings all yield `tag: None`. For example, an untagged scalar `hello` produces `Event::Scalar { tag: None, .. }` and the loader stores `Node::Scalar { tag: None, .. }`. Yielding `tag: None` is the parser's representation of an unresolved node — the application deals with a partial representation, as the spec requires.
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs` (lines 557, 638, 651, 678, 742) — confirms `tag: None` for untagged mappings and scalars. `rlsp-yaml-parser/tests/smoke/tags.rs:1169–1195` (TL-9/TL-11) — verifies `tag_loc: None` for untagged nodes of all kinds.

### Failsafe Schema — `!!map` (tag:yaml.org,2002:map)

- Classification: Conformant
- Spec (§10.1.1): "URI: `tag:yaml.org,2002:map`. Kind: [Mapping]. Definition: [Represents] an associative container, where each [key] is unique in the association and mapped to exactly one [value]. YAML places no restrictions on the type of [keys]; in particular, they are not restricted to being [scalars]."
- Implementation: The parser recognises the `!!map` shorthand via `DirectiveScope::resolve_tag` in `rlsp-yaml-parser/src/event_iter/directive_scope.rs:93–109`, expanding it to `"tag:yaml.org,2002:map"` using the default `!!` prefix `"tag:yaml.org,2002:"`. The tag is stored on `MappingStart` events and on `Node::Mapping { tag: Some("tag:yaml.org,2002:map"), .. }` in the AST.
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs` (`resolve_tag_double_bang_uses_default_yaml_prefix` via `!!str`; same expansion applies to `!!map`). `rlsp-yaml-parser/src/event_iter/directive_scope.rs:208–212` (unit test confirming `!!str` → `tag:yaml.org,2002:str`; the same code path handles `!!map`).

### Failsafe Schema — `!!seq` (tag:yaml.org,2002:seq)

- Classification: Conformant
- Spec (§10.1.1): "URI: `tag:yaml.org,2002:seq`. Kind: [Sequence]. Definition: [Represents] a collection indexed by sequential integers starting with zero."
- Implementation: The `!!seq` shorthand is expanded by `DirectiveScope::resolve_tag` (`rlsp-yaml-parser/src/event_iter/directive_scope.rs:93–109`) to `"tag:yaml.org,2002:seq"`. The tag is stored on `SequenceStart` events and on `Node::Sequence { tag: Some("tag:yaml.org,2002:seq"), .. }` in the AST. Untagged sequences have `tag: None`.
- Test coverage: `rlsp-yaml-parser/src/event_iter/directive_scope.rs:208–212` (unit test for `!!`-prefix expansion; applies equally to `!!seq`).

### Failsafe Schema — `!!str` (tag:yaml.org,2002:str)

- Classification: Conformant
- Spec (§10.1.1): "URI: `tag:yaml.org,2002:str`. Kind: [Scalar]. Definition: [Represents] a Unicode string, a sequence of zero or more Unicode characters. Canonical Form: The obvious."
- Implementation: The `!!str` shorthand is expanded by `DirectiveScope::resolve_tag` (`rlsp-yaml-parser/src/event_iter/directive_scope.rs:93–109`) to `"tag:yaml.org,2002:str"`. The tag is stored on `Scalar` events and on `Node::Scalar { tag: Some("tag:yaml.org,2002:str"), .. }` in the AST. Unquoted and quoted scalars without an explicit tag have `tag: None`.
- Test coverage: `rlsp-yaml-parser/src/event_iter/directive_scope.rs:208–212` (`resolve_tag_double_bang_uses_default_yaml_prefix` — confirms `!!str` → `"tag:yaml.org,2002:str"`). `rlsp-yaml-parser/tests/smoke/tags.rs` (groups A–D exercise shorthand tag scanning and resolve path).

### JSON Schema — tag resolution for plain scalars

- Classification: Conformant
- Spec (§10.2.2): "[Scalars] with the "`?`" non-specific tag (that is, [plain scalars]) are matched with a list of regular expressions (first match wins, e.g. `0` is resolved as `!!int`). In principle, JSON files should not contain any [scalars] that do not match at least one of these. Hence the YAML [processor] should consider them to be an error. | Regular expression | Resolved to tag | | `null` | tag:yaml.org,2002:null | | `true \| false` | tag:yaml.org,2002:bool | | `-? ( 0 \| [1-9] [0-9]* )` | tag:yaml.org,2002:int | | `-? ( 0 \| [1-9] [0-9]* ) ( \\. [0-9]* )? ( [eE] [-+]? [0-9]+ )?` | tag:yaml.org,2002:float | | `*` | Error |"
- Implementation: `rlsp-yaml-parser/src/schema.rs` (`resolve_scalar`) — when `Schema::Json` is active, untagged plain scalars are matched against the four JSON regex patterns. Matching scalars resolve to the appropriate tag URI. Non-matching plain scalars produce `LoadError::UnresolvedScalar`. Non-plain scalars (quoted, literal, folded) always resolve to `tag:yaml.org,2002:str`. The schema is applied in the loader via `LoaderBuilder::schema(Schema::Json)` or `load_with_schema(input, Schema::Json)`.
- Test coverage: `rlsp-yaml-parser/tests/schema_resolution.rs` — `json_plain_int_resolves_to_int`, `json_plain_bool_resolves_to_bool`, `json_plain_null_resolves_to_null`, `json_plain_float_resolves_to_float`, `json_plain_false_resolves_to_bool`, `json_plain_zero_resolves_to_int`, `json_plain_string_returns_unresolved_scalar_error`, `json_octal_plain_returns_unresolved_scalar_error`, `json_plus_prefix_int_returns_unresolved_scalar_error`, `json_tilde_returns_unresolved_scalar_error`, `json_uppercase_bool_returns_unresolved_scalar_error`, `json_inf_notation_returns_unresolved_scalar_error`, `json_nan_notation_returns_unresolved_scalar_error`, `json_double_quoted_string_resolves_to_str`, `json_empty_document_returns_unresolved_scalar_error`, `json_unresolved_scalar_propagates_from_nested_sequence_item`, `json_unresolved_scalar_propagates_from_nested_mapping_value`, `json_unresolved_scalar_display_message_is_exact`, `json_unresolved_scalar_pos_reflects_actual_position`, `json_unresolved_scalar_value_field_contains_scalar_content`, `json_unresolved_scalar_truncates_long_value`.

### JSON Schema — tag resolution for untagged collections

- Classification: Conformant
- Spec (§10.2.2): "[Collections] with the "`?`" non-specific tag (that is, [untagged] [collections]) are [resolved] to "`tag:yaml.org,2002:seq`" or "`tag:yaml.org,2002:map`" according to their [kind]."
- Implementation: `rlsp-yaml-parser/src/schema.rs` (`resolve_collection`) — when `Schema::Json` is active, untagged sequences resolve to `tag:yaml.org,2002:seq` and untagged mappings resolve to `tag:yaml.org,2002:map`. The resolution is applied in the loader for both `SequenceStart` and `MappingStart` events.
- Test coverage: `rlsp-yaml-parser/tests/schema_resolution.rs` — `json_untagged_sequence_resolves_to_seq`, `json_untagged_mapping_resolves_to_map`.

### Core Schema — tag resolution for plain scalars

- Classification: Conformant
- Spec (§10.3.2): "[Scalars] with the "`?`" non-specific tag (that is, [plain scalars]) are matched with an extended list of regular expressions. However, in this case, if none of the regular expressions matches, the [scalar] is [resolved] to `tag:yaml.org,2002:str` (that is, considered to be a string). | Regular expression | Resolved to tag | | `null \| Null \| NULL \| ~` | tag:yaml.org,2002:null | | `/* Empty */` | tag:yaml.org,2002:null | | `true \| True \| TRUE \| false \| False \| FALSE` | tag:yaml.org,2002:bool | | `[-+]? [0-9]+` | tag:yaml.org,2002:int (Base 10) | | `0o [0-7]+` | tag:yaml.org,2002:int (Base 8) | | `0x [0-9a-fA-F]+` | tag:yaml.org,2002:int (Base 16) | | `[-+]? ( \\. [0-9]+ \| [0-9]+ ( \\. [0-9]* )? ) ( [eE] [-+]? [0-9]+ )?` | tag:yaml.org,2002:float (Number) | | `[-+]? ( \\.inf \| \\.Inf \| \\.INF )` | tag:yaml.org,2002:float (Infinity) | | `\\.nan \| \\.NaN \| \\.NAN` | tag:yaml.org,2002:float (Not a number) | | `*` | tag:yaml.org,2002:str (Default) |"
- Implementation: `rlsp-yaml-parser/src/schema.rs` (`resolve_scalar`) — when `Schema::Core` is active, untagged plain scalars are matched against the extended regex table covering null, bool, int (decimal, octal, hex), float (number, infinity, NaN), with `tag:yaml.org,2002:str` as the fallback for any unmatched value. Non-plain scalars (quoted, literal, folded) always resolve to `tag:yaml.org,2002:str`. The schema is applied in the loader via `LoaderBuilder::schema(Schema::Core)` or `load_with_schema(input, Schema::Core)`.
- Test coverage: `rlsp-yaml-parser/tests/schema_resolution.rs` — `core_plain_integer_resolves_to_int`, `core_plain_bool_resolves_to_bool`, `core_plain_string_resolves_to_str`, `core_plain_float_resolves_to_float`, `core_plain_null_lowercase_resolves_to_null`, `core_plain_tilde_resolves_to_null`, `core_plain_empty_resolves_to_null`, `core_double_quoted_integer_resolves_to_str`, `core_block_literal_resolves_to_str`, `core_explicit_str_tag_on_integer_value_preserved`, `core_explicit_int_tag_on_quoted_string_preserved`, `core_octal_resolves_to_int`, `core_hex_resolves_to_int`, `core_inf_resolves_to_float`, `core_nan_resolves_to_float`, `core_positive_signed_int_resolves_to_int`, `core_negative_zero_resolves_to_int`.

### Core Schema — tag resolution for untagged collections

- Classification: Conformant
- Spec (§10.3.2): "[Collections] with the "`?`" non-specific tag (that is, [untagged] [collections]) are [resolved] to "`tag:yaml.org,2002:seq`" or "`tag:yaml.org,2002:map`" according to their [kind]." (Same rule as JSON schema, inherited by Core schema extension.)
- Implementation: `rlsp-yaml-parser/src/schema.rs` (`resolve_collection`) — when `Schema::Core` is active, untagged sequences resolve to `tag:yaml.org,2002:seq` and untagged mappings resolve to `tag:yaml.org,2002:map`. Same resolution path as JSON schema; the Core schema inherits collection resolution from JSON.
- Test coverage: `rlsp-yaml-parser/tests/schema_resolution.rs` — `core_untagged_mapping_resolves_to_map`, `core_untagged_sequence_resolves_to_seq`, `core_nested_structure_all_tags_resolved`.

### Other Schemas

- Classification: Not Applicable (descriptive)
- Spec (§10.4): "None of the above recommended [schemas] preclude the use of arbitrary explicit [tags]. Hence YAML [processors] for a particular programming language typically provide some form of [local tags] that map directly to the language's [native data structures] (e.g., `!ruby/object:Set`). […] It is strongly recommended that such [schemas] be based on the [core schema] defined above."
- Implementation: The parser supports arbitrary explicit tags syntactically. Local tags (`!suffix` with no registered primary handle) pass through as-is via `DirectiveScope::resolve_tag` (`rlsp-yaml-parser/src/event_iter/directive_scope.rs:153–154`). Named handles (`!handle!suffix`) and secondary handles (`!!suffix`) expand to their registered or default prefixes. No language-native schema (Ruby, Python, etc.) is implemented — the parser is language-agnostic at the schema level.
- Test coverage: `rlsp-yaml-parser/tests/smoke/tags.rs` (groups B and C: local tags `!local`, named handles `!yaml!str`). `rlsp-yaml-parser/src/event_iter/directive_scope.rs:287–299` (`resolve_tag_local_tag_returns_as_is`, `resolve_tag_bare_bang_returns_as_is`).

## Summary

0 Lenient findings, 0 Strict findings (bug-class), 3 Strict (security-hardened) findings, total 3 entries.

| Spec production | Classification | Source file:line | Discrepancy (one sentence) | Test coverage |
|---|---|---|---|---|
| §5 [59] `ns-esc-8-bit` | Strict (security-hardened) | `rlsp-yaml-parser/src/lexer/quoted.rs:594–618` | The implementation rejects hex escapes whose decoded character falls outside `c-printable` and additionally rejects hex escapes in the bidi-override range; named escapes are exempt by design. | `tests/yaml-test-suite/src/G4RS.yaml` |
| §5 [60] `ns-esc-16-bit` | Strict (security-hardened) | `rlsp-yaml-parser/src/lexer/quoted.rs:594–618` | The implementation rejects hex escapes whose decoded character falls outside `c-printable` and additionally rejects hex escapes in the bidi-override range; named escapes are exempt by design. | `tests/yaml-test-suite/src/G4RS.yaml` |
| §5 [61] `ns-esc-32-bit` | Strict (security-hardened) | `rlsp-yaml-parser/src/lexer/quoted.rs:594–618` | The implementation rejects hex escapes whose decoded character falls outside `c-printable` and additionally rejects hex escapes in the bidi-override range; named escapes are exempt by design. | `rlsp-yaml-parser/src/chars.rs:393–394` |
