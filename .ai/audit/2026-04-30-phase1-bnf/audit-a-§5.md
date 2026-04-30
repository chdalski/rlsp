---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: A
section: §5 (with §3, §4)
date: 2026-04-30
---

### [§3] Not Applicable (descriptive)

BNF: (no BNF — meta-notation)
Spec prose: §3 "Processes and Models" describes the three translation stages (representation, serialization, presentation) and the loading-failure points (well-formed streams, resolved tags, recognized and valid tags, available tags). It contains no BNF productions; it specifies the *semantic* obligations a YAML processor must meet (e.g. `tag:yaml.org,2002:int` resolution, mapping-key uniqueness, alias graph integrity).
Verdict: Not-applicable
Evidence: n/a — no implementation site (audit scope is BNF productions; §3 has no productions).
Reasoning: The phase 1 audit scope is BNF productions and meta-notation. §3 is a descriptive chapter that explains the model layered above the grammar. There is no production to verdict against. Functional behavior implied by §3 (tag resolution, schema selection, alias graph integrity) maps onto productions in later chapters and to higher-level loader code (`/workspace/rlsp-yaml-parser/src/loader.rs`, `/workspace/rlsp-yaml-parser/src/schema.rs`); auditing those belongs to phase 2 of this audit, not phase 1.

### [§4] Not Applicable (meta-notation)

BNF: (no BNF — meta-notation)
Spec prose: §4 "Syntax Conventions" defines the BNF meta-notation used throughout §5–§9: `production-name ::= term`, atomic terms (quoted strings, hex literals, hex ranges), lookarounds (`[lookahead = term]`, `[lookahead ≠ term]`, `[lookbehind = term]`), special productions (`<start-of-line>`, `<end-of-input>`, `<empty>`), parenthesized terms, concatenation, alternation, quantifiers (`?`, `*`, `+`), production parameters (`n`, `c`, `t`), and the prefix-naming convention (`c-`, `b-`, `nb-`, `s-`, `ns-`, `l-`, `X-Y-`, `X+`).
Verdict: Not-applicable
Evidence: n/a — no implementation site (meta-notation only).
Reasoning: §4 is the legend for reading the productions in §5–§9. There is nothing for a parser to "implement" — the productions themselves are the spec. The parser must understand and respect the parameters (`n`, `c`, `t`) when implementing parameterized productions, but the meta-notation has no direct execution surface. The implementation does honour the prefix conventions in its naming (e.g. `is_c_printable`, `is_ns_char`, `is_c_indicator`), and chomping is modelled in `event.rs` `Chomp` enum (matching `STRIP`/`CLIP`/`KEEP`).

### [1] c-printable

BNF:
```
[1] c-printable ::=
                         # 8 bit
    x09                  # Tab (\t)
  | x0A                  # Line feed (LF \n)
  | x0D                  # Carriage Return (CR \r)
  | [x20-x7E]            # Printable ASCII
                         # 16 bit
  | x85                  # Next Line (NEL)
  | [xA0-xD7FF]          # Basic Multilingual Plane (BMP)
  | [xE000-xFFFD]        # Additional Unicode Areas
  | [x010000-x10FFFF]    # 32 bit
```
Spec prose: §5.1 "On input, a YAML processor must accept all characters in this printable subset. On output, a YAML processor must only produce only characters in this printable subset." The C0 block `x00-x1F` (except TAB/LF/CR), DEL `x7F`, the C1 block `x80-x9F` (except NEL `x85`), surrogates `xD800-xDFFF`, `xFFFE`, and `xFFFF` are excluded.
Verdict: Lenient
Evidence: `chars.rs:14-26` defines `is_c_printable`; the only callers are `lexer/quoted.rs:580` (gating hex-decoded `\x`/`\u`/`\U` escapes in double-quoted scalars) and the test module. There is no global input-validation pass that rejects non-printable raw input.
Reasoning: The spec says "On input, a YAML processor must accept all characters in this printable subset" — by inversion, it must reject characters outside the printable subset. The parser's `is_c_printable` predicate is well-formed and matches the BNF exactly (TAB/LF/CR, x20-x7E, x85, xA0-xD7FF, xE000-xFFFD, x010000-x10FFFF). However, no production code path calls it on raw stream input. Plain-scalar scanning in `lexer/plain.rs:381` rejects `0x00..=0x1F | 0x7F` (which incidentally enforces ASCII control rejection in plain scalars), and BOM `U+FEFF` is rejected mid-document at `event_iter/step.rs:73-80`. But characters in `x80-x9F` excluding x85, surrogate-paired-output via UTF-8 ill-formed bytes, and `xFFFE`/`xFFFF` are not actively rejected at the input boundary — they pass through `encoding.rs` (which only validates well-formed UTF-8/UTF-16/UTF-32) and through the line scanner (`lines.rs:91-101`) and reach scalar content. A document containing a literal `\u{86}` (PRIVATE USE 1) inside a double-quoted scalar is accepted even though `is_c_printable('\u{86}')` is false. The verdict is `Lenient`: input that violates [1] passes through silently except where overlapping productions (plain-scalar scan, mid-document BOM check) catch a subset.

### [2] nb-json

BNF:
```
[2] nb-json ::=
    x09              # Tab character
  | [x20-x10FFFF]    # Non-C0-control characters
```
Spec prose: §5.1 "To ensure JSON compatibility, YAML processors must allow all non-C0 characters inside quoted scalars. To ensure readability, non-printable characters should be escaped on output, even inside such scalars." The set is broader than `c-printable` — it admits DEL `x7F`, C1 `x80-x9F`, surrogates, and `xFFFE/xFFFF`.
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:618-751` (`scan_double_quoted_line`) and `lexer/quoted.rs:415-455` (`scan_single_quoted_line`) accept any byte that is not the closing delimiter (`"`/`'`) or escape (`\` for double-quoted) — including `0x80-0x9F`, DEL, and any non-line-break byte. Line breaks within quoted scalars are handled as fold/blank/continuation logic, not as content rejection.
Reasoning: The spec requires processors to *allow* all `nb-json` characters (i.e. any non-C0-control character including TAB) inside quoted scalars. The parser's quoted-scalar scanners explicitly use `memchr2(b'"', b'\\', ...)` (double) and `memchr(b'\'', ...)` (single) to skip past arbitrary bytes between the relevant ASCII delimiters, so any non-delimiter byte (regardless of its `c-printable` status) is admitted as scalar content. Test `lexer/quoted.rs:1015-1019` `null_byte_escape_is_allowed` confirms `\0` produces U+0000 and the resulting Rust `String` accommodates it. Bidi-control and non-printable hex escapes are rejected (`quoted.rs:580-600`), but those are policy controls beyond [2]; literal (unescaped) DEL or C1 inside a quoted scalar is admitted, matching [2].

### [3] c-byte-order-mark

