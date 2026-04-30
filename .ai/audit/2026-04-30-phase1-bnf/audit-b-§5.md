---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: B
section: §5 (with §3, §4)
date: 2026-04-30
---

### [§3] Not Applicable (descriptive)

BNF: (no BNF — meta-notation)
Spec prose: §3 ("Processes and Models"): "YAML is both a text format and a method for presenting any native data structure in this format. Therefore, this specification defines two concepts: a class of data objects called YAML representations and a syntax for presenting YAML representations as a series of characters, called a YAML stream." (https://yaml.org/spec/1.2.2/, §3.)
Verdict: Not-applicable
Evidence: n/a — no implementation site
Reasoning: Chapter 3 of the YAML 1.2.2 spec contains no numbered BNF productions; it describes the dump/load pipeline, the representation graph, the serialization tree, and the presentation stream. None of these prose statements impose a single, unambiguous, mechanically-verifiable constraint on a parser implementation that maps to a discrete code site. A parser conformance audit operates at the level of grammar productions; a chapter that defines processes and models has no production to verdict.

### [§4] Not Applicable (meta-notation)

BNF: (no BNF — meta-notation)
Spec prose: §4 ("Syntax Conventions"): "The following chapters formally define the syntax of YAML character streams, using parameterized BNF productions. Each BNF production is both named and numbered for easy reference. Whenever possible, basic structures are specified before the more complex structures using them in a 'bottom up' fashion." (https://yaml.org/spec/1.2.2/, §4.)
Verdict: Not-applicable
Evidence: n/a — no implementation site
Reasoning: Chapter 4 defines the meta-notation used in §5–§9 — production naming prefixes (`c-`, `b-`, `nb-`, `s-`, `ns-`, `l-`), parameter conventions (`(n)`, `(c)`), and the BNF dialect itself. It establishes the language used to write the productions, not the productions themselves. A parser cannot be conformant or non-conformant against the meta-notation; the meta-notation is the medium in which conformance is measured.

### [1] c-printable

BNF: `c-printable ::= x09 | x0A | x0D | [x20-x7E] | x85 | [xA0-xD7FF] | [xE000-xFFFD] | [x010000-x10FFFF]`
Spec prose: §5.1: "To ensure readability, YAML streams use only the printable subset of the Unicode character set. The allowed character range explicitly excludes the C0 control block x00-x1F (except for TAB x09, LF x0A and CR x0D which are allowed), DEL x7F, the C1 control block x80-x9F (except for NEL x85 which is allowed), the surrogate block xD800-xDFFF, xFFFE and xFFFF. On input, a YAML processor must accept all characters in this printable subset."
Verdict: Lenient
Evidence: `rlsp-yaml-parser/src/chars.rs:14-26` (`is_c_printable` predicate definition); call sites checked via `grep -rn 'is_c_printable' src/`: only `lexer/quoted.rs:580` (escape-decoded character) and the unit tests in `chars.rs:241-258`. No call site validates literal stream characters.
Reasoning: §5.1 says a YAML processor "must accept all characters in this printable subset", and the spec excludes the listed control codes, surrogates, U+FFFE, and U+FFFF from that subset. Strict conformance requires that the parser both accept printable input and reject non-printable input that appears unescaped in the stream. The parser's `is_c_printable` function correctly encodes the inclusion ranges, and unit tests at `chars.rs:241-258` lock the predicate's truth table. However, `is_c_printable` is invoked exactly once on input data — at `quoted.rs:580`, gating the decoded character produced by `\x`/`\u`/`\U` escapes. Literal stream characters (inside double-quoted scalars, single-quoted scalars, plain scalars, block scalars, comment text, anchor names, tag URIs) are not validated against `is_c_printable`. The `step_in_document` post-dispatch fallback at `event_iter/step.rs:1032-1045` rejects only the *first* character of an unrecognised line when that character is not whitespace and not `ns-char`; it does not scan the full content of any token. A NUL inside a comment is rejected at `lexer/plain.rs:81` only on the trailing-comment branch of plain scalars. Other non-printable characters (DEL x7F, C0 controls other than NUL, the C1 block x80–x9F except NEL, U+FFFE, U+FFFF) pass through any quoted scalar, block scalar, or plain scalar body without diagnosis. The conformance doc labels this entry "Conformant" with citation `chars.rs:14-26`; that citation is the *predicate definition* only — not an enforcement site — so the doc's classification rests on the existence of a function rather than on its application to input. Per the audit's "predicate defined but no caller invokes it on input" rule, the verdict is `Lenient`.

### [2] nb-json

BNF: `nb-json ::= x09 | [x20-x10FFFF]`
Spec prose: §5.1: "To ensure JSON compatibility, YAML processors must allow all non-C0 characters inside quoted scalars. To ensure readability, non-printable characters should be escaped on output, even inside such scalars."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lexer/quoted.rs:618-751` (`scan_double_quoted_line` body uses `memchr2(b'"', b'\\', …)` and accepts every byte between quote/backslash hits); `rlsp-yaml-parser/src/lexer/quoted.rs:415-455` (`scan_single_quoted_line` accepts every byte that is not the unescaped closing `'`); the literal-stream-character branch never invokes `is_c_printable` and accepts tab and any non-C0 character.
Reasoning: nb-json is the broader "JSON compatibility" character class — tab and every Unicode codepoint from U+0020 through U+10FFFF, with no exclusion of C1 controls, surrogates, U+FFFE, U+FFFF, or the BOM (which is itself permitted *inside* quoted scalars per §5.2: "byte order marks are also allowed inside quoted scalars"). The spec MUST is "allow all non-C0 characters" inside quoted scalars; the parser's quoted scanners accept any byte that is not the structural delimiter (`"` or `\` for double-quoted; `'` for single-quoted) and rely on the input being valid UTF-8 to keep multi-byte characters intact. Tab is admitted because the scanners do not reject it. Verdict is `Strict-conformant` because the parser admits the full nb-json set inside quoted scalars; the C0 exclusion is enforced indirectly only at escape-decode (it does not need to be enforced on literal bytes by spec because the spec MUST is "allow all non-C0", not "reject C0", and a literal NUL byte happens to be admitted too — which is consistent with an input string that already contains a literal NUL, since the parser takes `&str` and Rust strings can hold NUL).

### [3] c-byte-order-mark

BNF: `c-byte-order-mark ::= xFEFF`
Spec prose: §5.2: "If a character stream begins with a byte order mark, the character encoding will be taken to be as indicated by the byte order mark. Otherwise, the stream must begin with an ASCII character. […] Byte order marks may appear at the start of any document, however all documents in the same stream must use the same character encoding. To allow for JSON compatibility, byte order marks are also allowed inside quoted scalars."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/encoding.rs:88-96` (`decode` strips BOM after encoding detection); `rlsp-yaml-parser/src/lines.rs:110-127` (`scan_line` strips a leading UTF-8 BOM on the first line); `rlsp-yaml-parser/src/lines.rs:282-305` (`signal_document_boundary` strips a BOM from the already-primed next line at document-prefix positions); `rlsp-yaml-parser/src/lexer.rs:131-146` (`skip_blank_lines_between_docs` calls `signal_document_boundary` after consuming inter-doc blanks); `rlsp-yaml-parser/src/event_iter/step.rs:64-82` (a BOM as the first character of a non-prefix line inside a document body is rejected with "invalid character U+FEFF in document").
Reasoning: §5.2 permits a BOM at stream start and at the start of any document, and additionally permits a BOM literally inside a quoted scalar. The parser handles all three cases. Stream start: `decode` (when the caller routes through it) and `LineBuffer::new` (the direct `&str` path used by `parse_events`) both strip a leading BOM. Document prefix (after `...`): `signal_document_boundary` strips one BOM from the primed next line, called from `skip_blank_lines_between_docs`. Inside a document body: `step_in_document` rejects a leading BOM at the start of a content line, in agreement with the spec's exclusion of U+FEFF from `nb-char` (the BOM is not a content character). Inside quoted scalars: the quoted scanners do not test for BOM, so a literal BOM inside a `"…"` scalar is accepted as part of the value, matching the §5.2 carve-out. The implementation matches the spec's positive permissions and its negative space (mid-stream BOM rejected outside quoted scalars).

### [4] c-sequence-entry

BNF: `c-sequence-entry ::= '-'`
Spec prose: §5.3: "'-' (x2D, hyphen) denotes a block sequence entry."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/step.rs:287-293` (dispatch arm `Some(b'-')` calling `peek_sequence_entry` then `handle_sequence_entry`); `rlsp-yaml-parser/src/event_iter/block.rs` and `block/sequence.rs` consume the `-` indicator under the rules of §8.2.1 (next byte must be space, tab, or end-of-line for the `-` to be a sequence indicator rather than a plain-scalar leading character).
Reasoning: The spec assigns the single character `-` as the block sequence entry indicator. The parser's dispatch matches `b'-'` as the first non-whitespace byte and routes to the sequence-entry handler. The parser's plain-scalar guard (`scan_plain_line_block` in `plain.rs`) treats a leading `-` followed by `ns-plain-safe` as a plain scalar character, matching the spec's §7.3.3 carve-out where a `-` followed by content is part of a plain scalar; only `-` followed by whitespace or end-of-line is the indicator. The behaviour at the dispatch boundary is exactly the spec.

### [5] c-mapping-key

BNF: `c-mapping-key ::= '?'`
Spec prose: §5.3: "'?' (x3F, question mark) denotes a mapping key."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/block/mapping.rs:52` (`peek_mapping_entry` strips a leading `?` for the explicit-key indicator); `rlsp-yaml-parser/src/event_iter/step.rs:282-878` dispatches the mapping case via `peek_mapping_entry` after the byte-prefix match falls through.
Reasoning: §5.3 names `?` the explicit-key indicator. The parser recognises `?` followed by whitespace as the explicit-key marker and routes the line to the mapping-entry handler. A `?` followed by a non-whitespace `ns-plain-safe` character (e.g. `?foo`) starts a plain scalar per spec §7.3.3, which is what `ns_plain_first_block` at `plain.rs:287-302` enforces. The two cases are correctly distinguished in the parser.

### [6] c-mapping-value

BNF: `c-mapping-value ::= ':'`
Spec prose: §5.3: "':' (x3A, colon) denotes a mapping value."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/line_mapping.rs:68-187` (`find_value_indicator_offset` locates `:` followed by whitespace, `\n`, `\r`, or end-of-input); `rlsp-yaml-parser/src/event_iter/flow.rs` uses the same recognition rules in flow context.
Reasoning: The parser recognises `:` as the mapping value indicator only when followed by whitespace, end-of-line, or end-of-input. A `:` followed by an `ns-char` (no whitespace) is treated as part of a plain scalar — the spec rule from §7.3.3 / §7.4. The implementation handles both block context (via `find_value_indicator_offset` and `peek_mapping_entry`) and flow context (per `flow.rs:1538` and the FlowMappingPhase state machine). The colon-as-indicator vs colon-as-content distinction is centralised in `find_value_indicator_offset:174-184`, which checks the byte after `:` against the spec's separator set.

### [7] c-collect-entry

BNF: `c-collect-entry ::= ','`
Spec prose: §5.3: "',' (x2C, comma) ends a flow collection entry."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/flow.rs:662-700` (the `if ch == ','` branch in the flow scanner inner loop handles the comma separator, with leading-comma and double-comma rejection).
Reasoning: The comma separates entries in flow collections. The parser's flow scanner treats `,` only inside an open `[`/`{` frame (the dispatcher in `step_in_document` at `step.rs:300` rejects `]` and `}` outside flow context, and `,` outside flow context falls into the plain-scalar scanner's terminator set in `scan_plain_line_block`). Inside flow, the comma advances the entry counter and triggers leading/empty-comma diagnostics — those diagnostics are stricter than the spec's silence on leading commas, but `[,]` is invalid by the BNF since `c-collect-entry` does not produce empty entries; rejecting leading commas is a stricter behaviour but follows the production grammar. The base recognition of `,` as the entry separator is exactly the spec.

### [8] c-sequence-start

BNF: `c-sequence-start ::= '['`
Spec prose: §5.3: "'[' (x5B, left bracket) starts a flow sequence."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/flow.rs:394-446` (the `if ch == '['` branch pushes `FlowFrame::Sequence` onto the flow stack and emits `SequenceStart` with style=Flow); `rlsp-yaml-parser/src/event_iter/step.rs:298` dispatches `Some(b'[' | b'{')` to `handle_flow_collection`; `rlsp-yaml-parser/src/chars.rs:58-60` (`is_c_flow_indicator` includes `[`).
Reasoning: `[` opens a flow sequence; the parser recognises this in two paths: as the entry to a top-level flow collection from block context (dispatched via `step.rs:298`), and as the start of a nested flow collection inside an existing flow frame (handled at `flow.rs:394-446`). In both cases the parser emits `SequenceStart { style: Flow }` and pushes the appropriate frame. The depth-limit check at `flow.rs:396-404` is a security control (`MAX_COLLECTION_DEPTH`), not a deviation from the spec — the spec sets no upper bound, and the limit only fires for pathologically nested input.

### [9] c-sequence-end

BNF: `c-sequence-end ::= ']'`
Spec prose: §5.3: "']' (x5D, right bracket) ends a flow sequence."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/flow.rs:475-499` (the `if ch == ']' || ch == '}'` branch with the `(']', FlowFrame::Sequence { … })` arm pops the sequence frame and emits `SequenceEnd`); `rlsp-yaml-parser/src/event_iter/flow.rs:521-527` rejects `]` when the top frame is a `FlowFrame::Mapping`; `rlsp-yaml-parser/src/event_iter/step.rs:300-310` rejects a stray `]` outside flow context.
Reasoning: The parser pairs `[` with `]` via the flow stack: a `]` is valid only when the topmost flow frame is `FlowFrame::Sequence`. Mismatched `]` against a `FlowFrame::Mapping` produces "expected '}' to close flow mapping, found ']'", and a `]` with an empty stack produces "unexpected ']' outside flow context". After closing, the parent frame's bookkeeping is updated at `flow.rs:541-574` so subsequent `,` and `:` work correctly. This is the spec's pairing behaviour with proper diagnostics on misuse.

### [10] c-mapping-start

BNF: `c-mapping-start ::= '{'`
Spec prose: §5.3: "'{' (x7B, left brace) starts a flow mapping."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/flow.rs:394-468` (the `else` branch within `if ch == '[' || ch == '{'` handles `{` by pushing `FlowFrame::Mapping` and emitting `MappingStart` with style=Flow); `rlsp-yaml-parser/src/event_iter/step.rs:298` dispatches `b'{'` alongside `b'['` to `handle_flow_collection`; `rlsp-yaml-parser/src/chars.rs:58-60` (`is_c_flow_indicator` includes `{`).
Reasoning: `{` opens a flow mapping; the parser recognises it via the same dispatch path as `[` and pushes a `FlowFrame::Mapping` with phase=Key. The mapping frame carries the explicit-key-pending flag, the after-colon flag, and the has-value flag that drive the `?`/`:`/`,` state machine in `flow.rs`. Recognition is consistent with §5.3's single-character indicator; the additional state is necessary to implement §7.4.2.

### [11] c-mapping-end

BNF: `c-mapping-end ::= '}'`
Spec prose: §5.3: "'}' (x7D, right brace) ends a flow mapping."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/flow.rs:501-520` (the `('}', FlowFrame::Mapping { … })` arm pops the mapping frame and emits `MappingEnd`, with empty-key-and-value padding when an explicit key was opened with no body); `rlsp-yaml-parser/src/event_iter/flow.rs:528-534` rejects `}` against a sequence frame; `rlsp-yaml-parser/src/event_iter/step.rs:300-310` rejects stray `}` outside flow context.
Reasoning: `}` closes a flow mapping; the parser's pairing logic mirrors that of `]`/`[`. The explicit-key-pending and Value-phase padding emit empty scalars for the key/value as required by spec §7.4.2 (a `?` with no following key implies an empty key, and a key without a `:` implies an empty value). Mismatched `}` against a sequence frame produces a clear error. Recognition of the indicator is exactly per spec.

### [12] c-comment

BNF: `c-comment ::= '#'`
Spec prose: §5.3: "'#' (x23, octothorpe, hash, sharp, pound, number sign) denotes a comment."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lexer/comment.rs:30-33` (`trim_start_matches([' ', '\t'])` then `starts_with('#')` triggers comment lexing); `rlsp-yaml-parser/src/lexer/comment.rs:50-51` (text body is the slice after the `#`); `rlsp-yaml-parser/src/lexer.rs:179-184` (`is_comment_line` predicate).
Reasoning: A `#` after optional leading whitespace begins a comment line; the parser's comment lexer extracts everything after the `#` up to the line terminator and emits an `Event::Comment` with the body as a borrowed slice. The body is bounded by `MAX_COMMENT_LEN` for security (`comment.rs:53-60`). The leading-whitespace requirement of `s-b-comment` (a `#` after content must be preceded by whitespace) is handled by the plain-scalar scanner at `plain.rs:320-330` (`ns_plain_char_block` rejects `#` only when the previous character was whitespace) and by `find_value_indicator_offset:162-173` (a non-whitespace-preceded `#` is content, not comment). The `#` indicator recognition matches §5.3.

### [13] c-anchor

BNF: `c-anchor ::= '&'`
Spec prose: §5.3: "'&' (x26, ampersand) denotes a node's anchor property."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/step.rs:640-867` (the `Some(b'&')` arm of the dispatch match; calls `scan_anchor_name` after the leading `&`); `rlsp-yaml-parser/src/event_iter/properties.rs:23-45` (`scan_anchor_name` consumes `ns-anchor-char` characters); `rlsp-yaml-parser/src/event_iter/flow.rs` handles the same indicator in flow context.
Reasoning: `&` introduces an anchor property attached to the next node. The parser recognises a leading `&` as the anchor indicator, scans the anchor name as a contiguous run of `ns-anchor-char` (per §6.9.2 / production [102]), and stores the anchor in `pending_anchor` until the next `Scalar`, `SequenceStart`, or `MappingStart` event consumes it via `make_meta`. Empty names are rejected. Length is bounded by `MAX_ANCHOR_NAME_BYTES`. The recognition is exactly the spec's §5.3 indicator semantics.

### [14] c-alias

BNF: `c-alias ::= '*'`
Spec prose: §5.3: "'*' (x2A, asterisk) denotes an alias node."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/step.rs:314-455` (the `Some(b'*')` arm scans the alias name and emits an `Event::Alias`); `rlsp-yaml-parser/src/event_iter/properties.rs:23-45` (`scan_anchor_name` is reused to scan the alias name, since `c-alias` and `c-anchor` share the `ns-anchor-char` body).
Reasoning: `*` introduces an alias node; the parser recognises a leading `*` and scans the following characters as `ns-anchor-char` to obtain the alias target. The implementation enforces the spec's §7.1 prohibition on alias-with-properties: a `pending_tag` or `Inline` `pending_anchor` produces an error at `step.rs:328-344`. The recognition is exactly the spec's §5.3 indicator semantics; the property-prohibition is a §7.1 layer enforced at the same site.

### [15] c-tag

BNF: `c-tag ::= '!'`
Spec prose: §5.3: "The '!' (x21, exclamation) is used for specifying node tags. It is used to denote tag handles used in tag directives and tag properties; to denote local tags; and as the non-specific tag for non-plain scalars."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/step.rs:458-637` (the `Some(b'!')` arm calls `scan_tag` and stores the resolved tag in `pending_tag`); `rlsp-yaml-parser/src/event_iter/properties.rs:85-234` (`scan_tag` handles verbatim `!<URI>`, primary `!!suffix`, named-handle `!handle!suffix`, secondary `!suffix`, and non-specific `!`).
Reasoning: §5.3 names `!` the tag indicator; §6.8.1 elaborates the four tag-property forms. The parser recognises a leading `!` as the start of a tag property, dispatches to `scan_tag`, which handles each form and validates URI characters and percent-encoding via `is_ns_uri_char_single` and the percent-hex check. The handle is resolved against the document's `directive_scope.tag_handles`. Length is bounded by `MAX_TAG_LEN`. The five forms covered match the spec exactly.

### [16] c-literal

BNF: `c-literal ::= '|'`
Spec prose: §5.3: "'|' (7C, vertical bar) denotes a literal block scalar."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lexer/block.rs:41-274` (`try_consume_literal_block_scalar`); `rlsp-yaml-parser/src/lexer/block.rs:48` (the `starts_with('|')` guard).
Reasoning: `|` introduces a literal block scalar header. The parser's block-scalar lexer detects a leading `|`, parses the optional indent indicator and chomp indicator via `parse_block_header`, and then collects continuation lines according to §8.1. Recognition of the indicator is per spec; the body-handling logic implements §8.1.2 productions [170]–[173].

### [17] c-folded

BNF: `c-folded ::= '>'`
Spec prose: §5.3: "'>' (x3E, greater than) denotes a folded block scalar."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lexer/block.rs:288-351` (`try_consume_folded_block_scalar`); `rlsp-yaml-parser/src/lexer/block.rs:294` (the `starts_with('>')` guard).
Reasoning: `>` introduces a folded block scalar header. The parser's block-scalar lexer detects a leading `>` and shares the header-parsing path with `|` via `parse_block_header`. Body folding is handled by the folded-specific consumer in `block.rs`. Recognition of the indicator is per spec.

### [18] c-single-quote

BNF: `c-single-quote ::= "'"`
Spec prose: §5.3: "\"'\" (x27, apostrophe, single quote) surrounds a single-quoted flow scalar."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lexer/quoted.rs:27-153` (`try_consume_single_quoted`); `rlsp-yaml-parser/src/lexer/quoted.rs:35` (the `starts_with('\'')` guard).
Reasoning: `'` opens and closes a single-quoted flow scalar. The parser's quoted lexer detects the opening `'`, consumes characters until the closing `'` (with `''` escape handled at `quoted.rs:428-433`), and applies multi-line folding at `quoted.rs:79-153` per §7.3.2. Recognition of the delimiter is per spec.

### [19] c-double-quote

BNF: `c-double-quote ::= '"'`
Spec prose: §5.3: "'\"' (x22, double quote) surrounds a double-quoted flow scalar."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lexer/quoted.rs:178-242` (`try_consume_double_quoted`); `rlsp-yaml-parser/src/lexer/quoted.rs:186` (the `starts_with('"')` guard).
Reasoning: `"` opens and closes a double-quoted flow scalar. The parser's double-quoted lexer detects the opening `"`, dispatches per-line scanning to `scan_double_quoted_line` which uses `memchr2(b'"', b'\\', …)` to handle quoted body and escape sequences, and applies multi-line folding via `collect_double_quoted_continuations` per §7.3.1. Recognition of the delimiter is per spec.

### [20] c-directive

BNF: `c-directive ::= '%'`
Spec prose: §5.3: "'%' (x25, percent) denotes a directive line."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lexer.rs:148-154` (`is_directive_line` checks `line.content.starts_with('%')`); `rlsp-yaml-parser/src/lexer.rs:161-174` (`try_consume_directive_line` consumes the line); `rlsp-yaml-parser/src/event_iter/directives.rs:51-104` (`parse_directive` dispatches on the directive name).
Reasoning: `%` introduces a directive line, recognised only at the start of a line (column 0, no leading whitespace). The parser checks `starts_with('%')` on the raw line content, so a `%` inside a content line is not mistaken for a directive. Inside a document body, a `%YAML ` or `%TAG ` line followed by a `---` marker is rejected at `step.rs:223-244` as an authorial mistake (forgot to close the previous document with `...`). Recognition matches §5.3.

### [21] c-reserved

BNF: `c-reserved ::= '@' | '\``
Spec prose: §5.3: "The '@' (x40, at) and '`' (x60, grave accent) are reserved for future use."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:33-55` (`is_c_indicator` includes `'@'` and `` '`' ``); `rlsp-yaml-parser/src/lexer/plain.rs:287-302` (`ns_plain_first_block` returns `false` for any `c-indicator` other than `?`, `:`, `-`, so `@` and `` ` `` cannot start a plain scalar); `rlsp-yaml-parser/src/event_iter/line_mapping.rs:73-94` rejects `@` and `` ` `` as the first byte of an implicit mapping key.
Reasoning: §5.3 reserves `@` and `` ` `` for future use; spec [126] `ns-plain-first` excludes them from plain-scalar starters. The parser's `is_c_indicator` includes both characters, and `ns_plain_first_block` rejects them at the start of a plain scalar. Inside scalars (quoted or as continuation chars in plain), they are accepted as content characters per §5.3 ("reserved for future use" — they are reserved as starters, not banned everywhere). The implementation matches the spec's reservation.

### [22] c-indicator

BNF: `c-indicator ::= c-sequence-entry | c-mapping-key | c-mapping-value | c-collect-entry | c-sequence-start | c-sequence-end | c-mapping-start | c-mapping-end | c-comment | c-anchor | c-alias | c-tag | c-literal | c-folded | c-single-quote | c-double-quote | c-directive | c-reserved`
Spec prose: §5.3: "Indicators are characters that have special semantics."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:33-55` (`is_c_indicator` enumerates exactly the 21 indicator characters: `-`, `?`, `:`, `,`, `[`, `]`, `{`, `}`, `#`, `&`, `*`, `!`, `|`, `>`, `\'`, `"`, `%`, `@`, `` ` ``); unit tests at `rlsp-yaml-parser/src/chars.rs:264-281` lock the full set against acceptance and verify rejection of non-indicator characters.
Reasoning: The composite production `c-indicator` is the union of [4]–[21]. The parser encodes the union as a `matches!` arm covering all 21 characters. The unit test `c_indicator_accepts_all_21_indicator_chars` enumerates the same set and asserts each is accepted; `c_indicator_rejects` confirms representative non-indicators (lowercase letter, digit, space) are not. The predicate matches the spec's enumeration exactly.

### [23] c-flow-indicator

BNF: `c-flow-indicator ::= c-collect-entry | c-sequence-start | c-sequence-end | c-mapping-start | c-mapping-end`
Spec prose: §5.3: "The '[', ']', '{', '}' and ',' indicators denote structure in flow collections. They are therefore forbidden in some cases, to avoid ambiguity in several constructs."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:58-60` (`is_c_flow_indicator` matches exactly `,`, `[`, `]`, `{`, `}`); unit tests at `rlsp-yaml-parser/src/chars.rs:283-298` (`c_flow_indicator_accepts_exactly_five_chars`, `c_flow_indicator_rejects_non_flow_indicators`); used by `is_ns_anchor_char` (`chars.rs:151`) to exclude flow indicators from anchor names, and by `is_ns_tag_char_single` (`chars.rs:121-143`) which excludes the same five characters from tag bodies.
Reasoning: The composite `c-flow-indicator` is the five characters that delimit flow collections; the parser's predicate matches the set exactly. Downstream uses (anchor-char, tag-char) consume the predicate to enforce the spec's §6.9.2 anchor restriction (`ns-anchor-char = ns-char – c-flow-indicator`) and §6.8.1 tag-char restriction (`ns-tag-char = ns-uri-char – c-tag – c-flow-indicator`). The predicate is one place where the audit can find structurally-correct downstream wiring.

### [24] b-line-feed

BNF: `b-line-feed ::= x0A`
Spec prose: §5.4: "YAML recognizes the following ASCII line break characters."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lines.rs:98-100` (the `strip_prefix('\n')` arm of `detect_break` recognises LF as `BreakType::Lf`); `rlsp-yaml-parser/src/lines.rs:130-132` (the line splitter uses `find(['\n', '\r'])` to identify the end of the line content).
Reasoning: `b-line-feed` is the ASCII LF character. The parser's line splitter matches `\n` as a line terminator and produces `BreakType::Lf`. The terminator is consumed from the byte stream and not retained in line content. This is the correct interpretation of spec §5.4.

### [25] b-carriage-return

BNF: `b-carriage-return ::= x0D`
Spec prose: §5.4: "YAML recognizes the following ASCII line break characters."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lines.rs:91-97` (the `strip_prefix("\r\n")` and `strip_prefix('\r')` arms of `detect_break` recognise CR-as-CRLF and bare CR); `rlsp-yaml-parser/src/lines.rs:130-132` (the line splitter uses `find(['\n', '\r'])`).
Reasoning: `b-carriage-return` is the ASCII CR character. The parser handles bare CR (legacy Mac-style) and CR-LF (Windows-style) terminators in the same `detect_break` function. CRLF is checked before bare CR so `\r\n` is consumed as a single terminator. This is exactly the recognition required by §5.4.

### [26] b-char

BNF: `b-char ::= b-line-feed | b-carriage-return`
Spec prose: §5.4: "YAML recognizes the following ASCII line break characters."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lines.rs:130-132` (`content_start.find(['\n', '\r'])` locates the first `b-char`); `rlsp-yaml-parser/src/lines.rs:91-102` (`detect_break` recognises CRLF, bare CR, bare LF).
Reasoning: `b-char` is the union of LF and CR. The line splitter identifies the line terminator by searching for either character; the resulting `BreakType` distinguishes the four cases (Lf, Cr, CrLf, Eof). The implementation matches the spec's union exactly.

### [27] nb-char

BNF: `nb-char ::= c-printable - b-char - c-byte-order-mark`
Spec prose: §5.4: "All other characters, including the form feed (x0C), are considered to be non-break characters. Note that these include the non-ASCII line breaks: next line (x85), line separator (x2028) and paragraph separator (x2029)."
Verdict: Lenient
Evidence: `rlsp-yaml-parser/src/lines.rs:130-132` (line splitter uses `find(['\n', '\r'])`, so x85/x2028/x2029 are correctly treated as non-break and remain inside line content); however no predicate or call site validates that line content excludes the C0 control block or U+FEFF — `is_c_printable` is not invoked on line content (see [1]); no `nb-char` predicate is defined in `chars.rs`.
Reasoning: `nb-char` is `c-printable` minus the two break chars and minus the BOM. Strict conformance requires that line-content characters lie within this set: x85, x2028, x2029 are non-break (so they are included in `nb-char`), but C0 controls (other than tab) and the BOM are excluded. The parser correctly does NOT split lines at x85/x2028/x2029 (the line splitter only sees `\n` and `\r`), so the positive side of `nb-char` — non-ASCII line break characters as content — is honoured. The negative side is not enforced: a NUL byte or a DEL inside a line's content passes through to the scalar value with no diagnosis (see [1] for the same gap from a different angle). The conformance doc labels this entry "Conformant" with the rationale "no standalone `nb-char` predicate is defined (the invariant is maintained structurally)"; the structural argument holds for the line-splitting half of the production but not for the `nb-char ⊆ c-printable` half. Per the audit rule for predicate-defined-but-not-enforced, the verdict here is `Lenient` because the implementation accepts `c-printable - b-char - {U+FEFF}` plus the C0/DEL characters that the spec excludes via `c-printable`.

### [28] b-break

BNF: `b-break ::= ( b-carriage-return b-line-feed ) | b-carriage-return | b-line-feed`
Spec prose: §5.4: "Line breaks are interpreted differently by different systems and have multiple widely used formats."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lines.rs:91-102` (`detect_break` checks CRLF first, then bare CR, then LF); `rlsp-yaml-parser/src/lines.rs:31-63` (`BreakType::byte_len` and `BreakType::advance` carry the break-type information through to position arithmetic).
Reasoning: `b-break` is the disjunction of CRLF, CR, and LF. The parser's `detect_break` matches the three forms in the order specified (CRLF first to avoid mis-classifying `\r` of `\r\n` as a bare CR), and the byte-length and position-advance functions in `BreakType` correctly account for one or two bytes consumed depending on the form. The implementation matches the production exactly.

### [29] b-as-line-feed

BNF: `b-as-line-feed ::= b-break`
Spec prose: §5.4: "Line breaks inside scalar content must be normalized by the YAML processor. Each such line break must be parsed into a single line feed character. The original line break format is a presentation detail and must not be used to convey content information."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lines.rs:130-156` (the line splitter strips the terminator from `content`, so line content carries no CR/LF bytes back to the lexer); `rlsp-yaml-parser/src/lexer/quoted.rs:309-315` (multi-line double-quoted folding pushes literal `\n` between lines via `owned.extend(std::iter::repeat_n('\n', pending_blanks))`); `rlsp-yaml-parser/src/lexer/quoted.rs:111-122` (single-quoted multi-line folding pushes literal `'\n'` for blank continuation lines); `rlsp-yaml-parser/src/lexer/block.rs` accumulates block-scalar content with `push('\n')` between content lines.
Reasoning: The spec MUST is "each such line break must be parsed into a single line feed character". The parser achieves this structurally rather than via a `normalize_line_breaks` pre-pass: line terminators (regardless of CRLF, bare CR, or LF) are consumed by `detect_break` and never reach scalar content directly; instead, the lexer reconstructs scalar content by inserting literal `'\n'` between the per-line content slices. The function `encoding::normalize_line_breaks` (`encoding.rs:179-197`) exists for the public `decode` API but is not invoked by `parse_events` on the `&str` input — and that is by design, because the `LineBuffer` discards original terminators and the lexer inserts `'\n'`, which is functionally equivalent to a CRLF→LF normalization plus a bare-CR→LF normalization. The conformance doc cites `encoding.rs:179-197` (the `normalize_line_breaks` function) as the implementation site; that citation is incorrect (`grep -rn normalize_line_breaks src/` returns only the function definition and its tests — production code never calls it), but the *behaviour* the spec requires is achieved through the line-buffer + lexer mechanism. The implementation matches the production via the structural pathway, even though the specific function the doc cites is dead code in the production parser path.

### [30] b-non-content

BNF: `b-non-content ::= b-break`
Spec prose: §5.4: "Outside scalar content, YAML allows any line break to be used to terminate lines."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lines.rs:91-102` (`detect_break` recognises all three break forms); `rlsp-yaml-parser/src/lines.rs:130-156` (the line splitter strips the terminator from `content` so it does not appear in any non-scalar token).
Reasoning: `b-non-content` is `b-break` consumed outside scalar content — i.e. between tokens, between directives, between document markers. The parser handles this by discarding the terminator byte(s) from line content during line splitting; outside scalar contexts the terminator is the line boundary and is otherwise not retained. The acceptance is uniform across CRLF, bare CR, and LF — exactly the spec's "any line break" allowance.

### [31] s-space

BNF: `s-space ::= x20`
Spec prose: §5.5: "YAML recognizes two white space characters: space and tab."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lines.rs:142` (`content.chars().take_while(|&ch| ch == ' ').count()` counts `s-space` for indentation); used as the literal `' '` and in `[' ', '\t']` patterns throughout `lexer/quoted.rs`, `lexer/comment.rs`, `event_iter/directives.rs`, etc.
Reasoning: `s-space` is exactly the ASCII space character. The parser uses `' '` directly as a sentinel everywhere a space is expected (indent counting, separator skipping, `s-separate-in-line` matching). Indentation is counted as a run of `' '` characters — never including tabs (see [63] s-indent for the related production). The recognition of `s-space` as `\x20` is exact.

### [32] s-tab

BNF: `s-tab ::= x09`
Spec prose: §5.5: "YAML recognizes two white space characters: space and tab."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:67-76` (`is_ns_char` excludes `'\t'`); `rlsp-yaml-parser/src/lines.rs:142` (indentation counter ignores tabs); used as the literal `'\t'` in `[' ', '\t']` patterns at `lexer/quoted.rs:34, 107, 185, 276, 294`, `lexer/comment.rs:30`, `event_iter/directives.rs:89, 93, 165, 170`, etc.
Reasoning: `s-tab` is exactly the ASCII tab character. The parser uses `'\t'` directly as a sentinel in separator and whitespace contexts. Tab is excluded from `ns-char` (per [34]) and from indentation (per [63] s-indent and the §6.1 portability requirement). The §6.1 prohibition on tabs as block indentation is enforced at `event_iter/step.rs:42-62` with a clear error message. The recognition of `s-tab` as `\x09` is exact.

### [33] s-white

BNF: `s-white ::= s-space | s-tab`
Spec prose: §5.5: "YAML recognizes two white space characters: space and tab."
Verdict: Strict-conformant
Evidence: Used as `[' ', '\t']` slice patterns and as `|' '| '\t'` match arms throughout the lexer: `lexer/quoted.rs:34, 110, 185, 294` (`trim_start_matches([' ', '\t'])`); `lexer/comment.rs:30` (same); `event_iter/directives.rs:89, 93` (same); `chars.rs:67-68` (the `!matches!(ch, ' ' | '\t' | …)` guard in `is_ns_char`).
Reasoning: `s-white` is the union of space and tab. The parser uses `[' ', '\t']` as the whitespace skip set in every `s-separate-in-line` context. The set is consistent everywhere — there is no point in the parser that treats whitespace as either narrower or broader than the spec's definition. The implementation matches the production exactly.

### [34] ns-char

BNF: `ns-char ::= nb-char - s-white`
Spec prose: §5.5: "The rest of the (printable) non-break characters are considered to be non-space characters."
Verdict: Lenient
Evidence: `rlsp-yaml-parser/src/chars.rs:67-76` (`is_ns_char` predicate); unit tests at `chars.rs:304-319`. Call sites: `lexer/plain.rs:301` (in `ns_plain_first_block`), `lexer/plain.rs:308` (in `ns_plain_safe_block`), `event_iter/step.rs:1035` (rejection of unrecognised first char of an unparsed line). Predicate uses the same character ranges as `is_c_printable` minus whitespace and BOM.
Reasoning: `ns-char` is the spec's `nb-char – s-white` = `c-printable – b-char – c-byte-order-mark – s-space – s-tab`. The predicate at `chars.rs:67-76` is structurally consistent with that definition: it excludes space, tab, CR, LF, BOM, and uses the printable-character ranges. Where it is used (plain-scalar first character, plain-scalar safe character, unrecognised-line first character), the check is correct. However, like [1] and [27], `ns-char` is not enforced on the *bodies* of plain scalars or quoted scalars — `scan_plain_line_block` at `plain.rs:340-…` accepts any non-`:`/`#`/whitespace byte without checking `is_ns_char`, and the quoted scanners pass through any byte. So a non-printable character (DEL, C0 control, U+FFFE) inside a plain-scalar body is accepted as content. The verdict mirrors [1] and [27]: predicate exists and is partially used, but the production-as-content-class invariant is not maintained. `Lenient`.

### [35] ns-dec-digit

BNF: `ns-dec-digit ::= [x30-x39]`
Spec prose: §5.6: "A decimal digit for numbers:"
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lexer/block.rs:562-587` (block-scalar header explicit indent indicator: `'0'` is rejected at line 562-572 as invalid; `ch @ '1'..='9'` is accepted at line 573-587); `rlsp-yaml-parser/src/event_iter/directives.rs:136-143` (`major_str.parse::<u8>()` and `minor_str.parse::<u8>()` validate that the YAML version major/minor parts are decimal digit sequences; Rust's `u8::from_str` accepts only `[0-9]+`); `rlsp-yaml-parser/src/chars.rs:210` (`is_ascii_hexdigit` is a superset that includes `[0-9]` for hex-escape decoding).
Reasoning: `ns-dec-digit` is the digit characters `0-9`. The parser uses Rust pattern `'0'..='9'` for block-scalar indent indicators and Rust's `parse::<u8>()` for directive version digits — both accept exactly the spec's digit set. The block header treats `0` as invalid because §8.1.1.1 says the indent indicator must be in `[1-9]` (the indicator value is `>= 1`); this is correctly enforced.

### [36] ns-hex-digit

BNF: `ns-hex-digit ::= ns-dec-digit | [x41-x46] | [x61-x66]`
Spec prose: §5.6: "A hexadecimal digit for escape sequences:"
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:204-215` (`decode_hex_escape` validates each char with `c.is_ascii_hexdigit()`, which is exactly `[0-9A-Fa-f]`); `rlsp-yaml-parser/src/event_iter/properties.rs:117-123` (verbatim tag URI percent-encoding validates two hex digits via `is_ascii_hexdigit`); `rlsp-yaml-parser/src/event_iter/properties.rs:247-254` (tag-suffix percent-encoding validates two hex digits via `is_ascii_hexdigit`).
Reasoning: `ns-hex-digit` covers `0-9`, `A-F`, `a-f`. Rust's `is_ascii_hexdigit` matches the same set exactly. The parser uses this in three places: hex-escape decoding (`\x`, `\u`, `\U`), verbatim tag percent-encoding `%HH`, and tag-suffix percent-encoding. Each site validates exactly two-or-more hex digits per the production; truncated hex escapes return `None` from `decode_hex_escape` and bubble up as "invalid escape" at `lexer/quoted.rs:563-573`. The implementation matches the spec.

### [37] ns-ascii-letter

BNF: `ns-ascii-letter ::= [x41-x5A] | [x61-x7A]`
Spec prose: §5.6: "ASCII letter (alphabetic) characters:"
Verdict: Strict-conformant
Evidence: Used implicitly via `c.is_ascii_alphanumeric()` in `event_iter/properties.rs:289` (`is_valid_tag_handle` validates named-handle inner characters); `c.is_ascii_alphanumeric()` is exactly `[0-9A-Za-z]` = `ns-dec-digit ∪ ns-ascii-letter`.
Reasoning: `ns-ascii-letter` is `[A-Za-z]`. The parser uses `is_ascii_alphanumeric` (a superset that includes digits) wherever `ns-word-char` is required (named tag handle bodies, §6.8.2.1 production [92]). Since `ns-word-char = ns-dec-digit | ns-ascii-letter | '-'`, using `is_ascii_alphanumeric || c == '-'` is exactly `ns-word-char`. The letter set itself is not used in isolation — only as part of `ns-word-char`. The conformance is maintained through the composite check.

### [38] ns-word-char

BNF: `ns-word-char ::= ns-dec-digit | ns-ascii-letter | '-'`
Spec prose: §5.6: "Word (alphanumeric) characters for identifiers:"
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/properties.rs:289` (`word.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')` — exactly `[0-9A-Za-z\-]+` for the named tag handle body); `rlsp-yaml-parser/src/chars.rs:88-113` (`is_ns_uri_char_single` includes `is_ascii_alphanumeric()` and `'-'`, so `ns-word-char ⊂ ns-uri-char`).
Reasoning: `ns-word-char` is `[0-9A-Za-z\-]`. The parser's named-handle validator uses `is_ascii_alphanumeric() || c == '-'`, which matches the production exactly. The exclusion of `_` from named-handle bodies is tested at `properties.rs:482-533` (the `is_valid_tag_handle_rejects_named_with_underscore` etc. cases). This is conformant.

### [39] ns-uri-char

BNF: `ns-uri-char ::= ( '%' ns-hex-digit{2} ) | ns-word-char | '#' | ';' | '/' | '?' | ':' | '@' | '&' | '=' | '+' | '$' | ',' | '_' | '.' | '!' | '~' | '*' | "'" | '(' | ')' | '[' | ']'`
Spec prose: §5.6: "URI characters for tags, as defined in the URI specification. By convention, any URI characters other than the allowed printable ASCII characters are first encoded in UTF-8 and then each byte is escaped using the '%' character. The YAML processor must not expand such escaped characters. Tag characters must be preserved and compared exactly as presented in the YAML stream, without any processing."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:88-113` (`is_ns_uri_char_single` enumerates the single-char form: `is_ascii_alphanumeric()` plus the punctuation set `-`, `_`, `.`, `!`, `~`, `*`, `'`, `(`, `)`, `[`, `]`, `#`, `;`, `/`, `?`, `:`, `@`, `&`, `=`, `+`, `$`, `,`); the percent-encoded form is handled in `event_iter/properties.rs:113-134` (verbatim) and `properties.rs:241-273` (tag suffix).
Reasoning: The single-char alternatives in the production are `ns-word-char` (alphanumerics plus `-`) plus an explicit punctuation list. The predicate matches that list character-for-character (the unit test at `chars.rs:372-376` confirms `!` is admitted). Percent-encoding is validated exactly per the production: `%` followed by exactly two hex digits, with the percent-encoded bytes preserved verbatim (no expansion) — that's exactly the spec's "must not expand" requirement. The implementation matches the production.

### [40] ns-tag-char

BNF: `ns-tag-char ::= ns-uri-char - c-tag - c-flow-indicator`
Spec prose: §5.6: "The '!' character is used to indicate the end of a named tag handle; hence its use in tag shorthands is restricted. In addition, such shorthands must not contain the '[', ']', '{', '}' and ',' characters. These characters would cause ambiguity with flow collection structures."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:121-143` (`is_ns_tag_char_single` enumerates the same set as `is_ns_uri_char_single` minus `!`, `[`, `]`, `{`, `}`, `,`); unit tests at `chars.rs:352-376` (`ns_tag_char_rejects_flow_indicators`, `ns_tag_char_accepts`, `ns_uri_char_accepts_exclamation_but_tag_char_does_not`).
Reasoning: `ns-tag-char` is `ns-uri-char` minus `c-tag` (the `!`) minus `c-flow-indicator` (the five flow chars). The predicate matches the spec's set difference exactly: it includes alphanumerics and `-_.~*'()#;/?:@&=+$`, and excludes `!`, `[`, `]`, `{`, `}`, `,`. The negative test `ns_tag_char_rejects_flow_indicators` enumerates all five flow chars and verifies they are rejected; the negative test `ns_uri_char_accepts_exclamation_but_tag_char_does_not` locks the `!` distinction between [39] and [40]. Conformance is exact.

### [41] c-escape

BNF: `c-escape ::= '\'`
Spec prose: §5.7: "All non-printable characters must be escaped. YAML escape sequences use the '\\' notation common to most modern computer languages. Each escape sequence must be parsed into the appropriate Unicode character. The original escape sequence is a presentation detail and must not be used to convey content information."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/lexer/quoted.rs:618-702` (`scan_double_quoted_line` uses `memchr2(b'"', b'\\', …)` to find escape sequences, then dispatches to `decode_and_push_escape`); `rlsp-yaml-parser/src/lexer/quoted.rs:557-614` (`decode_and_push_escape` decodes the escape via `chars::decode_escape`); `rlsp-yaml-parser/src/chars.rs:173-199` (`decode_escape` is the dispatch table for all escape codes); `rlsp-yaml-parser/src/lexer/quoted.rs:676-689` (the bare-`\` at end-of-line is the line-continuation escape per §7.3.1).
Reasoning: `c-escape` is the single backslash character that introduces an escape sequence inside a double-quoted scalar. The parser's double-quoted scanner uses `\\` as one of two memchr2 stop bytes; on hitting `\\`, it dispatches to `decode_escape`, which handles all 20 escape forms of [42]–[61]. The escape sequence is consumed and replaced by the decoded character, exactly as the spec requires ("must be parsed into the appropriate Unicode character"). The original escape source is not retained. The §7.3.1 line-continuation escape (`\` at end-of-line suppressing the following break) is handled separately at `quoted.rs:676-689`.

### [42] ns-esc-null

BNF: `ns-esc-null ::= '0'`
Spec prose: §5.7: "Escaped ASCII null (x00) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:177` (`'0' => Some(('\x00', 1))`); unit test `null_escape` at `chars.rs:383`; integration test `double_quoted_named_null_escape_is_ok` at `lexer/quoted.rs:1316-1320`; the c-printable gating at `quoted.rs:580` is conditional on `escape_prefix in {'x','u','U'}`, so named `\0` is NOT subject to printability rejection.
Reasoning: `\0` decodes to U+0000. The parser's table maps `'0'` to `'\x00'` and consumes 1 byte (the `0`). The c-printable security check at `quoted.rs:580` is gated to numeric escapes only, with the comment "Named escapes (\\0, \\a, \\b, …) produce well-known control chars and are exempt". The integration test confirms `"\0"` produces a NUL byte in the output. Conformance is exact.

### [43] ns-esc-bell

BNF: `ns-esc-bell ::= 'a'`
Spec prose: §5.7: "Escaped ASCII bell (x07) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:178` (`'a' => Some(('\x07', 1))`); integration test `double_quoted_all_single_char_escapes` at `lexer/quoted.rs:1186-1203` includes `("\"\\a\"", "\x07")`.
Reasoning: `\a` decodes to U+0007. The parser's table maps `'a'` to `'\x07'`. The integration test confirms the round trip. The named-escape exemption from c-printable applies. Conformance is exact.

### [44] ns-esc-backspace

BNF: `ns-esc-backspace ::= 'b'`
Spec prose: §5.7: "Escaped ASCII backspace (x08) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:179` (`'b' => Some(('\x08', 1))`); integration test `double_quoted_all_single_char_escapes` at `lexer/quoted.rs:1186-1203` includes `("\"\\b\"", "\x08")`.
Reasoning: `\b` decodes to U+0008. The parser's table maps `'b'` to `'\x08'`. The integration test confirms the round trip. Conformance is exact.

### [45] ns-esc-horizontal-tab

BNF: `ns-esc-horizontal-tab ::= 't' | x09`
Spec prose: §5.7: "Escaped ASCII horizontal tab (x09) character. This is useful at the start or the end of a line to force a leading or trailing tab to become part of the content."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:180` (`'t' | '\t' => Some(('\t', 1))` — the production uniquely accepts both the letter `t` and the literal tab character after the backslash); integration test `escape_tab` at `lexer/quoted.rs:1169` (`("\"foo\\tbar\"", "foo\tbar")`).
Reasoning: This production has two alternatives — `t` (letter) and x09 (literal tab). The parser's match arm covers both with the pattern `'t' | '\t'`, mapping each to `'\t'` (U+0009). The integration test exercises the letter form. Conformance is exact, and the dual alternative is the only escape in [42]–[58] that admits two source forms.

### [46] ns-esc-line-feed

BNF: `ns-esc-line-feed ::= 'n'`
Spec prose: §5.7: "Escaped ASCII line feed (x0A) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:181` (`'n' => Some(('\n', 1))`); unit test `newline_escape` at `chars.rs:384`; integration test `escape_newline` at `lexer/quoted.rs:1168` (`("\"foo\\nbar\"", "foo\nbar")`).
Reasoning: `\n` decodes to U+000A. Conformance is exact.

### [47] ns-esc-vertical-tab

BNF: `ns-esc-vertical-tab ::= 'v'`
Spec prose: §5.7: "Escaped ASCII vertical tab (x0B) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:182` (`'v' => Some(('\x0B', 1))`); integration test `double_quoted_all_single_char_escapes` at `lexer/quoted.rs:1186-1203` includes `("\"\\v\"", "\x0B")`.
Reasoning: `\v` decodes to U+000B. Conformance is exact.

### [48] ns-esc-form-feed

BNF: `ns-esc-form-feed ::= 'f'`
Spec prose: §5.7: "Escaped ASCII form feed (x0C) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:183` (`'f' => Some(('\x0C', 1))`); integration test `double_quoted_all_single_char_escapes` at `lexer/quoted.rs:1186-1203` includes `("\"\\f\"", "\x0C")`.
Reasoning: `\f` decodes to U+000C. Conformance is exact.

### [49] ns-esc-carriage-return

BNF: `ns-esc-carriage-return ::= 'r'`
Spec prose: §5.7: "Escaped ASCII carriage return (x0D) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:184` (`'r' => Some(('\r', 1))`); integration test `double_quoted_all_single_char_escapes` at `lexer/quoted.rs:1186-1203` includes `("\"\\r\"", "\r")`.
Reasoning: `\r` decodes to U+000D. Conformance is exact.

### [50] ns-esc-escape

BNF: `ns-esc-escape ::= 'e'`
Spec prose: §5.7: "Escaped ASCII escape (x1B) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:185` (`'e' => Some(('\x1B', 1))`); integration test `double_quoted_all_single_char_escapes` at `lexer/quoted.rs:1186-1203` includes `("\"\\e\"", "\x1B")`.
Reasoning: `\e` decodes to U+001B. Conformance is exact.

### [51] ns-esc-space

BNF: `ns-esc-space ::= x20`
Spec prose: §5.7: "Escaped ASCII space (x20) character. This is useful at the start or the end of a line to force a leading or trailing space to become part of the content."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:186` (`' ' => Some((' ', 1))` — the source byte after `\` is the space character itself); integration test `escape_space` at `lexer/quoted.rs:1173` (`("\"foo\\ bar\"", "foo bar")`).
Reasoning: `\<space>` decodes to U+0020. The parser's match arm uses the literal `' '` source byte, exactly matching the production's x20 source form. Note the production source is the literal space character (x20) following the backslash, not a letter — the parser handles this correctly. Conformance is exact.

### [52] ns-esc-double-quote

BNF: `ns-esc-double-quote ::= '"'`
Spec prose: §5.7: "Escaped ASCII double quote (x22)."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:187` (`'"' => Some(('"', 1))`); integration test `escape_double_quote` at `lexer/quoted.rs:1171` (`("\"say \\\"hi\\\"\"", "say \"hi\"")`).
Reasoning: `\"` decodes to U+0022. Conformance is exact.

### [53] ns-esc-slash

BNF: `ns-esc-slash ::= '/'`
Spec prose: §5.7: "Escaped ASCII slash (x2F), for JSON compatibility."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:188` (`'/' => Some(('/', 1))`); integration test `escape_slash` at `lexer/quoted.rs:1172` (`("\"foo\\/bar\"", "foo/bar")`).
Reasoning: `\/` decodes to U+002F. Conformance is exact, including the spec's JSON-compatibility note (the slash escape is unnecessary in YAML but accepted for JSON-document compatibility).

### [54] ns-esc-backslash

BNF: `ns-esc-backslash ::= '\'`
Spec prose: §5.7: "Escaped ASCII back slash (x5C)."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:189` (`'\\' => Some(('\\', 1))`); integration test `escape_backslash` at `lexer/quoted.rs:1170` (`("\"foo\\\\bar\"", "foo\\bar")`).
Reasoning: `\\` decodes to U+005C. Conformance is exact.

### [55] ns-esc-next-line

BNF: `ns-esc-next-line ::= 'N'`
Spec prose: §5.7: "Escaped Unicode next line (x85) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:190` (`'N' => Some(('\u{85}', 1))`); unit test `nel_escape` at `chars.rs:387`; integration test `double_quoted_all_single_char_escapes` at `lexer/quoted.rs:1186-1203` includes `("\"\\N\"", "\u{85}")`.
Reasoning: `\N` decodes to U+0085 (NEL). Conformance is exact.

### [56] ns-esc-non-breaking-space

BNF: `ns-esc-non-breaking-space ::= '_'`
Spec prose: §5.7: "Escaped Unicode non-breaking space (xA0) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:191` (`'_' => Some(('\u{A0}', 1))`); unit test `nbsp_escape` at `chars.rs:388`; integration test at `lexer/quoted.rs:1186-1203` includes `("\"\\_\"", "\u{A0}")`.
Reasoning: `\_` decodes to U+00A0 (NBSP). Conformance is exact.

### [57] ns-esc-line-separator

BNF: `ns-esc-line-separator ::= 'L'`
Spec prose: §5.7: "Escaped Unicode line separator (x2028) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:192` (`'L' => Some(('\u{2028}', 1))`); unit test `line_sep_escape` at `chars.rs:389`; integration test at `lexer/quoted.rs:1186-1203` includes `("\"\\L\"", "\u{2028}")`.
Reasoning: `\L` decodes to U+2028 (LINE SEPARATOR). Conformance is exact.

### [58] ns-esc-paragraph-separator

BNF: `ns-esc-paragraph-separator ::= 'P'`
Spec prose: §5.7: "Escaped Unicode paragraph separator (x2029) character."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:193` (`'P' => Some(('\u{2029}', 1))`); unit test `para_sep_escape` at `chars.rs:390`; integration test at `lexer/quoted.rs:1186-1203` includes `("\"\\P\"", "\u{2029}")`.
Reasoning: `\P` decodes to U+2029 (PARAGRAPH SEPARATOR). Conformance is exact.

### [59] ns-esc-8-bit

BNF: `ns-esc-8-bit ::= 'x' ns-hex-digit{2}`
Spec prose: §5.7: "Escaped 8-bit Unicode character."
Verdict: Stricter-than-spec
Evidence: `rlsp-yaml-parser/src/chars.rs:194` (`'x' => decode_hex_escape(input, 1, 2)`); `rlsp-yaml-parser/src/chars.rs:204-215` (`decode_hex_escape` validates two hex digits and constructs the codepoint); `rlsp-yaml-parser/src/lexer/quoted.rs:576-588` (the c-printable rejection for hex escapes); `rlsp-yaml-parser/src/lexer/quoted.rs:590-600` (the bidi-control rejection for hex escapes); test `dq_non_printable_hex_escape_is_rejected` at `lexer/quoted.rs:1059-1069`.
Reasoning: `\xHH` decodes a two-digit hex sequence to a codepoint in U+0000..U+00FF. The parser decodes correctly via `decode_hex_escape`, but the spec says "Escaped 8-bit Unicode character" — meaning any of U+0000..U+00FF is permitted. The parser additionally rejects `\xHH` whose decoded character is not in `c-printable` (i.e. C0 controls except tab/LF/CR are rejected, DEL is rejected, the C1 block x80-x9F except NEL is rejected) and rejects bidi override characters. These are stricter behaviours than the BNF grammar requires. The rejections are documented as security controls in the source comments at `quoted.rs:576-578` and `quoted.rs:590-591`. Verdict is `Stricter-than-spec` because the parser correctly decodes but rejects spec-permitted characters as a deliberate security policy. The conformance doc labels this "Strict (security-hardened)", which corresponds to the same finding under their classification scheme.

### [60] ns-esc-16-bit

BNF: `ns-esc-16-bit ::= 'u' ns-hex-digit{4}`
Spec prose: §5.7: "Escaped 16-bit Unicode character."
Verdict: Stricter-than-spec
Evidence: `rlsp-yaml-parser/src/chars.rs:195` (`'u' => decode_hex_escape(input, 1, 4)`); `rlsp-yaml-parser/src/chars.rs:204-215` (`decode_hex_escape` validates four hex digits); `rlsp-yaml-parser/src/lexer/quoted.rs:576-600` (same c-printable + bidi rejection as [59]); test `dq_bidi_escape_is_rejected` at `lexer/quoted.rs:1047-1057`; test `unicode_surrogate_low/high` at `lexer/quoted.rs:1232-1233` (surrogates rejected via `char::from_u32` returning None).
Reasoning: `\uHHHH` decodes a four-digit hex sequence to a Unicode codepoint in U+0000..U+FFFF. Surrogates (U+D800..U+DFFF) are rejected because Rust's `char::from_u32` returns `None` for them — that rejection is a strict spec requirement (surrogates are not valid Unicode scalars). Beyond that, the parser additionally rejects non-c-printable codepoints and bidi-override codepoints, which are stricter than the BNF. Verdict is `Stricter-than-spec` for the same reason as [59].

### [61] ns-esc-32-bit

BNF: `ns-esc-32-bit ::= 'U' ns-hex-digit{8}`
Spec prose: §5.7: "Escaped 32-bit Unicode character."
Verdict: Stricter-than-spec
Evidence: `rlsp-yaml-parser/src/chars.rs:196` (`'U' => decode_hex_escape(input, 1, 8)`); `rlsp-yaml-parser/src/chars.rs:204-215` (`decode_hex_escape` validates eight hex digits and rejects out-of-range codepoints via `char::from_u32`); `rlsp-yaml-parser/src/lexer/quoted.rs:576-600` (same c-printable + bidi rejection as [59]); unit test `high_plane_codepoint` at `chars.rs:394`; integration test `unicode_8digit_supplementary` at `lexer/quoted.rs:1222`; test `unicode_8digit_out_of_range` at `lexer/quoted.rs:1234`.
Reasoning: `\UHHHHHHHH` decodes an eight-digit hex sequence to a Unicode codepoint up to U+10FFFF. Codepoints above U+10FFFF are rejected via `char::from_u32` returning `None` — a strict spec requirement (Unicode is bounded at U+10FFFF). Beyond that, the parser rejects non-c-printable codepoints and bidi-override codepoints, which are stricter than the BNF. Verdict is `Stricter-than-spec` for the same reason as [59] and [60].

### [62] c-ns-esc-char

BNF: `c-ns-esc-char ::= c-escape ( ns-esc-null | ns-esc-bell | ns-esc-backspace | ns-esc-horizontal-tab | ns-esc-line-feed | ns-esc-vertical-tab | ns-esc-form-feed | ns-esc-carriage-return | ns-esc-escape | ns-esc-space | ns-esc-double-quote | ns-esc-slash | ns-esc-backslash | ns-esc-next-line | ns-esc-non-breaking-space | ns-esc-line-separator | ns-esc-paragraph-separator | ns-esc-8-bit | ns-esc-16-bit | ns-esc-32-bit )`
Spec prose: §5.7: "Note that escape sequences are only interpreted in double-quoted scalars. In all other scalar styles, the '\\' character has no special meaning and non-printable characters are not available."
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/chars.rs:173-199` (`decode_escape` is the dispatch table for the union of [42]–[61] plus the unknown-escape sink at line 197 returning `None`); invoked exclusively from `lexer/quoted.rs:563` (`decode_and_push_escape` in `scan_double_quoted_line`); not called from `lexer/quoted.rs:415-455` (single-quoted scanner) — single-quoted scalars do not interpret `\` as an escape (the only "escape" is `''` for a literal single quote); not called from any block-scalar code path (`lexer/block.rs`); test `backslash_not_special` at `lexer/quoted.rs:1100` confirms `'foo\\nbar'` (single-quoted) returns the literal value `foo\\nbar` with no escape processing.
Reasoning: `c-ns-esc-char` is the union of all 20 escape forms, and the spec prose restricts escape interpretation to double-quoted scalars only. The parser's `decode_escape` is the single dispatch point implementing the union; it is invoked only from the double-quoted scanner, exactly as the spec requires. Single-quoted and block scalars treat `\` as a literal character, confirmed by the `backslash_not_special` test. The unknown-escape sink returns `None`, which `decode_and_push_escape` translates into "invalid escape sequence" — the spec's silent rejection of escapes outside the union is enforced as an explicit error. The conformance to the production is exact.
