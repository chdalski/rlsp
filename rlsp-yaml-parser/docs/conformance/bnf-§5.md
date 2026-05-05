# BNF Conformance — §3, §4, §5 Character Productions

Source: `.ai/audit/2026-04-30-phase1-bnf/reconciliation-§5.md` (64 entries)

**Verdict tally (post-fix):** Strict-conformant: 59, Stricter-than-spec: 3, Not-applicable: 2

---

## §3 and §4 — Not Applicable

### [§3] Processes and Models

BNF: (none — §3 contains no numbered BNF productions)

- **Verdict:** Not-applicable
- **Spec (§3):** "YAML is both a text format and a method for presenting any native data structure in this format."
- **Implementation:** (no implementation obligation)
- **Tests:** (no implementation obligation)

### [§4] Syntax Conventions

BNF: (none — §4 contains no numbered BNF productions)

- **Verdict:** Not-applicable
- **Spec (§4):** "The following chapters formally define the syntax of YAML character streams, using parameterized BNF productions."
- **Implementation:** (no implementation obligation)
- **Tests:** (no implementation obligation)

---

## §5 — Character Productions

### [1] c-printable

BNF: `c-printable ::= x09 | x0A | x0D | [x20-x7E] | x85 | [xA0-xD7FF] | [xE000-xFFFD] | [x010000-x10FFFF]`

- **Verdict:** Strict-conformant
- **Spec (§5.1):** "On input, a YAML processor must accept all characters in this printable subset."
- **Implementation:** `is_c_printable()` in `chars.rs`; `reject_non_printable()` in `lexer.rs` enforces the predicate on literal stream input
- **Tests:** `chars.rs` unit tests `c_printable_accepts`, `c_printable_rejects`; `rlsp-yaml-parser/tests/conformance/non_printable.rs`
- **Note:** Previously Lenient — the predicate existed but was not enforced on literal stream characters (only on escape-decoded characters). Fixed in commit `666e2f2` (`fix(rlsp-yaml-parser): reject non-c-printable characters in literal stream input`).

### [2] nb-json

BNF: `nb-json ::= x09 | [x20-x10FFFF]`

- **Verdict:** Strict-conformant
- **Spec (§5.1):** "To ensure JSON compatibility, YAML processors must allow all non-C0 characters inside quoted scalars."
- **Implementation:** `try_consume_double_quoted()` in `lexer/quoted.rs` — `is_c_printable` gating applies only to escape-decoded characters; literal non-C0 characters inside quoted scalars pass through unmodified
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml` (tab and Unicode inside double-quoted scalars)

### [3] c-byte-order-mark

BNF: `c-byte-order-mark ::= xFEFF`

- **Verdict:** Strict-conformant
- **Spec (§5.2):** "Byte order marks may appear at the start of any document, however all documents in the same stream must use the same character encoding."
- **Implementation:** `decode()` in `encoding.rs` (BOM at byte-stream level); `signal_document_boundary()` in `lines.rs` (strips leading BOM at document-prefix positions); `step_in_document()` in `event_iter/step.rs` (BOM mid-document rejected)
- **Tests:** `rlsp-yaml-parser/tests/encoding.rs` (multiple BOM-related test cases); `parse_events_rejects_double_bom_at_document_prefix`

### [4] c-sequence-entry

BNF: `c-sequence-entry ::= '-'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`-` (x2D, hyphen) denotes a block sequence entry."
- **Implementation:** `peek_sequence_entry()` in `event_iter/block/sequence.rs`
- **Tests:** `tests/yaml-test-suite/src/229Q.yaml`

### [5] c-mapping-key

BNF: `c-mapping-key ::= '?'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`?` (x3F, question mark) denotes a mapping key."
- **Implementation:** `peek_mapping_entry()` in `event_iter/block/mapping.rs`
- **Tests:** `tests/yaml-test-suite/src/229Q.yaml`

### [6] c-mapping-value

BNF: `c-mapping-value ::= ':'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`:` (x3A, colon) denotes a mapping value."
- **Implementation:** `find_value_indicator_offset()` in `event_iter/line_mapping.rs`
- **Tests:** `tests/yaml-test-suite/src/229Q.yaml`

### [7] c-collect-entry

BNF: `c-collect-entry ::= ','`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`,` (x2C, comma) ends a flow collection entry."
- **Implementation:** `,` branch in the flow scanner loop in `event_iter/flow.rs`
- **Tests:** `tests/yaml-test-suite/src/4ABK.yaml`