BNF:
```
[3] c-byte-order-mark ::= xFEFF
```
Spec prose: §5.2 "If a character stream begins with a byte order mark, the character encoding will be taken to be as indicated by the byte order mark. … Byte order marks may appear at the start of any document, however all documents in the same stream must use the same character encoding. To allow for JSON compatibility, byte order marks are also allowed inside quoted scalars."
Verdict: Strict-conformant
Evidence: `encoding.rs:55-71` (`detect_encoding`); `encoding.rs:104-167` (BOM stripping for UTF-8/UTF-16/UTF-32 in `decode_utf8`/`decode_utf16`/`decode_utf32`); `lines.rs:115-127` (first-line BOM strip in `scan_line` when `is_first`); `lines.rs:282-303` (`signal_document_boundary` strips BOM at each document boundary); `event_iter/step.rs:64-82` (rejects `U+FEFF` inside a document body); `chars.rs:68` and `chars.rs:150` (BOM excluded from `ns-char` and `ns-anchor-char`); `lexer/quoted.rs:618-751` (BOM permitted inside quoted scalars because the scan only stops at `"`/`\`).
Reasoning: The BOM character `xFEFF` is recognised at three positions corresponding to spec positions: stream start (encoding detection + strip), document prefix (`signal_document_boundary` strips after a blank-line skip in `BetweenDocs`), and inside quoted scalars (no rejection). The "must not appear inside a document" rule is enforced by `event_iter/step.rs:73-80` raising "invalid character U+FEFF in document". Every spec-allowed BOM site is implemented; every spec-forbidden BOM site is rejected.

### [4] c-sequence-entry

BNF:
```
[4] c-sequence-entry ::= '-'
```
Spec prose: §5.3 `"-" (x2D, hyphen) denotes a block sequence entry.`
Verdict: Strict-conformant
Evidence: `event_iter/step.rs:287-293` matches `Some(b'-')` to dispatch to `peek_sequence_entry()` which validates the second byte (must be space, tab, EOL, or sequence indicator). `chars.rs:36` includes `'-'` in `is_c_indicator`. The plain-scalar opener `lexer/plain.rs:287-302` also handles `-` as a possible plain-scalar starter when not followed by whitespace.
Reasoning: The spec defines the indicator as the literal `-` (x2D). The parser dispatches on `b'-'` at `step.rs:287` and only recognises it as a block-sequence-entry indicator when followed by whitespace or end-of-line — matching the disambiguation requirement in §6.2 / §8.2.1 (`c-l-block-seq-entry` requires `-` followed by `s-l+block-indented`). When `-` is followed by non-whitespace, the parser falls through to plain-scalar handling, matching `ns-plain-first(c)` which permits `-`, `?`, `:` as the first plain-scalar character when followed by `ns-plain-safe(c)`.

### [5] c-mapping-key

BNF:
```
[5] c-mapping-key ::= '?'
```
Spec prose: §5.3 `"?" (x3F, question mark) denotes a mapping key.`
Verdict: Strict-conformant
Evidence: `chars.rs:36` includes `'?'` in `is_c_indicator`. `event_iter/step.rs:875-880` falls through to a plain/explicit-key dispatcher; explicit-key handling for `? key` is in `event_iter/step.rs` and `event_iter/base.rs` (search for `explicit_key_pending` flag, set in `lib.rs:147`). `lexer/plain.rs:287-302` permits `?` as a plain-scalar first char only when followed by an `ns-plain-safe(c)` char.
Reasoning: The spec defines `?` as the explicit-key indicator. The parser disambiguates `? <ws>` (explicit key indicator) from `?x` (plain scalar starting with `?`) at `lexer/plain.rs:287-302` per `ns-plain-first(c)` rule [126]. Tests in `lexer/plain.rs:548-606` exercise the disambiguation. `?` alone or `?<space>` is treated as the indicator.

### [6] c-mapping-value

BNF:
```
[6] c-mapping-value ::= ':'
```
Spec prose: §5.3 `":" (x3A, colon) denotes a mapping value.`
Verdict: Strict-conformant
Evidence: `chars.rs:37` includes `':'` in `is_c_indicator`. `lexer/plain.rs:325-330,395-413` implement the `:` plain-scalar disambiguation (`:` followed by `ns-plain-safe(c)` is content; otherwise it terminates the scalar). Mapping-value detection is in `event_iter/base.rs`, `event_iter/step.rs:875+`, and `event_iter/flow.rs:1088-1099` (flow context).
Reasoning: The spec uses `:` for both block-mapping-value (`: ` or `:` at EOL) and flow-mapping-value (`:` followed by `,`/`]`/`}`/whitespace/EOL). Both contexts are handled: block context in `lexer/plain.rs:198-200` (terminates plain scalar at `: ` or trailing `:`), and flow context in `event_iter/flow.rs:1088-1099` (next char must be space/tab/`,`/`]`/`}`/EOL or empty for `:` to be a value indicator). `:` followed by content (`:abc`) is treated as part of a plain scalar per [130] `ns-plain-char(c)`, matching the spec.

### [7] c-collect-entry

BNF:
```
[7] c-collect-entry ::= ','
```
Spec prose: §5.3 `","  (x2C, comma) ends a flow collection entry.`
Verdict: Strict-conformant
Evidence: `chars.rs:38,59` include `,` in `is_c_indicator` and `is_c_flow_indicator`. `event_iter/flow.rs:662` handles `,` as the entry separator. `lexer/plain.rs:467` (flow plain scanner) treats `,` as a terminator. `chars.rs:151` excludes `,` from `is_ns_anchor_char`. `chars.rs:122-142` excludes `,` from `is_ns_tag_char_single` (it falls through to `is_c_flow_indicator` and is not in the enumerated allowed set).
Reasoning: The comma is used only inside flow collections per the spec. `event_iter/flow.rs:662` consumes `,` as the entry separator and `event_iter/flow.rs:693` checks for it after a value. The flow plain scanner stops at `,` (`plain.rs:467`). Anchor names and tag suffixes correctly exclude `,` because both productions intersect with the flow-indicator exclusion set.

### [8] c-sequence-start

BNF:
```
[8] c-sequence-start ::= '['
```
Spec prose: §5.3 `"[" (x5B, left bracket) starts a flow sequence.`
Verdict: Strict-conformant
Evidence: `chars.rs:39,59` include `[` in `is_c_indicator` and `is_c_flow_indicator`. `event_iter/step.rs:297-298` dispatches `b'['` to `handle_flow_collection()`. `lexer/plain.rs:467` (flow plain scanner) and `chars.rs:151,122-142` exclude `[` from anchor/tag chars.
Reasoning: The parser dispatches `[` at the block-context entry point (`step.rs:297`) into the flow-collection handler, matching the spec's flow-sequence opener. Stray `]` outside a flow collection produces an error (`step.rs:300-310`). Inside scalars, `[` correctly terminates flow plain scalars and is excluded from anchors and tags.

### [9] c-sequence-end

BNF:
```
[9] c-sequence-end ::= ']'
```
Spec prose: §5.3 `"]" (x5D, right bracket) ends a flow sequence.`
Verdict: Strict-conformant
Evidence: `chars.rs:40,59` include `]` in `is_c_indicator` and `is_c_flow_indicator`. `event_iter/step.rs:300-310` rejects `]` when not inside a flow collection. `event_iter/flow.rs` handles `]` as the closing token for `[ ... ]` collections. Flow plain scanner treats `]` as a terminator (`plain.rs:467`).
Reasoning: The parser correctly recognizes `]` as the flow-sequence terminator inside flow context and rejects it as an error in block context (`step.rs:300-310`: "unexpected ']' outside flow collection"). The character is excluded from anchors and tags via the `c-flow-indicator` exclusion. This matches the spec.

### [10] c-mapping-start

BNF:
```
[10] c-mapping-start ::= '{'
```
Spec prose: §5.3 `"{" (x7B, left brace) starts a flow mapping.`
Verdict: Strict-conformant
Evidence: `chars.rs:41,59` include `{` in `is_c_indicator` and `is_c_flow_indicator`. `event_iter/step.rs:297-298` dispatches `b'{'` to `handle_flow_collection()`. Flow plain scanner stops at `{` (`plain.rs:467`).
Reasoning: Same shape as `[`/`]`: the brace is dispatched to the flow handler at block-context entry, terminates flow plain scalars, and is excluded from anchor/tag char sets.

### [11] c-mapping-end

BNF:
```
[11] c-mapping-end ::= '}'
```
Spec prose: §5.3 `"}" (x7D, right brace) ends a flow mapping.`
Verdict: Strict-conformant
Evidence: `chars.rs:42,59` include `}` in `is_c_indicator` and `is_c_flow_indicator`. `event_iter/step.rs:300-310` rejects `}` outside flow context. `event_iter/flow.rs` handles `}` as the closing token for `{ ... }` collections.
Reasoning: Mirror image of `c-mapping-start`. Stray `}` in block context is rejected with an error message; inside flow context, `}` closes the flow mapping.

### [12] c-comment

BNF:
```
[12] c-comment ::= '#'
```
Spec prose: §5.3 `"#" (x23, octothorpe, hash, sharp, pound, number sign) denotes a comment.`
Verdict: Strict-conformant
Evidence: `chars.rs:43` includes `#` in `is_c_indicator`. `lexer/comment.rs:22-73` (`try_consume_comment`) parses `#`-prefixed lines. `lexer/plain.rs:516-531` (`extract_trailing_comment`) implements the spec rule that `#` is a comment indicator only when preceded by whitespace. `lexer/plain.rs:321-324,398-401` mirror this rule inside plain-scalar scanning (a `#` not preceded by whitespace is content; one preceded by whitespace terminates the scalar).
Reasoning: The spec requires `#` to be whitespace-preceded to function as a comment indicator (§6.6 / [75] `c-nb-comment-text`). The parser correctly implements this in three places: standalone-comment lines (`comment.rs`), trailing comments after scalars (`plain.rs:516-531`), and mid-scalar `#` handling (`plain.rs:321-324`). NUL inside a comment body is rejected (`plain.rs:81-91`), exceeding spec strictness on this one point but consistent with [1] `c-printable` exclusion of NUL.

