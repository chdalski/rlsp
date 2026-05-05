# BNF Conformance — §6 Structural Productions

Source: `.ai/audit/2026-04-30-phase1-bnf/reconciliation-§6.md` (41 entries)

**Verdict tally (post-fix):** Strict-conformant: 37, Stricter-than-spec: 2, Not-applicable: 0

---

### [63] s-indent(n)

BNF: `s-indent(0) ::= <empty>` / `s-indent(n+1) ::= s-space s-indent(n)`

- **Verdict:** Strict-conformant
- **Spec (§6.1):** "In YAML block styles, structure is determined by indentation. To maintain portability, tab characters must not be used in indentation."
- **Implementation:** `ch == ' '` loop in `lines.rs` counts only leading space characters for `Line::indent`; tabs are excluded from the indent count
- **Tests:** `rlsp-yaml-parser/tests/smoke/block_scalars.rs`; `rlsp-yaml-parser/tests/smoke/mappings.rs`; `lines.rs` unit tests `indent_counts_only_leading_spaces`, `leading_tab_does_not_count_toward_indent`
- **Rationale:** The §6.1 prose normatively requires that tabs MUST NOT be used in indentation. The BNF + prose together require tab rejection; the implementation is Strict-conformant, not Stricter-than-spec.

### [64] s-indent-less-than(n)

BNF: `s-indent-less-than(1) ::= <empty>` / `s-indent-less-than(n+1) ::= s-space s-indent-less-than(n) | <empty>`

- **Verdict:** Strict-conformant
- **Spec (§6.1):** "A block style construct is terminated when encountering a line which is less indented than the construct."
- **Implementation:** `peek_until_dedent()` in `lines.rs` — `line.indent <= base_indent` halts lookahead at the first non-blank line not strictly greater than the base
- **Tests:** `lines.rs` unit test `peek_until_dedent_returns_lines_until_indent_le_base`

### [65] s-indent-less-or-equal(n)

BNF: `s-indent-less-or-equal(0) ::= <empty>` / `s-indent-less-or-equal(n+1) ::= s-space s-indent-less-or-equal(n) | <empty>`

- **Verdict:** Strict-conformant
- **Spec (§6.1):** "The productions use the notation `s-indent-less-than(n)` and `s-indent-less-or-equal(n)` to express this."
- **Implementation:** `is_content_line()` in `lexer/block.rs` uses `indent >= content_indent`; block sequence and mapping parsers apply `<= n` guards for termination
- **Tests:** `rlsp-yaml-parser/tests/smoke/block_scalars.rs`

### [66] s-separate-in-line

BNF: `s-separate-in-line ::= s-white+ | <start-of-line>`

- **Verdict:** Strict-conformant
- **Spec (§6.2):** "Outside indentation and scalar content, YAML uses white space characters for separation between tokens within a line."
- **Implementation:** `trim_start_matches([' ', '\t'])` in `lexer/quoted.rs`; `find([' ', '\t'])` in `event_iter/directives.rs`
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs`; `rlsp-yaml-parser/tests/smoke/comments.rs`

### [67] s-line-prefix(n,c)

BNF: `s-line-prefix(n,BLOCK-OUT) ::= s-block-line-prefix(n)` / `s-line-prefix(n,BLOCK-IN) ::= s-block-line-prefix(n)` / `s-line-prefix(n,FLOW-OUT) ::= s-flow-line-prefix(n)` / `s-line-prefix(n,FLOW-IN) ::= s-flow-line-prefix(n)`

- **Verdict:** Strict-conformant
- **Spec (§6.3):** "Inside scalar content, each line begins with a non-content line prefix. This prefix always includes the indentation."
- **Implementation:** Block context: continuation lines validated against block indent in `lexer/block.rs`. Flow context: `trim_start_matches([' ', '\t'])` in `lexer/quoted.rs`
- **Tests:** `rlsp-yaml-parser/tests/smoke/block_scalars.rs`; `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [68] s-block-line-prefix(n)