### [8] c-sequence-start

BNF: `c-sequence-start ::= '['`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`[` (x5B, left bracket) starts a flow sequence."
- **Implementation:** `is_c_flow_indicator()` in `chars.rs`; `[` branch pushes `FlowFrame::Sequence` in `event_iter/flow.rs`
- **Tests:** `tests/yaml-test-suite/src/4ABK.yaml`

### [9] c-sequence-end

BNF: `c-sequence-end ::= ']'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`]` (x5D, right bracket) ends a flow sequence."
- **Implementation:** `is_c_flow_indicator()` in `chars.rs`; `]` branch pops `FlowFrame::Sequence` in `event_iter/flow.rs`
- **Tests:** `tests/yaml-test-suite/src/4ABK.yaml`

### [10] c-mapping-start

BNF: `c-mapping-start ::= '{'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`{` (x7B, left brace) starts a flow mapping."
- **Implementation:** `is_c_flow_indicator()` in `chars.rs`; `{` branch pushes `FlowFrame::Mapping` in `event_iter/flow.rs`
- **Tests:** `tests/yaml-test-suite/src/4ABK.yaml`

### [11] c-mapping-end

BNF: `c-mapping-end ::= '}'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`}` (x7D, right brace) ends a flow mapping."
- **Implementation:** `is_c_flow_indicator()` in `chars.rs`; `}` branch pops `FlowFrame::Mapping` in `event_iter/flow.rs`
- **Tests:** `tests/yaml-test-suite/src/4ABK.yaml`

### [12] c-comment

BNF: `c-comment ::= '#'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`#` (x23, octothorpe) denotes a comment."
- **Implementation:** `#` triggers comment lexing in `lexer/comment.rs`
- **Tests:** `rlsp-yaml-parser/tests/smoke/comments.rs`

### [13] c-anchor

BNF: `c-anchor ::= '&'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`&` (x26, ampersand) denotes a node's anchor property."
- **Implementation:** `scan_anchor_name()` in `event_iter/properties.rs` (invoked after `&` indicator)
- **Tests:** `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`

### [14] c-alias

BNF: `c-alias ::= '*'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`*` (x2A, asterisk) denotes an alias node."
- **Implementation:** `scan_anchor_name()` in `event_iter/properties.rs` (also used after `*` for alias scanning)
- **Tests:** `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`

### [15] c-tag

BNF: `c-tag ::= '!'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "The `!` (x21, exclamation) is used for specifying node tags."
- **Implementation:** `scan_tag()` in `event_iter/properties.rs` (handles all `!`-introduced tag forms)
- **Tests:** `rlsp-yaml-parser/tests/smoke/tags.rs`

### [16] c-literal

BNF: `c-literal ::= '|'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`|` (7C, vertical bar) denotes a literal block scalar."
- **Implementation:** `try_consume_literal_block_scalar()` in `lexer/block.rs`
- **Tests:** `tests/yaml-test-suite/src/A2M4.yaml`

### [17] c-folded

BNF: `c-folded ::= '>'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`>` (x3E, greater than) denotes a folded block scalar."
- **Implementation:** `try_consume_folded_block_scalar()` in `lexer/block.rs`
- **Tests:** `rlsp-yaml-parser/tests/smoke/folded_scalars.rs`

### [18] c-single-quote

BNF: `c-single-quote ::= "'"`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`'` (x27, apostrophe, single quote) surrounds a single-quoted flow scalar."
- **Implementation:** `try_consume_single_quoted()` in `lexer/quoted.rs`
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [19] c-double-quote