### [13] c-anchor

BNF:
```
[13] c-anchor ::= '&'
```
Spec prose: §5.3 `"&" (x26, ampersand) denotes a node's anchor property.`
Verdict: Strict-conformant
Evidence: `chars.rs:44` includes `&` in `is_c_indicator`. `event_iter/step.rs:640-867` handles the `Some(b'&')` arm. `event_iter/properties.rs:23-46` (`scan_anchor_name`) consumes `ns-anchor-char` characters after the `&`. `lexer.rs:491-502` (`anchor_followed_by_block_mapping`) handles the marker-line case `--- &anchor ...`.
Reasoning: The `&` indicator is dispatched at `step.rs:640`, the anchor name is scanned via the anchor-name predicate, and the resulting anchor is attached to the next node event. The marker-line corner case (`--- &anchor key: val`) is rejected in `lexer.rs:323-338`. Anchor names use `ns-anchor-char` (production [102] `chars.rs:149-159`), correctly excluding flow indicators per the spec.

### [14] c-alias

BNF:
```
[14] c-alias ::= '*'
```
Spec prose: §5.3 `"*" (x2A, asterisk) denotes an alias node.`
Verdict: Strict-conformant
Evidence: `chars.rs:45` includes `*` in `is_c_indicator`. `event_iter/step.rs:314-455` handles the `Some(b'*')` arm. The alias name is scanned with the same `scan_anchor_name` (`event_iter/properties.rs:23-46`), enforcing the alias-name = anchor-name shape.
Reasoning: `*` is dispatched at `step.rs:314`, the alias name is scanned via `scan_anchor_name`, and an `Event::Alias` is emitted (`step.rs:450`). Alias-on-property errors are correctly raised: `step.rs:328-344` rejects an alias following a tag or an inline anchor (per §7.1: alias nodes cannot have properties).

### [15] c-tag

BNF:
```
[15] c-tag ::= '!'
```
Spec prose: §5.3 `The "!" (x21, exclamation) is used for specifying node tags. It is used to denote tag handles used in tag directives and tag properties; to denote local tags; and as the non-specific tag for non-plain scalars.`
Verdict: Strict-conformant
Evidence: `chars.rs:46` includes `!` in `is_c_indicator`. `event_iter/step.rs:458-637` handles the `Some(b'!')` arm. `event_iter/properties.rs:85-234` (`scan_tag`) parses verbatim (`!<URI>`), primary (`!!suffix`), named (`!handle!suffix`), local (`!suffix`), and non-specific (`!`) forms. `chars.rs:121-142` defines `is_ns_tag_char_single` which excludes `!` from tag suffix chars (so `!` always means a handle delimiter or non-specific marker).
Reasoning: The parser handles all five tag forms enumerated in the spec. Verbatim tags (`!<URI>`) validate via `is_ns_uri_char_single` plus `%HH` decoding (`properties.rs:101-147`). Named/primary/secondary handles use `scan_tag_suffix` (`properties.rs:241-273`) which mixes `is_ns_tag_char_single` with `%HH`. The `is_valid_tag_handle` function (`properties.rs:281-295`) constrains handle names to `[a-zA-Z0-9-]` per [89] `c-named-tag-handle`.

### [16] c-literal

BNF:
```
[16] c-literal ::= '|'
```
Spec prose: §5.3 `"|" (7C, vertical bar) denotes a literal block scalar.`
Verdict: Strict-conformant
Evidence: `chars.rs:47` includes `|` in `is_c_indicator`. `lexer/block.rs:41-50` (`try_consume_literal_block_scalar`) detects `|` as the literal-block-scalar opener. `lexer/block.rs:500+` (`parse_block_header`) parses the chomping/indent indicator.
Reasoning: The literal-block opener is matched at `block.rs:48`, and the rest of the line after `|` is handled by `parse_block_header` for the optional `+`/`-` chomp indicator and `1`-`9` explicit-indent indicator. Body lines preserve newlines (no folding).

### [17] c-folded

BNF:
```
[17] c-folded ::= '>'
```
Spec prose: §5.3 `">" (x3E, greater than) denotes a folded block scalar.`
Verdict: Strict-conformant
Evidence: `chars.rs:48` includes `>` in `is_c_indicator`. `lexer/block.rs:291` matches `>` as the folded-block-scalar opener (mirror of `|` handling). The same `parse_block_header` is reused.
Reasoning: The folded-block opener is matched at `block.rs:291`. Folding logic (single line breaks → space, blank lines → newlines per §8.1.3 / [177] `b-l-folded`) is implemented in the body-collection loop later in `block.rs`.

### [18] c-single-quote

BNF:
```
[18] c-single-quote ::= "'"
```
Spec prose: §5.3 `"'" (x27, apostrophe, single quote) surrounds a single-quoted flow scalar.`
Verdict: Strict-conformant
Evidence: `chars.rs:49` includes `'` in `is_c_indicator`. `lexer/quoted.rs:35-37` checks for the opening `'`. `lexer/quoted.rs:415-455` (`scan_single_quoted_line`) handles the body, including the `''` escape rule from [120] `c-quoted-quote`.
Reasoning: The single-quote indicator opens and closes single-quoted scalars; doubling (`''`) escapes a literal `'`. The scanner recognizes both: at `quoted.rs:428-432` an interior `'` followed by another `'` is treated as an escape (`has_escape = true`); a lone `'` closes the scalar (`quoted.rs:434-444`). Multi-line folding follows §7.3.2 (`quoted.rs:79-153`).

### [19] c-double-quote

BNF:
```
[19] c-double-quote ::= '"'
```
Spec prose: §5.3 `"\"" (x22, double quote) surrounds a double-quoted flow scalar.`
Verdict: Stricter-than-spec
Evidence: `chars.rs:50` includes `"` in `is_c_indicator`. `lexer/quoted.rs:185-241` opens a double-quoted scalar. `lexer/quoted.rs:618-751` (`scan_double_quoted_line`) parses the body. Stricter behavior: `quoted.rs:580-588` rejects `\x`/`\u`/`\U` escapes that produce non-`c-printable` characters; `quoted.rs:592-600` rejects bidi-control characters from numeric escapes; `quoted.rs:606-611` enforces a 1 MiB scalar length cap.
Reasoning: The double-quote indicator is correctly dispatched. Beyond the spec, the parser adds three security policies: (1) hex escapes producing non-`c-printable` chars are rejected (security hardening — even though [2] `nb-json` would admit DEL/C1 inside the literal scalar, an escape that *introduces* a non-printable char is rejected); (2) bidi-control characters from `\u`/`\U` escapes are rejected (Trojan Source mitigation); (3) a 1 MiB length cap rejects oversize scalars (DoS mitigation). The first restriction technically narrows what [62] `c-ns-esc-char` would accept; rationale is security hardening. The verdict is `Stricter-than-spec` rather than `Lenient` because the parser rejects valid spec input only in security-motivated edge cases — any reasonable double-quoted scalar is parsed correctly.