BNF: `s-block-line-prefix(n) ::= s-indent(n)`

- **Verdict:** Strict-conformant
- **Spec (§6.3):** "Inside scalar content, each line begins with a non-content line prefix. This prefix always includes the indentation."
- **Implementation:** `try_consume_literal_block_scalar()` in `lexer/block.rs` validates that each non-empty content line has `indent >= content_indent`
- **Tests:** `rlsp-yaml-parser/tests/smoke/block_scalars.rs`

### [69] s-flow-line-prefix(n)

BNF: `s-flow-line-prefix(n) ::= s-indent(n) s-separate-in-line?`

- **Verdict:** Strict-conformant
- **Spec (§6.3):** "For flow scalar styles it additionally includes all leading white space, which may contain tab characters."
- **Implementation:** `enforce_flow_line_prefix()` in `lexer/quoted.rs` — checks that the first `n` bytes of a continuation line are spaces (enforcing `s-indent(n)`), then strips any additional `s-separate-in-line` whitespace. Tabs in the `s-indent(n)` portion cause a parse error.
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `rlsp-yaml-parser/tests/conformance/flow_line_prefix.rs`
- **Note:** Previously Lenient — `trim_start_matches([' ', '\t'])` accepted tabs in the indent portion. Fixed in commit `1ed7c94` (`fix(rlsp-yaml-parser): enforce s-indent(n) on quoted scalar continuation lines`).

### [70] l-empty(n,c)

BNF: `l-empty(n,c) ::= ( s-line-prefix(n,c) | s-indent-less-than(n) ) b-as-line-feed`

- **Verdict:** Strict-conformant
- **Spec (§6.4):** "An empty line line consists of the non-content prefix followed by a line break."
- **Implementation:** `skip_empty_lines()` in `lexer.rs`; blank continuation lines in `lexer/quoted.rs` push literal `'\n'` into the value
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `rlsp-yaml-parser/tests/smoke/block_scalars.rs`

### [71] b-l-trimmed(n,c)

BNF: `b-l-trimmed(n,c) ::= b-non-content l-empty(n,c)+`

- **Verdict:** Strict-conformant
- **Spec (§6.5):** "If a line break is followed by an empty line, it is trimmed; the first line break is discarded and the rest are retained as content."
- **Implementation:** `collect_double_quoted_continuations()` in `lexer/quoted.rs` — blank lines accumulated in `pending_blanks`; when a non-blank line follows, N blank lines produce N literal newlines (originating break discarded, empty-line breaks retained)
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [72] b-as-space

BNF: `b-as-space ::= b-break`

- **Verdict:** Strict-conformant
- **Spec (§6.5):** "Otherwise (the following line is not empty), the line break is converted to a single space (x20)."
- **Implementation:** `owned.push(' ')` in `lexer/quoted.rs` when `pending_blanks == 0` and `line_continuation` is false; same for single-quoted continuation folds
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [73] b-l-folded(n,c)

BNF: `b-l-folded(n,c) ::= b-l-trimmed(n,c) | b-as-space`

- **Verdict:** Strict-conformant
- **Spec (§6.5):** "A folded non-empty line may end with either of the above line breaks."
- **Implementation:** `collect_double_quoted_continuations()` in `lexer/quoted.rs` — `pending_blanks` counter selects between `b-l-trimmed` (N>0) and `b-as-space` (N==0) on each fold boundary; same logic for single-quoted scalars
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [74] s-flow-folded(n)

BNF: `s-flow-folded(n) ::= s-separate-in-line? b-l-folded(n,FLOW-IN) s-flow-line-prefix(n)`