BNF: `c-double-quote ::= '"'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`"` (x22, double quote) surrounds a double-quoted flow scalar."
- **Implementation:** `try_consume_double_quoted()` in `lexer/quoted.rs`
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`
- **Rationale:** The BNF for [19] is a single-character indicator. Escape-related strictness ([59]/[60]/[61]) and resource limits (1 MiB cap) are attributed to the productions where those rules are enforced, not to the parent indicator production.

### [20] c-directive

BNF: `c-directive ::= '%'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "`%` (x25, percent) denotes a directive line."
- **Implementation:** `is_directive_line()` in `lexer.rs`
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs`

### [21] c-reserved

BNF: `c-reserved ::= '@' | '\`'`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "The `@` (x40, at) and `` ` `` (x60, grave accent) are reserved for future use."
- **Implementation:** `is_c_indicator()` in `chars.rs` includes `@` and `` ` ``; `scan_plain_line_block()` in `lexer/plain.rs` rejects reserved chars as plain scalar starts
- **Tests:** `tests/yaml-test-suite/src/R4YG.yaml`

### [22] c-indicator

BNF: `c-indicator ::= c-sequence-entry | c-mapping-key | c-mapping-value | c-collect-entry | c-sequence-start | c-sequence-end | c-mapping-start | c-mapping-end | c-comment | c-anchor | c-alias | c-tag | c-literal | c-folded | c-single-quote | c-double-quote | c-directive | c-reserved`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "Indicators are characters that have special semantics."
- **Implementation:** `is_c_indicator()` in `chars.rs`
- **Tests:** `chars.rs` unit tests `c_indicator_accepts_all_21_indicator_chars`, `c_indicator_rejects`

### [23] c-flow-indicator

BNF: `c-flow-indicator ::= c-collect-entry | c-sequence-start | c-sequence-end | c-mapping-start | c-mapping-end`

- **Verdict:** Strict-conformant
- **Spec (§5.3):** "The `[`, `]`, `{`, `}` and `,` indicators denote structure in flow collections."
- **Implementation:** `is_c_flow_indicator()` in `chars.rs`
- **Tests:** `chars.rs` unit tests `c_flow_indicator_accepts_exactly_five_chars`, `c_flow_indicator_rejects_non_flow_indicators`

### [24] b-line-feed

BNF: `b-line-feed ::= x0A`

- **Verdict:** Strict-conformant
- **Spec (§5.4):** "YAML recognizes the following ASCII line break characters."
- **Implementation:** `detect_break()` in `lines.rs` matches `'\n'`
- **Tests:** `encoding.rs` (`normalize_line_breaks_cases` — lf-only case)

### [25] b-carriage-return

BNF: `b-carriage-return ::= x0D`

- **Verdict:** Strict-conformant
- **Spec (§5.4):** "YAML recognizes the following ASCII line break characters."
- **Implementation:** `detect_break()` in `lines.rs` matches `'\r'`
- **Tests:** `encoding.rs` (`normalize_line_breaks_cases` — lone-cr and crlf cases)

### [26] b-char

BNF: `b-char ::= b-line-feed | b-carriage-return`

- **Verdict:** Strict-conformant
- **Spec (§5.4):** "YAML recognizes the following ASCII line break characters."
- **Implementation:** `find(['\n', '\r'])` in `lines.rs` locates end of line content
- **Tests:** `encoding.rs` (`normalize_line_breaks_cases`)

### [27] nb-char

BNF: `nb-char ::= c-printable - b-char - c-byte-order-mark`

- **Verdict:** Strict-conformant
- **Spec (§5.4):** "All other characters, including the form feed (x0C), are considered to be non-break characters."
- **Implementation:** The line splitter in `lines.rs` treats only `['\n', '\r']` as break characters; `reject_non_printable()` in `lexer.rs` enforces c-printable on stream input, which transitively enforces nb-char on non-break positions
- **Tests:** No direct test; transitively covered by all indentation-sensitive yaml-test-suite cases
- **Note:** Previously Lenient — same root cause as [1] c-printable. Fixed together with [1] in commit `666e2f2`.

### [28] b-break

BNF: `b-break ::= ( b-carriage-return b-line-feed ) | b-carriage-return | b-line-feed`

- **Verdict:** Strict-conformant
- **Spec (§5.4):** "Line breaks are interpreted differently by different systems and have multiple widely used formats."
- **Implementation:** `detect_break()` in `lines.rs` — CRLF checked first, then bare CR, then LF
- **Tests:** `encoding.rs` (`normalize_line_breaks_cases` covers CRLF, lone CR, LF)

### [29] b-as-line-feed

BNF: `b-as-line-feed ::= b-break`

- **Verdict:** Strict-conformant
- **Spec (§5.4):** "Line breaks inside scalar content must be normalized by the YAML processor. Each such line break must be parsed into a single line feed character."
- **Implementation:** The structural pathway — `LineBuffer` discards the terminator and the lexer inserts `'\n'` — achieves the spec MUST. The `normalize_line_breaks()` function in `encoding.rs` also normalizes CRLF/CR before parsing.
- **Tests:** `encoding.rs` (`normalize_line_breaks_cases`)

### [30] b-non-content

BNF: `b-non-content ::= b-break`

- **Verdict:** Strict-conformant
- **Spec (§5.4):** "Outside scalar content, YAML allows any line break to be used to terminate lines."
- **Implementation:** `detect_break()` in `lines.rs` is called after `find(['\n', '\r'])` separates content from terminator; outside scalars the terminator is discarded (non-content)
- **Tests:** No direct test

### [31] s-space

BNF: `s-space ::= x20`

- **Verdict:** Strict-conformant
- **Spec (§5.5):** "YAML recognizes two white space characters: space and tab."
- **Implementation:** `ch == ' '` loop in `lines.rs` counts leading space characters for indentation
- **Tests:** No direct test (indirectly exercised by all indentation-sensitive yaml-test-suite cases)

### [32] s-tab

BNF: `s-tab ::= x09`

- **Verdict:** Strict-conformant
- **Spec (§5.5):** "YAML recognizes two white space characters: space and tab."
- **Implementation:** Used as literal `'\t'` throughout `src/lexer/` and `src/event_iter/`; `is_ns_char()` in `chars.rs` excludes `'\t'`
- **Tests:** `tests/yaml-test-suite/src/4ZYM.yaml`

### [33] s-white

BNF: `s-white ::= s-space | s-tab`

- **Verdict:** Strict-conformant
- **Spec (§5.5):** "YAML recognizes two white space characters: space and tab."
- **Implementation:** Used as `[' ', '\t']` or `' ' | '\t'` patterns throughout `src/lexer/quoted.rs`, `src/event_iter/`
- **Tests:** `tests/yaml-test-suite/src/4ZYM.yaml`

### [34] ns-char

BNF: `ns-char ::= nb-char - s-white`

- **Verdict:** Strict-conformant
- **Spec (§5.5):** "The rest of the (printable) non-break characters are considered to be non-space characters."
- **Implementation:** `is_ns_char()` in `chars.rs`; `reject_non_printable()` enforcement in `lexer.rs` ensures the predicate is honoured on literal stream content
- **Tests:** `chars.rs` unit tests `ns_char_accepts`, `ns_char_rejects`
- **Note:** Previously Lenient — predicate was partially used but not enforced on plain-scalar bodies or quoted-scalar bodies. Fixed together with [1] in commit `666e2f2`.

### [35] ns-dec-digit

BNF: `ns-dec-digit ::= [x30-x39]`

- **Verdict:** Strict-conformant
- **Spec (§5.6):** "A decimal digit for numbers."
- **Implementation:** Digit matching in `lexer/block.rs` (block scalar header); range `'1'..='9'` is Rust's equivalent to `[x31-x39]`
- **Tests:** `rlsp-yaml-parser/tests/smoke/block_scalars.rs`

### [36] ns-hex-digit

BNF: `ns-hex-digit ::= ns-dec-digit | [x41-x46] | [x61-x66]`

- **Verdict:** Strict-conformant
- **Spec (§5.6):** "A hexadecimal digit for escape sequences."
- **Implementation:** `decode_hex_escape()` in `chars.rs` uses `.is_ascii_hexdigit()`; percent-encoded URI validation in `event_iter/properties.rs` via `.is_ascii_hexdigit()`
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml`; `chars.rs` unit tests for `decode_escape`