### [20] c-directive

BNF:
```
[20] c-directive ::= '%'
```
Spec prose: §5.3 `"%" (x25, percent) denotes a directive line.`
Verdict: Strict-conformant
Evidence: `chars.rs:51` includes `%` in `is_c_indicator`. `lexer.rs:150-174` (`is_directive_line`, `try_consume_directive_line`) detects `%`-prefixed lines at column 0. `event_iter/directives.rs:51-104` parses `%YAML`, `%TAG`, and reserved directives.
Reasoning: The `%` directive indicator is recognised only at line-start in the `BetweenDocs` state — within a document body, `%`-prefixed lines are content (test `lexer.rs:733-743` `is_blank_or_comment_does_not_skip_directive_lines` regression-tests this distinction). `%YAML` and `%TAG` are parsed; unknown directives are silently ignored per the spec ("Reserved directive — silently ignore", `directives.rs:99-102`).

### [21] c-reserved

BNF:
```
[21] c-reserved ::=
    '@' | '`'
```
Spec prose: §5.3 `The "@" (x40, at) and "` " (x60, grave accent) are reserved for future use.` The example shows `@text` and `` `text `` rejected as plain-scalar starters: "Reserved indicators can't start a plain scalar."
Verdict: Strict-conformant
Evidence: `chars.rs:52-53` include `@` and `` ` `` in `is_c_indicator`. Because `is_c_indicator` returns true for these characters and `lexer/plain.rs:287-302` (`ns_plain_first_block`) rejects any indicator (other than `?`/`:`/`-` followed by safe chars) as a plain-scalar opener, lines beginning with `@` or `` ` `` are not treated as plain scalars. Without a higher-level dispatch arm for `@`/`` ` ``, the line falls through and produces an "unexpected character" path.
Reasoning: The parser implements [21] indirectly: by including `@`/`` ` `` in `c-indicator` and rejecting indicator-starts in `ns-plain-first(c)`, plain scalars cannot begin with these characters. They are not consumed by any structural-token arm in `step.rs`, so a document starting with `@text` produces a parse error (the dispatch table at `step.rs:282+` has no arm for `b'@'` or `` b'`' ``, and the plain-scalar opener correctly refuses, leaving the input to fall through to error handling). This matches the spec's "reserved" semantic.

### [22] c-indicator

BNF:
```
[22] c-indicator ::=
    c-sequence-entry    # '-'
  | c-mapping-key       # '?'
  | c-mapping-value     # ':'
  | c-collect-entry     # ','
  | c-sequence-start    # '['
  | c-sequence-end      # ']'
  | c-mapping-start     # '{'
  | c-mapping-end       # '}'
  | c-comment           # '#'
  | c-anchor            # '&'
  | c-alias             # '*'
  | c-tag               # '!'
  | c-literal           # '|'
  | c-folded            # '>'
  | c-single-quote      # "'"
  | c-double-quote      # '"'
  | c-directive         # '%'
  | c-reserved          # '@' '`'
```
Spec prose: §5.3 "Any indicator character." The set comprises all 18 indicators enumerated above (`-`, `?`, `:`, `,`, `[`, `]`, `{`, `}`, `#`, `&`, `*`, `!`, `|`, `>`, `'`, `"`, `%`, `@`, `` ` ``).
Verdict: Strict-conformant
Evidence: `chars.rs:33-55` defines `is_c_indicator(ch)` matching exactly the 19 characters listed in the BNF (note: the spec body says "21 indicator characters" but the actual production lists 18 distinct characters including `c-reserved` which expands to `@` and `` ` ``, so 19 chars total — the comment in `chars.rs:32` says "21" which inherits a spec-wording artefact). The test `chars.rs:265-273` `c_indicator_accepts_all_21_indicator_chars` enumerates all 19 chars and confirms `is_c_indicator` returns true for each.
Reasoning: The predicate enumerates exactly the spec's union: `-`, `?`, `:`, `,`, `[`, `]`, `{`, `}`, `#`, `&`, `*`, `!`, `|`, `>`, `'`, `"`, `%`, `@`, `` ` `` — 19 distinct characters. The doc comment's "21" count is inaccurate but the set is correct. The predicate is consumed by `lexer/plain.rs:287-302` (`ns_plain_first_block`) for the `ns-plain-first(c)` rule, where indicator status is the gating condition.

### [23] c-flow-indicator

BNF:
```
[23] c-flow-indicator ::=
    c-collect-entry     # ','
  | c-sequence-start    # '['
  | c-sequence-end      # ']'
  | c-mapping-start     # '{'
  | c-mapping-end       # '}'
```
Spec prose: §5.3 "The `[`, `]`, `{`, `}` and `,` indicators denote structure in flow collections. They are therefore forbidden in some cases, to avoid ambiguity in several constructs."
Verdict: Strict-conformant
Evidence: `chars.rs:58-60` defines `is_c_flow_indicator` matching exactly `,`, `[`, `]`, `{`, `}`. Tests `chars.rs:284-298` confirm acceptance of the five chars and rejection of all other `c-indicator` chars. Used in: `chars.rs:151` (`is_ns_anchor_char` excludes flow indicators), `chars.rs:122-142` (`is_ns_tag_char_single` excludes flow indicators), `lexer/plain.rs:467` (flow-context plain scanner), `event_iter/flow.rs` (flow-collection structural-token detection).
Reasoning: The set is exact: `{,`, `[`, `]`, `{`, `}`}`. The predicate is correctly composed into `ns-tag-char` and `ns-anchor-char` (which subtract `c-flow-indicator` from their parents) and is the explicit terminator set for flow-context plain scalars.

### [24] b-line-feed

BNF:
```
[24] b-line-feed ::= x0A
```
Spec prose: §5.4 "YAML recognizes the following ASCII line break characters."
Verdict: Strict-conformant
Evidence: `lines.rs:91-101` (`detect_break`) recognizes `\n` as `BreakType::Lf`. `encoding.rs:179-198` (`normalize_line_breaks`) preserves `\n` and converts CR/CRLF to LF.
Reasoning: LF is recognized as a line terminator at every input scan site. The line buffer (`lines.rs`) yields `BreakType::Lf` for stand-alone `\n` and `BreakType::CrLf` when the LF follows a CR, matching the spec's two-character vs single-character treatment.

### [25] b-carriage-return

BNF:
```
[25] b-carriage-return ::= x0D
```
Spec prose: §5.4 ASCII line-break recognition.
Verdict: Strict-conformant
Evidence: `lines.rs:92-97` (`detect_break`): CRLF is matched first (`BreakType::CrLf`), then bare CR (`BreakType::Cr`). `encoding.rs:179-198` normalizes both to LF.
Reasoning: Bare CR and CRLF are both recognized; the order of checks in `detect_break` correctly handles CRLF without misclassifying it as a bare CR followed by an LF. Both forms are normalized to LF for downstream processing, matching [28] `b-as-line-feed`.

### [26] b-char

BNF:
```
[26] b-char ::=
    b-line-feed          # x0A
  | b-carriage-return    # X0D