- **Verdict:** Strict-conformant
- **Spec (§6.5):** "Folding in flow styles provides more relaxed semantics. Once all such spaces have been discarded, all line breaks are folded without exception."
- **Implementation:** `try_consume_single_quoted()` / `try_consume_double_quoted()` in `lexer/quoted.rs` — trailing whitespace trimmed from each partial line before fold; leading whitespace trimmed from each continuation line; fold space or newline inserted between
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`

### [75] c-nb-comment-text

BNF: `c-nb-comment-text ::= c-comment nb-char*`

- **Verdict:** Strict-conformant
- **Spec (§6.6):** "An explicit comment is marked by a `#` indicator. Comments must be separated from other tokens by white space characters."
- **Implementation:** `starts_with('#')` check in `lexer/comment.rs`; text slice is everything after `#`; `reject_non_printable()` enforcement in `lexer.rs` ensures comment content is within c-printable (which is a superset of nb-char)
- **Tests:** `rlsp-yaml-parser/tests/smoke/comments.rs`; `comment.rs` unit tests `happy_path_text`
- **Note:** Previously Lenient — a literal BOM in a comment body was retained verbatim. Fixed together with [1] in commit `666e2f2`.

### [76] b-comment

BNF: `b-comment ::= b-non-content | <end-of-input>`

- **Verdict:** Strict-conformant
- **Spec (§6.6):** "To ensure JSON compatibility, YAML processors must allow for the omission of the final comment line break of the input stream."
- **Implementation:** `lexer/comment.rs` — consumed line content returned up to but not including the terminator; end-of-input handled by `LineBuffer` returning `BreakType::Eof`
- **Tests:** `rlsp-yaml-parser/tests/smoke/comments.rs`

### [77] s-b-comment

BNF: `s-b-comment ::= ( s-separate-in-line c-nb-comment-text? )? b-comment`

- **Verdict:** Strict-conformant
- **Spec (§6.6):** "Comments must be separated from other tokens by white space characters."
- **Implementation:** `handle_plain_scalar_inline()` in `lexer.rs` — trailing comment after inline plain scalar on marker lines requires `#` preceded by implicit whitespace; `parse_yaml_directive()` in `event_iter/directives.rs` — trailing content after YAML version checked for empty or `#` prefix
- **Tests:** `rlsp-yaml-parser/tests/smoke/comments.rs`; `rlsp-yaml-parser/tests/smoke/directives.rs`
- **Rationale:** Audit B initially flagged a lenient case at `lexer.rs:354-381` (`handle_plain_scalar_inline`), but investigation showed the plain-scanner's behavior — treating `#` without preceding whitespace as part of the scalar — is correct per spec. The `hash_without_preceding_space_is_content` test confirms this.

### [78] l-comment

BNF: `l-comment ::= s-separate-in-line c-nb-comment-text? b-comment`

- **Verdict:** Strict-conformant
- **Spec (§6.6):** "Outside scalar content, comments may appear on a line of their own, independent of the indentation level. Note that outside scalar content, a line containing only white space characters is taken to be a comment line."
- **Implementation:** `lexer/comment.rs` — `trim_start_matches([' ', '\t'])` followed by `starts_with('#')`; `is_blank_not_comment()` in `lexer.rs` distinguishes blank-but-not-comment lines from comment lines
- **Tests:** `rlsp-yaml-parser/tests/smoke/comments.rs`

### [79] s-l-comments

BNF: `s-l-comments ::= ( s-b-comment | <start-of-line> ) l-comment*`

- **Verdict:** Strict-conformant
- **Spec (§6.6):** "In most cases, when a line may end with a comment, YAML allows it to be followed by additional comment lines. The only exception is a comment ending a block scalar header."
- **Implementation:** `consume_preamble_between_docs()` in `event_iter/directives.rs` — loops consuming blank and comment lines; block scalar header explicitly stops at the comment on its header line (enforced in `lexer/block.rs`)
- **Tests:** `rlsp-yaml-parser/tests/smoke/comments.rs`

### [80] s-separate(n,c)

BNF: `s-separate(n,BLOCK-OUT) ::= s-separate-lines(n)` / `s-separate(n,BLOCK-IN) ::= s-separate-lines(n)` / `s-separate(n,FLOW-OUT) ::= s-separate-lines(n)` / `s-separate(n,FLOW-IN) ::= s-separate-lines(n)` / `s-separate(n,BLOCK-KEY) ::= s-separate-in-line` / `s-separate(n,FLOW-KEY) ::= s-separate-in-line`