### [37] ns-ascii-letter

BNF: `ns-ascii-letter ::= [x41-x5A] | [x61-x7A]`

- **Verdict:** Strict-conformant
- **Spec (§5.6):** "ASCII letter (alphabetic) characters."
- **Implementation:** `is_valid_tag_handle()` in `event_iter/properties.rs` uses `.is_ascii_alphanumeric()` which covers `ns-ascii-letter`
- **Tests:** `rlsp-yaml-parser/tests/smoke/tags.rs`

### [38] ns-word-char

BNF: `ns-word-char ::= ns-dec-digit | ns-ascii-letter | '-'`

- **Verdict:** Strict-conformant
- **Spec (§5.6):** "Word (alphanumeric) characters for identifiers."
- **Implementation:** Tag handle validation in `event_iter/properties.rs` uses `.is_ascii_alphanumeric() || c == '-'`; `is_ns_uri_char_single()` in `chars.rs` includes alphanumeric and `-`
- **Tests:** `rlsp-yaml-parser/tests/smoke/tags.rs`

### [39] ns-uri-char

BNF: `ns-uri-char ::= ( '%' ns-hex-digit{2} ) | ns-word-char | '#' | ';' | '/' | '?' | ':' | '@' | '&' | '=' | '+' | '$' | ',' | '_' | '.' | '!' | '~' | '*' | "'" | '(' | ')' | '[' | ']'`