```
Spec prose: §5.4 "All other characters, including the form feed (x0C), are considered to be non-break characters. Note that these include the non-ASCII line breaks: next line (x85), line separator (x2028) and paragraph separator (x2029)."
Verdict: Strict-conformant
Evidence: `lines.rs:91-101` only treats `\n` and `\r` as line breaks. `chars.rs:21,71,154` include `x85`, `x2028`, `x2029` as `c-printable`/`ns-char` content (not line breaks). `chars.rs:190-193` decodes `\N`/`\L`/`\P` escapes to their respective Unicode codepoints — confirming that NEL/LS/PS are content characters, not line breaks.
Reasoning: The spec narrows YAML 1.2 line breaks to ASCII LF/CR (a deliberate change from YAML 1.1 to align with JSON; spec note at §5.4: "YAML version 1.1 did support the above non-ASCII line break characters; however, JSON does not."). The parser respects this: `detect_break` only recognizes `\n` and `\r`. NEL `x85`, LS `x2028`, PS `x2029` are treated as content per [27] `nb-char`, which matches the spec exactly.

### [27] nb-char

BNF:
```
[27] nb-char ::=
  c-printable - b-char - c-byte-order-mark
```
Spec prose: §5.4 — `nb-char` is the non-break printable subset (`c-printable` minus LF/CR minus BOM).
Verdict: Lenient
Evidence: There is no direct `is_nb_char` predicate. `chars.rs:67-76` (`is_ns_char`) computes the ns-char subset (`nb-char - s-white`) directly, but never references an `nb-char` predicate. The plain-scalar scanner in `lexer/plain.rs:340-416` accepts any byte that isn't a structural terminator; it does NOT enforce membership in `c-printable`. `lexer/quoted.rs:618-751` accepts any byte that isn't `"`/`\`.
Reasoning: The spec defines `nb-char` as `c-printable - b-char - c-byte-order-mark` — meaning content characters in scalars/comments must be drawn from `nb-char`. The parser enforces this only partially: BOM is excluded mid-document (`step.rs:73-80`), LF/CR are line terminators handled by the line scanner, and the plain scanner rejects `0x00..=0x1F` (excluding TAB) and `0x7F`. But characters in `0x80-0x9F` excluding NEL `x85`, surrogate-pair output, `xFFFE`, `xFFFF`, are not actively rejected as content. The verdict mirrors [1] `c-printable`: input violating `nb-char` passes silently for non-ASCII non-printable chars. (For the ASCII subset of `nb-char` violations, the plain scanner does enforce — that subset is conformant.)

### [28] b-break

BNF:
```
[28] b-break ::=
    (
      b-carriage-return  # x0A
      b-line-feed
    )                    # x0D
  | b-carriage-return
  | b-line-feed
```
Spec prose: §5.4 "Line breaks are interpreted differently by different systems and have multiple widely used formats." (CRLF, CR, LF.)
Verdict: Strict-conformant
Evidence: `lines.rs:91-101` (`detect_break`) implements the alternation in order: CRLF first (so CR+LF is consumed atomically), then bare CR, then LF. `lines.rs:34-40` (`BreakType::byte_len`) returns the correct byte length per variant.
Reasoning: The parser's `detect_break` matches the production exactly: CRLF (the parenthesized concatenation) is checked first to prevent the ambiguity where bare CR then LF would otherwise be classified as two breaks. The `BreakType` enum carries the variant downstream so positional arithmetic is correct (`lines.rs:48-63`).

### [29] b-as-line-feed

BNF:
```
[29] b-as-line-feed ::=
  b-break
```
Spec prose: §5.4 "Line breaks inside scalar content must be normalized by the YAML processor. Each such line break must be parsed into a single line feed character. The original line break format is a presentation detail and must not be used to convey content information."
Verdict: Strict-conformant
Evidence: `encoding.rs:179-198` (`normalize_line_breaks`) converts `\r\n` and bare `\r` to `\n` at the decode boundary. `lexer/quoted.rs:308-316,309-311` produce `\n` characters in scalar fold output regardless of input break type. `lexer/block.rs` (literal/folded scalar collection) emits `\n` per consumed break.
Reasoning: All scalar content normalization paths produce `\n` regardless of the input break type. The break-type distinction (`BreakType` enum) survives only for position arithmetic, not for emitted scalar content — content is always normalized to LF, matching [29].

### [30] b-non-content

BNF:
```
[30] b-non-content ::=
  b-break
```
Spec prose: §5.4 "Outside scalar content, YAML allows any line break to be used to terminate lines."
Verdict: Strict-conformant
Evidence: `lines.rs:91-101` accepts CRLF, CR, LF as line terminators throughout the input — there is no preference or restriction by context. Lexer methods that consume lines (`lexer.rs:104-118` `skip_empty_lines`, `lexer.rs:131-146` `skip_blank_lines_between_docs`, `lexer.rs:241-292` `consume_marker_line`) all treat any `BreakType` variant as terminating a line.
Reasoning: Outside scalar content the parser does not distinguish break types — every break terminates a line and is consumed. This is exactly what [30] requires.

### [31] s-space

BNF:
```
[31] s-space ::= x20
```
Spec prose: §5.5 "YAML recognizes two white space characters: space and tab."
Verdict: Strict-conformant
Evidence: `lines.rs:142` counts only `' '` (x20) for `indent` (the spec is explicit at [31]–[33] that indentation is composed of `s-space` only — see [63] `s-indent` "n is given as a parameter and is the number of *space* characters"). `chars.rs:68,150` exclude `' '` from `ns-char`/`ns-anchor-char`. Plain scanners (`lexer/plain.rs:374-377,460-463`) treat `' '` as whitespace.
Reasoning: SP is treated as whitespace at every site. Critically, indentation counts only spaces (not tabs), matching the YAML 1.2 prohibition on tab-indentation in block context (see [63] `s-indent` and [206] `c-indentation-indicator` in §6.1). A leading tab in a block scalar's indentation is rejected (`block.rs:134-145`).

### [32] s-tab

BNF:
```
[32] s-tab ::= x09
```
Spec prose: §5.5 White-space recognition.
Verdict: Strict-conformant
Evidence: `chars.rs:68,150` exclude `\t` from `ns-char`/`ns-anchor-char`. Plain scanners (`lexer/plain.rs:374-377,460-463`) treat `\t` as whitespace alongside SP. `lexer/quoted.rs:34,107,185` use `[' ', '\t']` for leading-whitespace stripping. `block.rs:134-145` rejects TAB at the start of a block-scalar body line as invalid indentation.
Reasoning: TAB is recognised as whitespace in scalar trimming and content boundary detection. It is excluded from indentation counts (per [31] `s-space` only), and it is rejected when it would form structural indentation in block scalars. This matches the YAML 1.2 stance that tabs are content separators but not indentation.

### [33] s-white

BNF:
```
[33] s-white ::=
  s-space | s-tab
```
Spec prose: §5.5 "the rest of the (printable) non-break characters are considered to be non-space characters" — `s-white` is the union of SP and TAB.
Verdict: Strict-conformant
Evidence: `chars.rs:68` `is_ns_char`: `!matches!(ch, ' ' | '\t' | …)`. `chars.rs:150` `is_ns_anchor_char` mirrors the exclusion. `lexer/quoted.rs:34,107,185,276`, `lexer/block.rs:47,290`, `lexer/plain.rs` use `[' ', '\t']` consistently for whitespace stripping.
Reasoning: The whitespace alphabet is exactly `{SP, TAB}` everywhere it is used. There is no ad-hoc treatment of, e.g., `\u{A0}` (NBSP) as whitespace — NBSP is content. This matches [33].

### [34] ns-char