- **Verdict:** Strict-conformant
- **Spec (§6.7):** "Implicit keys are restricted to a single line. In all other cases, YAML allows tokens to be separated by multi-line (possibly empty) comments."
- **Implementation:** Block context: `skip_and_collect_comments_in_doc()` in `event_iter/directives.rs` — multi-line comment separation. Flow/key context: single-line whitespace separation in `event_iter/flow.rs`
- **Tests:** `rlsp-yaml-parser/tests/smoke/comments.rs`; `rlsp-yaml-parser/tests/smoke/flow_collections.rs`

### [81] s-separate-lines(n)

BNF: `s-separate-lines(n) ::= ( s-l-comments s-flow-line-prefix(n) ) | s-separate-in-line`

- **Verdict:** Strict-conformant
- **Spec (§6.7):** "Note that structures following multi-line comment separation must be properly indented, even though there is no such restriction on the separation comment lines themselves."
- **Implementation:** `consume_preamble_between_docs()` in `event_iter/directives.rs` — comment-then-indent path; inline whitespace path for single-line separation in `lexer.rs`
- **Tests:** `rlsp-yaml-parser/tests/smoke/comments.rs`

### [82] l-directive

BNF: `l-directive ::= c-directive ( ns-yaml-directive | ns-tag-directive | ns-reserved-directive ) s-l-comments`

- **Verdict:** Strict-conformant
- **Spec (§6.8):** "Directives are instructions to the YAML processor. There is no way to define private directives. Directives are a presentation detail and must not be used to convey content information."
- **Implementation:** `parse_directive()` in `event_iter/directives.rs` — dispatches on directive name; `is_directive_line()` in `lexer.rs`
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs`

### [83] ns-reserved-directive

BNF: `ns-reserved-directive ::= ns-directive-name ( s-separate-in-line ns-directive-parameter )*`

- **Verdict:** Strict-conformant
- **Spec (§6.8):** "A YAML processor should ignore unknown directives with an appropriate warning."
- **Implementation:** Unknown directive names silently increment `directive_count` and return `Ok(())` in `event_iter/directives.rs` — no warning emitted (spec says "should", not "must")
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs`
- **Rationale:** Audit A's concern about body-shape validation of unknown directives is captured at [1] c-printable (non-printable-in-body); [83]'s "ignore" behavior is spec-permitted for unknown directives.

### [84] ns-directive-name

BNF: `ns-directive-name ::= ns-char+`

- **Verdict:** Strict-conformant
- **Spec (§6.8):** "Each directive is specified on a separate non-indented line starting with the `%` indicator, followed by the directive name."
- **Implementation:** `parse_directive()` in `event_iter/directives.rs` — `find([' ', '\t'])` extracts the name, then `validate_directive_name()` checks each character against `is_ns_char`
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs`; `rlsp-yaml-parser/tests/conformance/directives.rs`
- **Note:** Previously Lenient — directive name was extracted by whitespace split without per-character validation. Fixed in commit `51cdfdd` (`fix(rlsp-yaml-parser): validate directive names and parameters against ns-char`).

### [85] ns-directive-parameter

BNF: `ns-directive-parameter ::= ns-char+`

- **Verdict:** Strict-conformant
- **Spec (§6.8):** "Each directive is specified on a separate non-indented line starting with the `%` indicator, followed by the directive name and a list of parameters."
- **Implementation:** `validate_directive_param()` in `event_iter/directives.rs` — validates each character of directive parameters against `is_ns_char`
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs`; `rlsp-yaml-parser/tests/conformance/directives.rs`
- **Note:** Previously Lenient — same root cause as [84]. Fixed in commit `51cdfdd`.

### [86] ns-yaml-directive

BNF: `ns-yaml-directive ::= "YAML" s-separate-in-line ns-yaml-version`

