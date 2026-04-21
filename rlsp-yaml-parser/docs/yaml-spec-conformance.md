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

- Classification: Conformant | Lenient | Strict | Not Implemented | Not Applicable (descriptive) | Not Applicable (meta-notation)
- Spec (§X.Y): "<verbatim quote of the normative text>"
- Implementation: <crate>/<path>:<line-range>
- Test coverage: <yaml-test-suite case ID(s)> | <project test path> | no direct test
- Discrepancy: <one-sentence gap — Lenient/Strict only; omit for other classes>
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
| requires X | does not implement X | **Not Implemented** |
| entry has no normative obligation on the implementation (purely descriptive) | — | **Not Applicable (descriptive)** |
| entry is meta-notation for the grammar itself | — | **Not Applicable (meta-notation)** |

The classification is the output of applying these rules to the spec quote and the
implementation fact recorded in the entry. A classification that does not follow from
the recorded evidence is a reviewer-rejectable defect.

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
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–500` (double-quoted scanner accepts tab and all non-C0 characters inside quoted scalars; `is_c_printable` gating applies to escape-decoded characters only, not to literal stream characters)
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (spec example 2.17 exercising tab and Unicode inside double-quoted scalars)

### [3] c-byte-order-mark

BNF: `c-byte-order-mark ::= xFEFF`

- Classification: Strict
- Spec (§5.2): "If a character stream begins with a byte order mark, the character encoding will be taken to be as indicated by the byte order mark. Otherwise, the stream must begin with an ASCII character. […] Byte order marks may appear at the start of any document, however all documents in the same stream must use the same character encoding. To allow for JSON compatibility, byte order marks are also allowed inside quoted scalars."
- Implementation: `rlsp-yaml-parser/src/lines.rs:115–127` (BOM stripped from first line only — `is_first == true` guard); `rlsp-yaml-parser/src/encoding.rs:88–96` (`decode` handles BOM at byte-stream level, before parsing); `rlsp-yaml-parser/src/lexer/plain.rs:103–106` (mid-stream BOM in a plain-scalar suffix is treated as an error)
- Test coverage: `rlsp-yaml-parser/tests/encoding.rs` (`decode_bom_stripping` cases `utf8_bom`, `utf16_le_bom`; `parse_events_accepts_bom_at_stream_start`; `parse_events_rejects_bom_mid_stream`)
- Discrepancy: The spec permits a BOM at the start of any document within a multi-document stream, but the implementation only strips the BOM on the first line of input (`is_first == true`); a BOM at the start of the second or subsequent document is treated as an invalid character.

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
- Implementation: `rlsp-yaml-parser/src/lexer/quoted.rs:165–500` (`try_consume_double_quoted`)
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
- Implementation: `rlsp-yaml-parser/src/event_iter/properties.rs:290` — tag handle validation uses `.is_ascii_alphanumeric() || c == '-' || c == '_'`; `rlsp-yaml-parser/src/chars.rs:89–113` (`is_ns_uri_char_single` includes alphanumeric and `-`)
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

- Classification: Strict
- Spec (§5.7): "Escaped 8-bit Unicode character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:194` (`decode_escape` — `'x' => decode_hex_escape(input, 1, 2)`); `rlsp-yaml-parser/src/lexer/quoted.rs:596–605` — if the decoded character is not `c-printable`, the escape is rejected with an error
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (`\x0d\x0a` in hex-esc string); `rlsp-yaml-parser/src/chars.rs:391` (unit test `hex_2digit`)
- Discrepancy: The spec defines `\xHH` as producing any 8-bit Unicode codepoint, but the implementation rejects `\xHH` forms whose decoded character falls outside `c-printable` (e.g. `\x01`), even though the spec lists `ns-esc-null` (`\0`) as a valid named escape that produces the same non-printable U+0000.

### [60] ns-esc-16-bit

BNF: `ns-esc-16-bit ::= 'u' ns-hex-digit{4}`

- Classification: Strict
- Spec (§5.7): "Escaped 16-bit Unicode character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:195` (`decode_escape` — `'u' => decode_hex_escape(input, 1, 4)`); same non-printable rejection applies via `rlsp-yaml-parser/src/lexer/quoted.rs:596–605`
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml` (`☺`); `rlsp-yaml-parser/src/chars.rs:392` (unit test `hex_4digit`)
- Discrepancy: The spec defines `\uHHHH` as producing any 16-bit Unicode codepoint, but the implementation rejects `\uHHHH` forms whose decoded character is not `c-printable`.

### [61] ns-esc-32-bit

BNF: `ns-esc-32-bit ::= 'U' ns-hex-digit{8}`

- Classification: Strict
- Spec (§5.7): "Escaped 32-bit Unicode character."
- Implementation: `rlsp-yaml-parser/src/chars.rs:196` (`decode_escape` — `'U' => decode_hex_escape(input, 1, 8)`); same non-printable rejection applies via `rlsp-yaml-parser/src/lexer/quoted.rs:596–605`
- Test coverage: `rlsp-yaml-parser/src/chars.rs:393` (unit test `hex_8digit`); `rlsp-yaml-parser/src/chars.rs:394` (unit test `high_plane_codepoint`)
- Discrepancy: The spec defines `\UHHHHHHHH` as producing any 32-bit Unicode codepoint, but the implementation rejects `\UHHHHHHHH` forms whose decoded character is not `c-printable`.

### [62] c-ns-esc-char

BNF: `c-ns-esc-char ::= c-escape ( ns-esc-null | ns-esc-bell | ns-esc-backspace | ns-esc-horizontal-tab | ns-esc-line-feed | ns-esc-vertical-tab | ns-esc-form-feed | ns-esc-carriage-return | ns-esc-escape | ns-esc-space | ns-esc-double-quote | ns-esc-slash | ns-esc-backslash | ns-esc-next-line | ns-esc-non-breaking-space | ns-esc-line-separator | ns-esc-paragraph-separator | ns-esc-8-bit | ns-esc-16-bit | ns-esc-32-bit )`

- Classification: Conformant
- Spec (§5.7): "Note that escape sequences are only interpreted in double-quoted scalars. In all other scalar styles, the \"\\\" character has no special meaning and non-printable characters are not available."
- Implementation: `rlsp-yaml-parser/src/chars.rs:173–199` (`decode_escape`); invoked exclusively from the double-quoted scanner in `rlsp-yaml-parser/src/lexer/quoted.rs:575–620`
- Test coverage: `tests/yaml-test-suite/src/G4RS.yaml`; `tests/yaml-test-suite/src/55WF.yaml` (invalid escape code rejected); `rlsp-yaml-parser/src/chars.rs:382–410` (comprehensive unit tests)

## §6

<!-- Task 4: draft §6 entries -->

## §7

<!-- Task 6: draft §7 entries -->

## §8

<!-- Task 8: draft §8 entries -->

## §9

<!-- Task 10: draft §9 entries -->

## §10

<!-- Task 12: draft §10 entries -->

## Summary

<!-- Task 13: append consolidated Summary table of all Lenient and Strict findings -->