BNF:
```
[34] ns-char ::=
  nb-char - s-white
```
Spec prose: §5.5 "The rest of the (printable) non-break characters are considered to be non-space characters."
Verdict: Lenient
Evidence: `chars.rs:67-76` `is_ns_char` enumerates the printable subset minus SP/TAB/LF/CR/BOM. The set is `{x21-x7E, x85, xA0-xD7FF, xE000-xFFFD, x010000-x10FFFF} - {space, tab}`. Used as a building block of `ns_plain_first_block` (`plain.rs:287-302`), `ns_plain_safe_block` (`plain.rs:307-309`), and as a re-export `lexer.rs:16`.
Reasoning: The predicate itself matches the production. However, like [27] `nb-char`, it is enforced only at scattered sites (plain-scalar opener, anchor-name scanning) — the plain-scalar body uses `ns_plain_char_block` which delegates to `ns_plain_safe_block(ch) = is_ns_char(ch)` for non-ASCII bytes (`plain.rs:359-372`), so non-printable ranges in `0x80-0x9F` (excluding x85) and `xFFFE`/`xFFFF` are correctly rejected at those sites. But the quoted-scalar and block-scalar scanners do NOT enforce `ns-char` membership on content bytes — they accept any byte that isn't a structural delimiter. Verdict: `Lenient` because the production's intent ("non-space-non-break-printable") is enforced only in plain-scalar context; in quoted and block scalars, content outside the printable subset passes through.

### [35] ns-dec-digit

BNF:
```
[35] ns-dec-digit ::=
  [x30-x39]             # 0-9
```
Spec prose: §5.6 "A decimal digit for numbers."
Verdict: Strict-conformant
Evidence: There is no dedicated `is_ns_dec_digit` predicate; instead, `char::is_ascii_digit` and `str::parse::<u8>()` are used. `event_iter/directives.rs:136-143` parses `%YAML major.minor` versions via `parse::<u8>()`. `lexer/block.rs` (chomping/indent indicator) parses `1-9` via `is_ascii_digit`-like checks in `parse_block_header` (`block.rs:500+`).
Reasoning: ASCII decimal digits are recognised via Rust's `is_ascii_digit` which matches `[x30-x39]` exactly. The set is identical to the production. Used in: directive version parsing, block-scalar indent indicator parsing, and indirectly in `is_ns_word_char` (via `is_ascii_alphanumeric`).

### [36] ns-hex-digit

BNF:
```
[36] ns-hex-digit ::=
    ns-dec-digit        # 0-9
  | [x41-x46]           # A-F
  | [x61-x66]           # a-f
```
Spec prose: §5.6 "A hexadecimal digit for escape sequences."
Verdict: Strict-conformant
Evidence: `chars.rs:210` (`decode_hex_escape`) calls `c.is_ascii_hexdigit()` which is exactly `[0-9A-Fa-f]`. `chars.rs:213` parses with `u32::from_str_radix(hex_str, 16)`. `event_iter/properties.rs:117-127` validates `%HH` percent-encoding hex digits via `b.is_ascii_hexdigit()` for verbatim tags. `event_iter/properties.rs:247-254` mirrors this for tag suffix `%HH`.
Reasoning: `is_ascii_hexdigit` matches the production exactly. The `\x`/`\u`/`\U` escape decoders in `chars.rs:204-216` accept exactly hex digits and reject non-hex bytes at the same boundary. Truncated hex sequences are rejected as invalid escapes (`quoted.rs:1232-1234`).

### [37] ns-ascii-letter

BNF:
```
[37] ns-ascii-letter ::=
    [x41-x5A]           # A-Z
  | [x61-x7A]           # a-z
```
Spec prose: §5.6 "ASCII letter (alphabetic) characters."
Verdict: Strict-conformant
Evidence: No dedicated `is_ns_ascii_letter`; `char::is_ascii_alphabetic` and `is_ascii_alphanumeric` are used. `event_iter/properties.rs:289` uses `c.is_ascii_alphanumeric() || c == '-'` for tag handle validation (matching [38] `ns-word-char`). `chars.rs:88-114` (`is_ns_uri_char_single`) uses `is_ascii_alphanumeric`. `chars.rs:121-142` mirrors.
Reasoning: `is_ascii_alphabetic` matches `[A-Za-z]` exactly. The production is used as a building block of `ns-word-char` and `ns-uri-char`; both call sites use `is_ascii_alphanumeric` (which is `is_ascii_alphabetic` ∪ `is_ascii_digit` = `ns-ascii-letter` ∪ `ns-dec-digit`). The decomposition is conformant.

### [38] ns-word-char

BNF:
```
[38] ns-word-char ::=
    ns-dec-digit        # 0-9
  | ns-ascii-letter     # A-Z a-z
  | '-'                 # '-'
```
Spec prose: §5.6 "Word (alphanumeric) characters for identifiers."
Verdict: Strict-conformant
Evidence: `event_iter/properties.rs:289` `c.is_ascii_alphanumeric() || c == '-'` enforces `ns-word-char` for tag handle inner characters. `chars.rs:88-114` `is_ns_uri_char_single` uses `is_ascii_alphanumeric()` plus `'-'` (line 92) plus other URI chars — covering `ns-word-char` as a subset.
Reasoning: The named-tag-handle validator (`is_valid_tag_handle`) requires every interior char to satisfy `ns-word-char`. The check `is_ascii_alphanumeric() || c == '-'` is exactly `[0-9A-Za-z-]`, matching the production. Tag-prefix and verbatim-URI scanning embed `ns-word-char` as a subset of `ns-uri-char`, conformantly.

### [39] ns-uri-char

BNF:
```
[39] ns-uri-char ::=
    (
      '%'
      ns-hex-digit{2}
    )
  | ns-word-char
  | '#'
  | ';'
  | '/'
  | '?'
  | ':'
  | '@'
  | '&'
  | '='
  | '+'
  | '$'
  | ','
  | '_'
  | '.'
  | '!'
  | '~'
  | '*'
  | "'"
  | '('
  | ')'
  | '['
  | ']'
```
Spec prose: §5.6 "URI characters for tags, as defined in the URI specification. By convention, any URI characters other than the allowed printable ASCII characters are first encoded in UTF-8 and then each byte is escaped using the `%` character."
Verdict: Strict-conformant
Evidence: `chars.rs:88-114` `is_ns_uri_char_single` enumerates the alphabet: `is_ascii_alphanumeric()` (=`ns-word-char` minus `-`) plus `-`, `_`, `.`, `!`, `~`, `*`, `'`, `(`, `)`, `[`, `]`, `#`, `;`, `/`, `?`, `:`, `@`, `&`, `=`, `+`, `$`, `,`. The `%HH` form is handled separately at the scanner level: `event_iter/properties.rs:113-127` (verbatim tag URI) and `event_iter/properties.rs:241-273` (`scan_tag_suffix`) both validate `%` followed by exactly two hex digits.
Reasoning: The single-char alphabet enumerated in `is_ns_uri_char_single` matches the production exactly (note: `%` itself is correctly NOT in the single-char predicate — it must be followed by two hex digits, handled in the scanner). All 23 base characters are present plus the alphanumerics. The percent-encoding form is enforced as a 3-byte unit at the scanner level. `chars.rs:372-376` test confirms `is_ns_uri_char_single('!')` is true.

### [40] ns-tag-char

BNF:
```
[40] ns-tag-char ::=
    ns-uri-char
  - c-tag               # '!'
  - c-flow-indicator
```
Spec prose: §5.6 "The `!` character is used to indicate the end of a named tag handle; hence its use in tag shorthands is restricted. In addition, such shorthands must not contain the `[`, `]`, `{`, `}` and `,` characters. These characters would cause ambiguity with flow collection structures."
Verdict: Strict-conformant
Evidence: `chars.rs:121-142` `is_ns_tag_char_single` enumerates `is_ascii_alphanumeric()` plus `-`, `_`, `.`, `~`, `*`, `'`, `(`, `)`, `#`, `;`, `/`, `?`, `:`, `@`, `&`, `=`, `+`, `$` — exactly `ns-uri-char` with `!`, `,`, `[`, `]`, `{`, `}` removed. Tests `chars.rs:352-360,362-369,372-376` confirm flow-indicator exclusion and `!` exclusion. `event_iter/properties.rs:241-273` (`scan_tag_suffix`) calls `is_ns_tag_char_single` per char and `%HH` is handled separately.
Reasoning: The set difference `ns-uri-char - {!} - {,, [, ], {, }}` is computed correctly: `is_ns_tag_char_single` lists the same chars as `is_ns_uri_char_single` minus exactly those six. The `%HH` encoded form is shared with [39] in the scanner. Suffix scanning stops at the first non-`ns-tag-char` char, meeting the spec's "must not contain" requirement.