- **Verdict:** Stricter-than-spec
- **Spec (§6.8.1):** "A version 1.2 YAML processor must accept documents with an explicit `%YAML 1.2` directive, as well as documents lacking a `YAML` directive."
- **Implementation:** `parse_yaml_directive()` in `event_iter/directives.rs` — `if major != 1` rejects both `major == 0` and `major >= 2`
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs`
- **Rationale:** Defensive rejection of `major == 0` (no defined YAML 0.x exists). The spec only mandates rejection of higher major versions; rejecting `major == 0` is conservatism beyond spec.

### [87] ns-yaml-version

BNF: `ns-yaml-version ::= ns-dec-digit+ '.' ns-dec-digit+`

- **Verdict:** Stricter-than-spec
- **Spec (§6.8.1):** "A version 1.2 YAML processor must also accept documents with an explicit `%YAML 1.1` directive."
- **Implementation:** `parse_yaml_directive()` in `event_iter/directives.rs` — `parse::<u8>()` limits major and minor to [0, 255]; `%YAML 1.300` would fail the minor-version parse
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs`
- **Rationale:** The BNF `ns-dec-digit+` admits any number of decimal digits. The `u8` parse is a pragmatic limit — no realistic YAML version exceeds 255 — but is technically stricter than the BNF.

### [88] ns-tag-directive

BNF: `ns-tag-directive ::= "TAG" s-separate-in-line c-tag-handle s-separate-in-line ns-tag-prefix`

- **Verdict:** Strict-conformant
- **Spec (§6.8.2):** "The `TAG` directive establishes a tag shorthand notation for specifying node tags."
- **Implementation:** `parse_tag_directive()` in `event_iter/directives.rs` — splits on whitespace for handle and prefix, validates handle via `is_valid_tag_handle()`, stores in `directive_scope.tag_handles`
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs`; `rlsp-yaml-parser/tests/smoke/tags.rs`

### [89] c-tag-handle

BNF: `c-tag-handle ::= c-named-tag-handle | c-secondary-tag-handle | c-primary-tag-handle`

- **Verdict:** Strict-conformant
- **Spec (§6.8.2.1):** "The tag handle exactly matches the prefix of the affected tag shorthand. There are three tag handle variants."
- **Implementation:** `is_valid_tag_handle()` in `event_iter/properties.rs` — recognises `!`, `!!`, and `!word-chars!` forms
- **Tests:** `rlsp-yaml-parser/tests/smoke/tags.rs`; `properties.rs` unit tests `is_valid_tag_handle_*`

### [90] c-primary-tag-handle

BNF: `c-primary-tag-handle ::= '!'`

- **Verdict:** Strict-conformant
- **Spec (§6.8.2.1):** "The primary tag handle is a single `!` character."
- **Implementation:** `"!" => true` branch in `is_valid_tag_handle()` in `event_iter/properties.rs`
- **Tests:** `properties.rs` unit test `is_valid_tag_handle_primary`

### [91] c-secondary-tag-handle

BNF: `c-secondary-tag-handle ::= "!!"`

- **Verdict:** Strict-conformant
- **Spec (§6.8.2.1):** "The secondary tag handle is written as `!!`."
- **Implementation:** `"!!" => true` branch in `is_valid_tag_handle()` in `event_iter/properties.rs`; `resolve_tag()` in `event_iter/directive_scope.rs` expands `!!suffix` using default prefix `"tag:yaml.org,2002:"`
- **Tests:** `properties.rs` unit test `is_valid_tag_handle_secondary`

### [92] c-named-tag-handle

BNF: `c-named-tag-handle ::= c-tag ns-word-char+ c-tag`

- **Verdict:** Strict-conformant
- **Spec (§6.8.2.1):** "A named tag handle surrounds a non-empty name with `!` characters. A handle name must not be used in a tag shorthand unless an explicit `TAG` directive has associated some prefix with it."
- **Implementation:** `is_valid_tag_handle()` in `event_iter/properties.rs` — inner word validated with `.is_ascii_alphanumeric() || c == '-'`; underscore in handle names is rejected
- **Tests:** `properties.rs` unit tests `is_valid_tag_handle_named_with_hyphen`, `is_valid_tag_handle_rejects_named_with_underscore`; `directives.rs` Group N integration tests

### [93] ns-tag-prefix

BNF: `ns-tag-prefix ::= c-ns-local-tag-prefix | ns-global-tag-prefix`

- **Verdict:** Strict-conformant
- **Spec (§6.8.2.2):** "There are two tag prefix variants."
- **Implementation:** `validate_tag_prefix()` in `event_iter/directives.rs` — dispatches on leading byte to select local vs global form, validates all bytes against `is_ns_uri_char_single()` or `%HH` percent-encoded sequences
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs` Group P (P-1 through P-10)
- **Note:** Previously Lenient — prefix validation rejected only ASCII control chars and DEL, not the full `ns-uri-char` constraints. Fixed in commit `4a6d2ee` (`fix(rlsp-yaml-parser): validate tag prefix against ns-uri-char and reject empty shorthand suffix`).