- **Verdict:** Strict-conformant
- **Spec (§5.6):** "URI characters for tags, as defined in the URI specification."
- **Implementation:** `is_ns_uri_char_single()` in `chars.rs` (single-char form); percent-encoded form handled in `event_iter/properties.rs`
- **Tests:** `rlsp-yaml-parser/tests/smoke/tags.rs`

### [40] ns-tag-char

BNF: `ns-tag-char ::= ns-uri-char - c-tag - c-flow-indicator`

- **Verdict:** Strict-conformant
- **Spec (§5.6):** "The `!` character is used to indicate the end of a named tag handle; hence its use in tag shorthands is restricted."
- **Implementation:** `is_ns_tag_char_single()` in `chars.rs`
- **Tests:** `chars.rs` unit tests `ns_tag_char_rejects_flow_indicators`, `ns_tag_char_accepts`, `ns_uri_char_accepts_exclamation_but_tag_char_does_not`

### [41] c-escape

BNF: `c-escape ::= '\'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "All non-printable characters must be escaped. YAML escape sequences use the `\` notation common to most modern computer languages."
- **Implementation:** `decode_and_push_escape()` in `lexer/quoted.rs` dispatches on `\` in the double-quoted scanner
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml`; `tests/yaml-test-suite/src/55WF.yaml`

### [42] ns-esc-null

BNF: `ns-esc-null ::= '0'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII null (x00) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'0' => Some(('\x00', 1))`
- **Tests:** `chars.rs` unit test `decode_escape_success` case `null_escape`

### [43] ns-esc-bell

BNF: `ns-esc-bell ::= 'a'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII bell (x07) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'a' => Some(('\x07', 1))`
- **Tests:** No direct test

### [44] ns-esc-backspace

BNF: `ns-esc-backspace ::= 'b'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII backspace (x08) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'b' => Some(('\x08', 1))`
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml`

### [45] ns-esc-horizontal-tab

BNF: `ns-esc-horizontal-tab ::= 't' | x09`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII horizontal tab (x09) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'t' | '\t' => Some(('\t', 1))`
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml`

### [46] ns-esc-line-feed

BNF: `ns-esc-line-feed ::= 'n'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII line feed (x0A) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'n' => Some(('\n', 1))`
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml`; `chars.rs` unit test `newline_escape`

### [47] ns-esc-vertical-tab

BNF: `ns-esc-vertical-tab ::= 'v'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII vertical tab (x0B) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'v' => Some(('\x0B', 1))`
- **Tests:** No direct test

### [48] ns-esc-form-feed

BNF: `ns-esc-form-feed ::= 'f'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII form feed (x0C) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'f' => Some(('\x0C', 1))`
- **Tests:** No direct test

### [49] ns-esc-carriage-return

BNF: `ns-esc-carriage-return ::= 'r'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII carriage return (x0D) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'r' => Some(('\r', 1))`
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml`

### [50] ns-esc-escape

BNF: `ns-esc-escape ::= 'e'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII escape (x1B) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'e' => Some(('\x1B', 1))`
- **Tests:** No direct test

### [51] ns-esc-space

BNF: `ns-esc-space ::= x20`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII space (x20) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `' ' => Some((' ', 1))`
- **Tests:** No direct test

### [52] ns-esc-double-quote

BNF: `ns-esc-double-quote ::= '"'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII double quote (x22)."
- **Implementation:** `decode_escape()` in `chars.rs` — `'"' => Some(('"', 1))`
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml`

### [53] ns-esc-slash

BNF: `ns-esc-slash ::= '/'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII slash (x2F), for JSON compatibility."
- **Implementation:** `decode_escape()` in `chars.rs` — `'/' => Some(('/', 1))`
- **Tests:** `tests/yaml-test-suite/src/3UYS.yaml`

### [54] ns-esc-backslash

BNF: `ns-esc-backslash ::= '\'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped ASCII back slash (x5C)."
- **Implementation:** `decode_escape()` in `chars.rs` — `'\\' => Some(('\\', 1))`
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml`