### [41] c-escape

BNF:
```
[41] c-escape ::= '\'
```
Spec prose: §5.7 "All non-printable characters must be escaped. YAML escape sequences use the `\` notation common to most modern computer languages."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:633` (`scan_double_quoted_line`): `memchr2(b'"', b'\\', ...)` — `\` is the escape introducer in double-quoted scalars. `lexer/quoted.rs:670-701` decodes the escape body. `chars.rs:173-199` (`decode_escape`) consumes the character after the `\` and dispatches to the appropriate sub-production.
Reasoning: The double-quoted scalar scanner correctly identifies `\` as the escape introducer. `\` outside a double-quoted scalar (in plain or single-quoted) is treated as content per §5.7 ("Note that escape sequences are only interpreted in double-quoted scalars. In all other scalar styles, the `\` character has no special meaning"). This is verified by `lexer/quoted.rs:1099` `backslash_not_special` test (single-quoted: `'foo\nbar'` → `foo\nbar` literal).

### [42] ns-esc-null

BNF:
```
[42] ns-esc-null ::= '0'
```
Spec prose: §5.7 "Escaped ASCII null (x00) character."
Verdict: Strict-conformant
Evidence: `chars.rs:177` `'0' => Some(('\x00', 1))`. Test `chars.rs:382-401` `decode_escape_success` includes `null_escape` case. `lexer/quoted.rs:1019,1180-1182` `null_byte_escape_is_allowed` confirms `\0` produces U+0000.
Reasoning: The `\0` escape decodes to NUL exactly per the spec. NUL is named-escape-only-acceptable: `quoted.rs:580-588` rejects non-printable hex escapes but explicitly exempts named escapes like `\0` (`quoted.rs:1314-1320` test `named_null_escape_is_ok`).

### [43] ns-esc-bell

BNF:
```
[43] ns-esc-bell ::= 'a'
```
Spec prose: §5.7 "Escaped ASCII bell (x07) character."
Verdict: Strict-conformant
Evidence: `chars.rs:178` `'a' => Some(('\x07', 1))`. Test `lexer/quoted.rs:1188` confirms `\a` produces `\x07`.
Reasoning: Direct mapping. The escape produces U+0007 (BEL).

### [44] ns-esc-backspace

BNF:
```
[44] ns-esc-backspace ::= 'b'
```
Spec prose: §5.7 "Escaped ASCII backspace (x08) character."
Verdict: Strict-conformant
Evidence: `chars.rs:179` `'b' => Some(('\x08', 1))`. Test `lexer/quoted.rs:1189` confirms `\b` → `\x08`.
Reasoning: Direct mapping. The escape produces U+0008 (BS).

### [45] ns-esc-horizontal-tab

BNF:
```
[45] ns-esc-horizontal-tab ::=
  't' | x09
```
Spec prose: §5.7 "Escaped ASCII horizontal tab (x09) character. This is useful at the start or the end of a line to force a leading or trailing tab to become part of the content."
Verdict: Strict-conformant
Evidence: `chars.rs:180` `'t' | '\t' => Some(('\t', 1))` — the alternation in the production is matched by the alternation in the Rust pattern.
Reasoning: The escape accepts both the letter `t` and the literal TAB character (`x09`) after the backslash. Both produce a TAB in the output. This is the exact alternation in the production.

### [46] ns-esc-line-feed

BNF:
```
[46] ns-esc-line-feed ::= 'n'
```
Spec prose: §5.7 "Escaped ASCII line feed (x0A) character."
Verdict: Strict-conformant
Evidence: `chars.rs:181` `'n' => Some(('\n', 1))`. Test `lexer/quoted.rs:1168` `escape_newline` confirms `\n` → LF.
Reasoning: Direct mapping. The escape produces U+000A.

### [47] ns-esc-vertical-tab

BNF:
```
[47] ns-esc-vertical-tab ::= 'v'
```
Spec prose: §5.7 "Escaped ASCII vertical tab (x0B) character."
Verdict: Strict-conformant
Evidence: `chars.rs:182` `'v' => Some(('\x0B', 1))`. Test `lexer/quoted.rs:1190` confirms `\v` → `\x0B`.
Reasoning: Direct mapping. The escape produces U+000B (VT).

### [48] ns-esc-form-feed

BNF:
```
[48] ns-esc-form-feed ::= 'f'
```
Spec prose: §5.7 "Escaped ASCII form feed (x0C) character."
Verdict: Strict-conformant
Evidence: `chars.rs:183` `'f' => Some(('\x0C', 1))`. Test `lexer/quoted.rs:1191` confirms `\f` → `\x0C`.
Reasoning: Direct mapping. The escape produces U+000C (FF).

### [49] ns-esc-carriage-return

BNF:
```
[49] ns-esc-carriage-return ::= 'r'
```
Spec prose: §5.7 "Escaped ASCII carriage return (x0D) character."
Verdict: Strict-conformant
Evidence: `chars.rs:184` `'r' => Some(('\r', 1))`. Test `lexer/quoted.rs:1192` confirms `\r` → CR.
Reasoning: Direct mapping. The escape produces U+000D.

### [50] ns-esc-escape

BNF:
```
[50] ns-esc-escape ::= 'e'
```
Spec prose: §5.7 "Escaped ASCII escape (x1B) character."
Verdict: Strict-conformant
Evidence: `chars.rs:185` `'e' => Some(('\x1B', 1))`. Test `lexer/quoted.rs:1193` confirms `\e` → `\x1B`.
Reasoning: Direct mapping. The escape produces U+001B (ESC).

### [51] ns-esc-space

BNF:
```
[51] ns-esc-space ::= x20
```
Spec prose: §5.7 "Escaped ASCII space (x20) character. This is useful at the start or the end of a line to force a leading or trailing space to become part of the content."
Verdict: Strict-conformant
Evidence: `chars.rs:186` `' ' => Some((' ', 1))`. Test `lexer/quoted.rs:1173` `escape_space` confirms `\<space>` → space.
Reasoning: The escape accepts a literal space after the backslash and produces a space character. This matches the production exactly (note: production uses `x20` directly, no letter-form, and the parser correctly accepts only the literal space).

### [52] ns-esc-double-quote

BNF:
```
[52] ns-esc-double-quote ::= '"'
```
Spec prose: §5.7 "Escaped ASCII double quote (x22)."
Verdict: Strict-conformant
Evidence: `chars.rs:187` `'"' => Some(('"', 1))`. Test `lexer/quoted.rs:1171` `escape_double_quote` confirms `\"` → `"`.
Reasoning: Direct mapping. The escape produces U+0022.

### [53] ns-esc-slash

BNF:
```
[53] ns-esc-slash ::= '/'
```
Spec prose: §5.7 "Escaped ASCII slash (x2F), for JSON compatibility."
Verdict: Strict-conformant
Evidence: `chars.rs:188` `'/' => Some(('/', 1))`. Test `lexer/quoted.rs:1172` `escape_slash` confirms `\/` → `/`.
Reasoning: Direct mapping. The escape produces U+002F. Required for JSON compatibility (JSON's `\/` escape).

### [54] ns-esc-backslash

BNF:
```
[54] ns-esc-backslash ::= '\'
```
Spec prose: §5.7 "Escaped ASCII back slash (x5C)."
Verdict: Strict-conformant
Evidence: `chars.rs:189` `'\\' => Some(('\\', 1))`. Test `lexer/quoted.rs:1170` `escape_backslash` confirms `\\` → `\`.
Reasoning: Direct mapping. The escape produces U+005C.

### [55] ns-esc-next-line

BNF:
```
[55] ns-esc-next-line ::= 'N'
```
Spec prose: §5.7 "Escaped Unicode next line (x85) character."
Verdict: Strict-conformant
Evidence: `chars.rs:190` `'N' => Some(('\u{85}', 1))`. Test `chars.rs:387,388` `nel_escape` confirms `N` → U+0085. `quoted.rs:1194` `\N` → U+0085.
Reasoning: Direct mapping. The escape produces U+0085 (NEL).

### [56] ns-esc-non-breaking-space

BNF:
```
[56] ns-esc-non-breaking-space ::= '_'
```
Spec prose: §5.7 "Escaped Unicode non-breaking space (xA0) character."
Verdict: Strict-conformant
Evidence: `chars.rs:191` `'_' => Some(('\u{A0}', 1))`. Test `chars.rs:388` `nbsp_escape` confirms `_` → U+00A0. `quoted.rs:1195` `\_` → U+00A0.
Reasoning: Direct mapping. The escape produces U+00A0 (NBSP).

### [57] ns-esc-line-separator

BNF:
```
[57] ns-esc-line-separator ::= 'L'
```
Spec prose: §5.7 "Escaped Unicode line separator (x2028) character."
Verdict: Strict-conformant
Evidence: `chars.rs:192` `'L' => Some(('\u{2028}', 1))`. Test `chars.rs:389` `line_sep_escape` confirms `L` → U+2028. `quoted.rs:1196` `\L` → U+2028.
Reasoning: Direct mapping. The escape produces U+2028 (LS).

### [58] ns-esc-paragraph-separator

BNF:
```
[58] ns-esc-paragraph-separator ::= 'P'
```
Spec prose: §5.7 "Escaped Unicode paragraph separator (x2029) character."
Verdict: Strict-conformant
Evidence: `chars.rs:193` `'P' => Some(('\u{2029}', 1))`. Test `chars.rs:390` `para_sep_escape` confirms `P` → U+2029. `quoted.rs:1197` `\P` → U+2029.
Reasoning: Direct mapping. The escape produces U+2029 (PS).

### [59] ns-esc-8-bit

BNF:
```
[59] ns-esc-8-bit ::=
  'x'
  ns-hex-digit{2}