### [94] c-ns-local-tag-prefix

BNF: `c-ns-local-tag-prefix ::= c-tag ns-uri-char*`

- **Verdict:** Strict-conformant
- **Spec (§6.8.2.2):** "If the prefix begins with a `!` character, shorthands using the handle are expanded to a local tag."
- **Implementation:** `validate_tag_prefix()` in `event_iter/directives.rs` — accepts `!` as the leading c-tag byte, then validates all subsequent bytes as `ns-uri-char` or `%HH`
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs` P-3 (`tag_prefix_local_starting_with_bang_is_accepted`)
- **Note:** Previously Lenient — same root cause as [93]. Fixed in commit `4a6d2ee`.

### [95] ns-global-tag-prefix

BNF: `ns-global-tag-prefix ::= ns-tag-char ns-uri-char*`

- **Verdict:** Strict-conformant
- **Spec (§6.8.2.2):** "If the prefix begins with a character other than `!`, it must be a valid URI prefix, and should contain at least the scheme."
- **Implementation:** `validate_tag_prefix()` in `event_iter/directives.rs` — requires `is_ns_tag_char_single()` on the first byte for global prefixes, then validates remaining bytes as `ns-uri-char` or `%HH`
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs` P-1, P-2, P-4, P-5, P-9, P-10
- **Note:** Previously Lenient — same root cause as [93]. Fixed in commit `4a6d2ee`.

### [96] c-ns-properties(n,c)

BNF: `c-ns-properties(n,c) ::= ( c-ns-tag-property ( s-separate(n,c) c-ns-anchor-property )? ) | ( c-ns-anchor-property ( s-separate(n,c) c-ns-tag-property )? )`

- **Verdict:** Strict-conformant
- **Spec (§6.9):** "Each node may have two optional properties, anchor and tag, in addition to its content. Either or both may be omitted."
- **Implementation:** `pending_tag` and `pending_anchor` fields in `event_iter/` accumulate both properties in either order; both emitted before the node event
- **Tests:** `rlsp-yaml-parser/tests/smoke/tags.rs`; `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`

### [97] c-ns-tag-property

BNF: `c-ns-tag-property ::= c-verbatim-tag | c-ns-shorthand-tag | c-non-specific-tag`

- **Verdict:** Strict-conformant
- **Spec (§6.9.1):** "The tag property identifies the type of the native data structure presented by the node."
- **Implementation:** `scan_tag()` in `event_iter/properties.rs` — dispatches on character after `!`: `<` → verbatim, `!` → secondary/primary shorthand, tag-chars → named/secondary shorthand, empty/non-tag → non-specific
- **Tests:** `rlsp-yaml-parser/tests/smoke/tags.rs`

### [98] c-verbatim-tag

BNF: `c-verbatim-tag ::= "!<" ns-uri-char+ '>'`