### [55] ns-esc-next-line

BNF: `ns-esc-next-line ::= 'N'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped Unicode next line (x85) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'N' => Some(('\u{85}', 1))`
- **Tests:** `chars.rs` unit test `nel_escape`

### [56] ns-esc-non-breaking-space

BNF: `ns-esc-non-breaking-space ::= '_'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped Unicode non-breaking space (xA0) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'_' => Some(('\u{A0}', 1))`
- **Tests:** `chars.rs` unit test `nbsp_escape`

### [57] ns-esc-line-separator

BNF: `ns-esc-line-separator ::= 'L'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped Unicode line separator (x2028) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'L' => Some(('\u{2028}', 1))`
- **Tests:** `chars.rs` unit test `line_sep_escape`

### [58] ns-esc-paragraph-separator

BNF: `ns-esc-paragraph-separator ::= 'P'`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Escaped Unicode paragraph separator (x2029) character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'P' => Some(('\u{2029}', 1))`
- **Tests:** `chars.rs` unit test `para_sep_escape`

### [59] ns-esc-8-bit

BNF: `ns-esc-8-bit ::= 'x' ns-hex-digit{2}`

- **Verdict:** Stricter-than-spec
- **Spec (§5.7):** "Escaped 8-bit Unicode character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'x' => decode_hex_escape(input, 1, 2)`; `try_consume_double_quoted()` in `lexer/quoted.rs` rejects the decoded character if it falls outside `c-printable` or is a bidi-control character
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml`; `chars.rs` unit test `hex_2digit`
- **Rationale:** Named escapes (`\0`, `\a`, …) are exempt; only hex escapes face the printability and bidi-control check. Source comment in `quoted.rs`: "Security: for hex escapes (\x, \u, \U), the decoded character must be a YAML c-printable character."

### [60] ns-esc-16-bit

BNF: `ns-esc-16-bit ::= 'u' ns-hex-digit{4}`

- **Verdict:** Stricter-than-spec
- **Spec (§5.7):** "Escaped 16-bit Unicode character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'u' => decode_hex_escape(input, 1, 4)`; same non-printable and bidi-control rejection in `lexer/quoted.rs`
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml`; `chars.rs` unit test `hex_4digit`
- **Rationale:** Same policy as [59]. Source comment in `quoted.rs`: "Security: reject bidi override characters produced by numeric escapes."

### [61] ns-esc-32-bit

BNF: `ns-esc-32-bit ::= 'U' ns-hex-digit{8}`

- **Verdict:** Stricter-than-spec
- **Spec (§5.7):** "Escaped 32-bit Unicode character."
- **Implementation:** `decode_escape()` in `chars.rs` — `'U' => decode_hex_escape(input, 1, 8)`; same non-printable and bidi-control rejection in `lexer/quoted.rs`
- **Tests:** `chars.rs` unit tests `hex_8digit`, `high_plane_codepoint`
- **Rationale:** Same policy as [59] and [60].

### [62] c-ns-esc-char

BNF: `c-ns-esc-char ::= c-escape ( ns-esc-null | ns-esc-bell | ns-esc-backspace | ns-esc-horizontal-tab | ns-esc-line-feed | ns-esc-vertical-tab | ns-esc-form-feed | ns-esc-carriage-return | ns-esc-escape | ns-esc-space | ns-esc-double-quote | ns-esc-slash | ns-esc-backslash | ns-esc-next-line | ns-esc-non-breaking-space | ns-esc-line-separator | ns-esc-paragraph-separator | ns-esc-8-bit | ns-esc-16-bit | ns-esc-32-bit )`

- **Verdict:** Strict-conformant
- **Spec (§5.7):** "Note that escape sequences are only interpreted in double-quoted scalars."
- **Implementation:** `decode_escape()` in `chars.rs`; invoked exclusively from the double-quoted scanner in `lexer/quoted.rs`
- **Tests:** `tests/yaml-test-suite/src/G4RS.yaml`; `tests/yaml-test-suite/src/55WF.yaml`; `chars.rs` unit tests (comprehensive)
- **Rationale:** [62] is the dispatch union composing all 20 alternates. The strictness on [59]/[60]/[61] is captured at those sub-productions. Marking [62] Strict-conformant avoids double-counting — strictness is attributed to the production where the rule is enforced.