```
Spec prose: §5.7 "Escaped 8-bit Unicode character."
Verdict: Stricter-than-spec
Evidence: `chars.rs:194` `'x' => decode_hex_escape(input, 1, 2)`. `chars.rs:204-216` (`decode_hex_escape`) requires exactly 2 hex digits, parses with `u32::from_str_radix(hex_str, 16)`, and converts to char via `char::from_u32`. Stricter behavior at the caller (`lexer/quoted.rs:580-588`): `\x` escapes that decode to a non-`c-printable` character are rejected.
Reasoning: The hex-decoding step is conformant ([59] requires exactly 2 hex digits → 0-255). However, the caller adds a printability gate: `quoted.rs:580` `if matches!(escape_prefix, 'x' | 'u' | 'U') && !is_c_printable(decoded_ch)` produces an error. This rejects `\x01` (SOH) even though [59] would admit it. Test `quoted.rs:1303-1311` `non_printable_hex_escape_rejected` confirms `\x01` is rejected. Verdict: `Stricter-than-spec` — security hardening (printable-input enforcement at the escape boundary).

### [60] ns-esc-16-bit

BNF:
```
[60] ns-esc-16-bit ::=
  'u'
  ns-hex-digit{4}
```
Spec prose: §5.7 "Escaped 16-bit Unicode character."
Verdict: Stricter-than-spec
Evidence: `chars.rs:195` `'u' => decode_hex_escape(input, 1, 4)`. The same `decode_hex_escape` validates exactly 4 hex digits. `char::from_u32` rejects surrogates (U+D800–U+DFFF) at the codepoint level (`chars.rs:214`). Caller (`lexer/quoted.rs:580-588,592-600`) adds: rejection of non-printable codepoints AND rejection of bidi-control codepoints (U+200E, U+200F, U+202A–U+202E, U+2066–U+2069).
Reasoning: Hex parsing matches [60]. Surrogates are rejected by Rust's `char::from_u32` (matching spec rule that surrogates are not valid Unicode scalar values). Beyond the spec, non-printable and bidi-control codepoints from `\u` escapes are rejected. Test `quoted.rs:1232-1233` confirms surrogate rejection (`\uD800` → error). Test `quoted.rs:1292-1300` confirms bidi rejection (`‮` → error). Verdict: `Stricter-than-spec` — security hardening (Trojan-Source-style attack mitigation; printability enforcement).

### [61] ns-esc-32-bit

BNF:
```
[61] ns-esc-32-bit ::=
  'U'
  ns-hex-digit{8}
```
Spec prose: §5.7 "Escaped 32-bit Unicode character."
Verdict: Stricter-than-spec
Evidence: `chars.rs:196` `'U' => decode_hex_escape(input, 1, 8)`. The same `decode_hex_escape` validates exactly 8 hex digits. `char::from_u32` rejects values above U+10FFFF (`chars.rs:214`) — covered by test `chars.rs:408` `out_of_range_codepoint("U00110000")` returns None. Caller rejects non-printable and bidi-control codepoints at the same boundary as [60].
Reasoning: 8-digit hex parsing matches [61]. Out-of-range codepoints (>U+10FFFF) are rejected. Non-printable and bidi codepoints rejected per the same security rules as [59]/[60]. Test `quoted.rs:1234` confirms `\U00110000` is rejected. Verdict: `Stricter-than-spec` — same security hardening rationale as [59] and [60].

### [62] c-ns-esc-char

BNF:
```
[62] c-ns-esc-char ::=
  c-escape         # '\'
  (
      ns-esc-null
    | ns-esc-bell
    | ns-esc-backspace
    | ns-esc-horizontal-tab
    | ns-esc-line-feed
    | ns-esc-vertical-tab
    | ns-esc-form-feed
    | ns-esc-carriage-return
    | ns-esc-escape
    | ns-esc-space
    | ns-esc-double-quote
    | ns-esc-slash
    | ns-esc-backslash
    | ns-esc-next-line
    | ns-esc-non-breaking-space
    | ns-esc-line-separator
    | ns-esc-paragraph-separator
    | ns-esc-8-bit
    | ns-esc-16-bit
    | ns-esc-32-bit
  )
```
Spec prose: §5.7 "Any escaped character." The full alternation of the 20 sub-productions [42]–[61] preceded by `\`.
Verdict: Stricter-than-spec
Evidence: `chars.rs:173-199` (`decode_escape`) implements the dispatch: it consumes the character after the `\` and pattern-matches against all 20 sub-productions ([42]–[61]). Unknown codes return `None` (`chars.rs:197`), which the caller (`lexer/quoted.rs:563-574`) translates to an "invalid escape sequence" error. Tests `chars.rs:403-411` `decode_escape_rejects` enumerate `q` (unknown), `x4` (truncated), `xGG` (non-hex), `uD800` (surrogate), `U00110000` (out-of-range) — all return `None`. The escape-introducer `\` is correctly required: `lexer/quoted.rs:633,670` only enters escape decoding after seeing `\` in a double-quoted body. Stricter-than-spec gates from [59]/[60]/[61] (printability + bidi rejection) apply on top of the alternation.
Reasoning: The alternation of all 20 sub-productions is implemented exactly. Invalid escapes raise an error rather than silently ignoring the `\` — matching §5.7's "Each escape sequence must be parsed into the appropriate Unicode character." However, the printability and bidi-control gates (inherited from [59]/[60]/[61]) reject some escapes that the strict spec [62] would admit. Test `quoted.rs:1235` `unknown_escape_code("\"\\q\"")` confirms unknown escape rejection. Verdict: `Stricter-than-spec` — the alternation matches; the security overlay narrows acceptance.