- **Verdict:** Strict-conformant
- **Spec (§6.9.1):** "A tag may be written verbatim by surrounding it with the `<` and `>` characters. A verbatim tag must either begin with a `!` (a local tag) or be a valid URI (a global tag)."
- **Implementation:** `scan_tag()` in `event_iter/properties.rs` — `strip_prefix('<')` branch validates URI body byte-by-byte against `is_ns_uri_char_single()` and `%HH` sequences; empty URI rejected; verbatim tag admissibility (local or valid global URI) enforced
- **Tests:** `rlsp-yaml-parser/tests/smoke/tags.rs`; `properties.rs` unit tests `scan_tag_verbatim_*`

### [99] c-ns-shorthand-tag

BNF: `c-ns-shorthand-tag ::= c-tag-handle ns-tag-char+`

- **Verdict:** Strict-conformant
- **Spec (§6.9.1):** "A tag shorthand consists of a valid tag handle followed by a non-empty suffix."
- **Implementation:** `scan_tag()` in `event_iter/properties.rs` — primary `!!suffix` branch rejects `suffix_bytes == 0`; named `!handle!suffix` branch rejects `named_handle_suffix_bytes == 0`; both return error `"shorthand tag requires a non-empty suffix"`
- **Tests:** `rlsp-yaml-parser/tests/smoke/directives.rs` P-6, P-7, P-8; `rlsp-yaml-parser/tests/smoke/tags.rs` `primary_handle_empty_suffix_returns_error`; `properties.rs` unit tests `scan_tag_secondary_handle_no_suffix`, `scan_tag_named_handle_with_empty_suffix`
- **Note:** Previously Lenient — `!!` alone and `!handle!` (without suffix) were accepted. Fixed in commit `4a6d2ee`.

### [100] c-non-specific-tag

BNF: `c-non-specific-tag ::= '!'`

- **Verdict:** Strict-conformant
- **Spec (§6.9.1):** "If a node has no tag property, it is assigned a non-specific tag that needs to be resolved to a specific one."
- **Implementation:** `scan_tag()` in `event_iter/properties.rs` — when `scan_tag_suffix` returns 0 and content does not start with `<` or `!`, the tag is the bare `!` one-byte slice
- **Tests:** `rlsp-yaml-parser/tests/smoke/tags.rs`; `properties.rs` unit tests `scan_tag_non_specific_*`

### [101] c-ns-anchor-property

BNF: `c-ns-anchor-property ::= c-anchor ns-anchor-name`

- **Verdict:** Strict-conformant
- **Spec (§6.9.2):** "An anchor is denoted by the `&` indicator. It marks a node for future reference."
- **Implementation:** `scan_anchor_name()` in `event_iter/properties.rs` — called after `&` indicator; scans `ns-anchor-char` characters until whitespace, flow indicator, or end
- **Tests:** `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`

### [102] ns-anchor-char

BNF: `ns-anchor-char ::= ns-char - c-flow-indicator`

- **Verdict:** Strict-conformant
- **Spec (§6.9.2):** "Anchor names must not contain the `[`, `]`, `{`, `}` and `,` characters."
- **Implementation:** `is_ns_anchor_char()` in `chars.rs` — `ns-char` range excluding `c-flow-indicator` characters
- **Tests:** `chars.rs` unit tests `ns_anchor_char_accepts`, `ns_anchor_char_rejects_flow_indicators`, `ns_anchor_char_rejects`

### [103] ns-anchor-name

BNF: `ns-anchor-name ::= ns-anchor-char+`

- **Verdict:** Strict-conformant
- **Spec (§6.9.2):** "An anchored node need not be referenced by any alias nodes; in particular, it is valid for all nodes to be anchored."
- **Implementation:** `scan_anchor_name()` in `event_iter/properties.rs` — `.take_while(|&(_, ch)| is_ns_anchor_char(ch))` scans one or more `ns-anchor-char`; empty result → error
- **Tests:** `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`; `properties.rs` unit tests `scan_anchor_name_*`
