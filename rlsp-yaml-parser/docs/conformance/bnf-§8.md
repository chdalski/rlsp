# BNF Conformance — §8 Block Style Productions

Source: `.ai/audit/2026-04-30-phase1-bnf/reconciliation-§8.md` (40 entries)

**Verdict tally:** Strict-conformant: 40, Stricter-than-spec: 0, Not-applicable: 0

§8 is the cleanest chapter audited. Both auditors independently noted the comprehensive header rejection rules, the §8.1.1.1 over-indented-leading-blank-line enforcement on both literal and folded paths, the Unicode-correct 1024-char implicit-key limit, and the BLOCK-IN/BLOCK-OUT split for `seq-space`. Zero disagreements after reconciliation.

---

### [162] c-b-block-header(t)

BNF: `c-b-block-header(t) ::= ( ( c-indentation-indicator c-chomping-indicator(t) ) | ( c-chomping-indicator(t) c-indentation-indicator ) ) s-b-comment`

- **Verdict:** Strict-conformant
- **Spec (§8.1.1):** "Block scalars are controlled by a few indicators given in a header preceding the content itself. This header is followed by a non-content line break with an optional comment. This is the only case where a comment must not be followed by additional comment lines."
- **Implementation:** `parse_block_header()` in `lexer/block.rs` — parses either order of indicators, validates that only optional whitespace + comment follow, and enforces no trailing non-comment content
- **Tests:** `tests/yaml-test-suite/src/P2AD.yaml` (Spec Example 8.1. Block Scalar Header); `block.rs` unit tests H-A (header parsing happy path)

### [163] c-indentation-indicator

BNF: `c-indentation-indicator ::= [x31-x39]    # 1-9`

- **Verdict:** Strict-conformant
- **Spec (§8.1.1.1):** "If a block scalar has an indentation indicator, then the content indentation level of the block scalar is equal to the indentation level of the block scalar plus the integer value of the indentation indicator character. It is an error if any non-empty line does not begin with a number of spaces greater than or equal to the content indentation level."
- **Implementation:** `parse_block_header()` in `lexer/block.rs` — digits `'1'..='9'` map to explicit indent; `'0'` is rejected as invalid
- **Tests:** `tests/yaml-test-suite/src/R4YG.yaml` (Spec Example 8.2. Block Indentation Indicator); `block.rs` unit tests H-E (explicit indent indicator)

### [164] c-chomping-indicator(t)

BNF: `c-chomping-indicator(STRIP) ::= '-'` / `c-chomping-indicator(KEEP) ::= '+'` / `c-chomping-indicator(CLIP) ::= ""`

- **Verdict:** Strict-conformant
- **Spec (§8.1.1.2):** "Stripping is specified by the `-` chomping indicator. Clipping is the default behavior used if no explicit chomping indicator is specified. Keeping is specified by the `+` chomping indicator."
- **Implementation:** `parse_block_header()` in `lexer/block.rs` — `'+'` → `Chomp::Keep`; `'-'` → `Chomp::Strip`; absent → `Chomp::Clip` default
- **Tests:** `tests/yaml-test-suite/src/A6F9.yaml` (Spec Example 8.4. Chomping Final Line Break); `block.rs` unit tests H-A

### [165] b-chomped-last(t)

BNF: `b-chomped-last(STRIP) ::= b-non-content | <end-of-input>` / `b-chomped-last(CLIP) ::= b-as-line-feed | <end-of-input>` / `b-chomped-last(KEEP) ::= b-as-line-feed | <end-of-input>`

- **Verdict:** Strict-conformant
- **Spec (§8.1.1.2):** "The interpretation of the final line break of a block scalar is controlled by the chomping indicator specified in the block scalar header."
- **Implementation:** `apply_chomping()` in `lexer/block.rs` — Strip removes the trailing `\n`; Clip preserves exactly one `\n`; Keep preserves the `\n` from the last content line before appending blank lines
- **Tests:** `tests/yaml-test-suite/src/A6F9.yaml`; `block.rs` unit tests H-D

### [166] l-chomped-empty(n,t)

BNF: `l-chomped-empty(n,STRIP) ::= l-strip-empty(n)` / `l-chomped-empty(n,CLIP) ::= l-strip-empty(n)` / `l-chomped-empty(n,KEEP) ::= l-keep-empty(n)`

- **Verdict:** Strict-conformant
- **Spec (§8.1.1.2):** "The interpretation of the trailing empty lines following a block scalar is also controlled by the chomping indicator specified in the block scalar header."
- **Implementation:** `apply_chomping()` in `lexer/block.rs` — Strip and Clip discard trailing blank lines (`trailing_blank_count` ignored); Keep appends `trailing_blank_count` newlines via `repeat_n`
- **Tests:** `tests/yaml-test-suite/src/F8F9.yaml` (Spec Example 8.5. Chomping Trailing Lines); `tests/yaml-test-suite/src/K858.yaml`

### [167] l-strip-empty(n)

BNF: `l-strip-empty(n) ::= ( s-indent-less-or-equal(n) b-non-content )* l-trail-comments(n)?`

- **Verdict:** Strict-conformant
- **Spec (§8.1.1.2):** "The interpretation of the trailing empty lines following a block scalar is also controlled by the chomping indicator specified in the block scalar header."
- **Implementation:** The content collection loop in `lexer/block.rs` — whitespace-only blank lines below `content_indent` are consumed and counted in `trailing_newlines`; discarded by `apply_chomping()` for Strip/Clip
- **Tests:** `tests/yaml-test-suite/src/F8F9.yaml`

### [168] l-keep-empty(n)

BNF: `l-keep-empty(n) ::= l-empty(n,BLOCK-IN)* l-trail-comments(n)?`

- **Verdict:** Strict-conformant
- **Spec (§8.1.1.2):** "Keeping is specified by the `+` chomping indicator. In this case, the final line break and any trailing empty lines are considered to be part of the scalar's content."
- **Implementation:** Blank lines counted into `trailing_newlines` in `lexer/block.rs`; `apply_chomping()` appends all trailing newlines in the `Chomp::Keep` branch
- **Tests:** `tests/yaml-test-suite/src/F8F9.yaml`; `tests/yaml-test-suite/src/K858.yaml`

### [169] l-trail-comments(n)

BNF: `l-trail-comments(n) ::= s-indent-less-than(n) c-nb-comment-text b-comment l-comment*`

- **Verdict:** Strict-conformant
- **Spec (§8.1.1.2):** "Explicit comment lines may follow the trailing empty lines. To prevent ambiguity, the first such comment line must be less indented than the block scalar content."
- **Implementation:** The content collection loop in `lexer/block.rs` — a `#`-prefixed line at `indent < content_indent` dedents the block scalar; control returns to the document-level dispatcher; the comment scanner processes it. A `#`-prefixed line at `indent >= content_indent` is part of the block scalar's content.
- **Tests:** `tests/yaml-test-suite/src/F8F9.yaml` (comment lines following block scalar)
- **Rationale:** Audit A misread the BNF — the production defines WHAT counts as a trailing-comment block, not WHAT MUST BE REJECTED. A line not satisfying `s-indent-less-than(n)` is simply not a trail-comment; it becomes scalar content. The implementation is Strict-conformant.

### [170] c-l+literal(n)

BNF: `c-l+literal(n) ::= c-literal c-b-block-header(t) l-literal-content(n+m,t)`

- **Verdict:** Strict-conformant
- **Spec (§8.1.2):** "The literal style is denoted by the `|` indicator. It is the simplest, most restricted and most readable scalar style."
- **Implementation:** `try_consume_literal_block_scalar()` in `lexer/block.rs` — dispatches on `|`, parses header via `parse_block_header()`, collects literal content lines
- **Tests:** `tests/yaml-test-suite/src/M9B4.yaml` (Spec Example 8.7. Literal Scalar); `tests/yaml-test-suite/src/DWX9.yaml`; `rlsp-yaml-parser/tests/smoke/block_scalars.rs`

### [171] l-nb-literal-text(n)

BNF: `l-nb-literal-text(n) ::= l-empty(n,BLOCK-IN)* s-indent(n) nb-char+`

- **Verdict:** Strict-conformant
- **Spec (§8.1.2):** "Inside literal scalars, all (indented) characters are considered to be content, including white space characters. Note that all line break characters are normalized."
- **Implementation:** Content line detection in `lexer/block.rs` — `indent >= content_indent` and non-empty after stripping indent prefix; leading blank lines (`l-empty`) accumulated before first real content
- **Tests:** `tests/yaml-test-suite/src/DWX9.yaml`; `block.rs` unit tests H-C (clip content collection)

### [172] b-nb-literal-next(n)

BNF: `b-nb-literal-next(n) ::= b-as-line-feed l-nb-literal-text(n)`

- **Verdict:** Strict-conformant
- **Spec (§8.1.2):** "Inside literal scalars, all (indented) characters are considered to be content, including white space characters."
- **Implementation:** Each content line adds `\n` via `out.push('\n')` in `lexer/block.rs` when `break_type != BreakType::Eof`; the next line is then collected as literal text
- **Tests:** `tests/yaml-test-suite/src/DWX9.yaml`

### [173] l-literal-content(n,t)

BNF: `l-literal-content(n,t) ::= ( l-nb-literal-text(n) b-nb-literal-next(n)* b-chomped-last(t) )? l-chomped-empty(n,t)`

- **Verdict:** Strict-conformant
- **Spec (§8.1.2):** "In addition, empty lines are not folded, though final line breaks and trailing empty lines are chomped."
- **Implementation:** Full content collection loop in `lexer/block.rs`; `apply_chomping()` call applying the chomping rules to the assembled content
- **Tests:** `tests/yaml-test-suite/src/DWX9.yaml`; `tests/yaml-test-suite/src/A6F9.yaml`

### [174] c-l+folded(n)

BNF: `c-l+folded(n) ::= c-folded c-b-block-header(t) l-folded-content(n+m,t)`

- **Verdict:** Strict-conformant
- **Spec (§8.1.3):** "The folded style is denoted by the `>` indicator. It is similar to the literal style; however, folded scalars are subject to line folding."
- **Implementation:** `try_consume_folded_block_scalar()` in `lexer/block.rs` — dispatches on `>`, parses header via `parse_block_header()`, collects folded content via `collect_folded_lines()`
- **Tests:** `tests/yaml-test-suite/src/G992.yaml` (Spec Example 8.9. Folded Scalar); `tests/yaml-test-suite/src/7T8X.yaml`; `rlsp-yaml-parser/tests/smoke/folded_scalars.rs`

### [175] s-nb-folded-text(n)

BNF: `s-nb-folded-text(n) ::= s-indent(n) ns-char nb-char*`

- **Verdict:** Strict-conformant
- **Spec (§8.1.3):** "Folding allows long lines to be broken anywhere a single space character separates two non-space characters."
- **Implementation:** `is_content_line()` in `lexer/block.rs` — content line requires `indent >= content_indent` and non-whitespace content
- **Tests:** `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.10. Folded Lines)

### [176] l-nb-folded-lines(n)

BNF: `l-nb-folded-lines(n) ::= s-nb-folded-text(n) ( b-l-folded(n,BLOCK-IN) s-nb-folded-text(n) )*`

- **Verdict:** Strict-conformant
- **Spec (§8.1.3):** "Folding allows long lines to be broken anywhere a single space character separates two non-space characters."
- **Implementation:** `collect_folded_lines()` in `lexer/block.rs` — equally-indented non-spaced consecutive content lines joined with a single space `out.push(' ')`
- **Tests:** `tests/yaml-test-suite/src/7T8X.yaml`

### [177] s-nb-spaced-text(n)

BNF: `s-nb-spaced-text(n) ::= s-indent(n) s-white nb-char*`

- **Verdict:** Strict-conformant
- **Spec (§8.1.3):** "Lines starting with white space characters (more-indented lines) are not folded."
- **Implementation:** `is_more_indented()` in `lexer/block.rs` — `next.indent > content_indent || after_indent.starts_with([' ', '\t'])` classifies lines whose content after the indent prefix starts with whitespace as more-indented and not folded
- **Tests:** `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.11. More Indented Lines)

### [178] b-l-spaced(n)

BNF: `b-l-spaced(n) ::= b-as-line-feed l-empty(n,BLOCK-IN)*`

- **Verdict:** Strict-conformant
- **Spec (§8.1.3):** "Lines starting with white space characters (more-indented lines) are not folded."
- **Implementation:** `collect_folded_lines()` in `lexer/block.rs` — when preceding or current line is `is_more_indented()`, the break is preserved as `\n` rather than folded to a space
- **Tests:** `tests/yaml-test-suite/src/7T8X.yaml`

### [179] l-nb-spaced-lines(n)

BNF: `l-nb-spaced-lines(n) ::= s-nb-spaced-text(n) ( b-l-spaced(n) s-nb-spaced-text(n) )*`

- **Verdict:** Strict-conformant
- **Spec (§8.1.3):** "Lines starting with white space characters (more-indented lines) are not folded."
- **Implementation:** `collect_folded_lines()` in `lexer/block.rs` — spaced lines: consecutive `is_more_indented()` content lines joined with `\n`, not space
- **Tests:** `tests/yaml-test-suite/src/7T8X.yaml`

### [180] l-nb-same-lines(n)

BNF: `l-nb-same-lines(n) ::= l-empty(n,BLOCK-IN)* ( l-nb-folded-lines(n) | l-nb-spaced-lines(n) )`

- **Verdict:** Strict-conformant
- **Spec (§8.1.3):** "Line breaks and empty lines separating folded and more-indented lines are also not folded."
- **Implementation:** `collect_folded_lines()` in `lexer/block.rs` — empty lines between content blocks accumulated in `trailing_newlines`; classification of surrounding lines determines folding vs preservation when a content line is reached
- **Tests:** `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.12. Empty Separation Lines)

### [181] l-nb-diff-lines(n)

BNF: `l-nb-diff-lines(n) ::= l-nb-same-lines(n) ( b-as-line-feed l-nb-same-lines(n) )*`

- **Verdict:** Strict-conformant
- **Spec (§8.1.3):** "Line breaks and empty lines separating folded and more-indented lines are also not folded."
- **Implementation:** Full `collect_folded_lines()` loop in `lexer/block.rs` — handles sequences of same-group blocks separated by blank lines with `trailing_newlines + extra` logic for transitions between folded and spaced groups
- **Tests:** `tests/yaml-test-suite/src/7T8X.yaml`

### [182] l-folded-content(n,t)

BNF: `l-folded-content(n,t) ::= ( l-nb-diff-lines(n) b-chomped-last(t) )? l-chomped-empty(n,t)`

- **Verdict:** Strict-conformant
- **Spec (§8.1.3):** "The final line break and trailing empty lines if any, are subject to chomping and are never folded."
- **Implementation:** `collect_folded_lines()` in `lexer/block.rs` returns `(content, trailing_newlines)`; `apply_chomping()` applies the chomp indicator to the assembled content
- **Tests:** `tests/yaml-test-suite/src/7T8X.yaml` (Spec Example 8.13. Final Empty Lines)

### [183] l+block-sequence(n)

BNF: `l+block-sequence(n) ::= ( s-indent(n+1+m) c-l-block-seq-entry(n+1+m) )+`

- **Verdict:** Strict-conformant
- **Spec (§8.2.1):** "A block sequence is simply a series of nodes, each denoted by a leading `-` indicator. The `-` indicator must be separated from the node by white space."
- **Implementation:** `handle_sequence_entry()` in `event_iter/block/sequence.rs` — opens a new `CollectionEntry::Sequence` when `dash_indent > parent_col`, requiring sequences to be strictly more indented than their enclosing block node
- **Tests:** `tests/yaml-test-suite/src/JQ4R.yaml` (Spec Example 8.14. Block Sequence); `rlsp-yaml-parser/tests/smoke/sequences.rs`

### [184] c-l-block-seq-entry(n)

BNF: `c-l-block-seq-entry(n) ::= c-sequence-entry [ lookahead ≠ ns-char ] s-l+block-indented(n,BLOCK-IN)`

- **Verdict:** Strict-conformant
- **Spec (§8.2.1):** "The `-` indicator must be separated from the node by white space. This allows `-` to be used as the first character in a plain scalar if followed by a non-space character (e.g. `-42`)."
- **Implementation:** `peek_sequence_entry()` in `event_iter/block/sequence.rs` — requires the character after `-` to be empty, space, or tab; rejects `-` followed by non-space so that `-42` is a plain scalar
- **Tests:** `tests/yaml-test-suite/src/W42U.yaml` (Spec Example 8.15. Block Sequence Entry Types); `tests/yaml-test-suite/src/JQ4R.yaml`

### [185] s-l+block-indented(n,c)

BNF: `s-l+block-indented(n,c) ::= ( s-indent(m) ( ns-l-compact-sequence(n+1+m) | ns-l-compact-mapping(n+1+m) ) ) | s-l+block-node(n,c) | ( e-node s-l-comments )`

- **Verdict:** Strict-conformant
- **Spec (§8.2.1):** "The entry node may be either completely empty, be a nested block node or use a compact in-line notation."
- **Implementation:** `consume_sequence_dash()` and `handle_sequence_entry()` in `event_iter/block/sequence.rs` — inline compact mapping/sequence dispatched via subsequent `step_in_document()` calls; empty scalar emitted when no inline content and next line is not more indented
- **Tests:** `tests/yaml-test-suite/src/W42U.yaml`

### [186] ns-l-compact-sequence(n)

BNF: `ns-l-compact-sequence(n) ::= c-l-block-seq-entry(n) ( s-indent(n) c-l-block-seq-entry(n) )*`

- **Verdict:** Strict-conformant
- **Spec (§8.2.1):** "In this case, both the `-` indicator and the following spaces are considered to be part of the indentation of the nested collection. Note that it is not possible to specify node properties for such a collection."
- **Implementation:** `handle_sequence_entry()` in `event_iter/block/sequence.rs` — when a `-` appears as inline content after another `-`, the compact sequence opens at the column of the nested `-`
- **Tests:** `tests/yaml-test-suite/src/W42U.yaml` (compact sequence case)

### [187] l+block-mapping(n)

BNF: `l+block-mapping(n) ::= ( s-indent(n+1+m) ns-l-block-map-entry(n+1+m) )+`

- **Verdict:** Strict-conformant
- **Spec (§8.2.2):** "A Block mapping is a series of entries, each presenting a key/value pair."
- **Implementation:** `handle_mapping_entry()` in `event_iter/block/mapping.rs` — opens a new `CollectionEntry::Mapping` when not already in a mapping at this indent
- **Tests:** `tests/yaml-test-suite/src/TE2A.yaml` (Spec Example 8.16. Block Mappings); `rlsp-yaml-parser/tests/smoke/mappings.rs`

### [188] ns-l-block-map-entry(n)

BNF: `ns-l-block-map-entry(n) ::= c-l-block-map-explicit-entry(n) | ns-l-block-map-implicit-entry(n)`

- **Verdict:** Strict-conformant
- **Spec (§8.2.2):** "If the `?` indicator is specified, the optional value node must be specified on a separate line, denoted by the `:` indicator."
- **Implementation:** `peek_mapping_entry()` in `event_iter/block/mapping.rs` — recognises both explicit `?` key and implicit `key: value` forms
- **Tests:** `tests/yaml-test-suite/src/5WE3.yaml` (Spec Example 8.17. Explicit Block Mapping Entries); `tests/yaml-test-suite/src/S3PD.yaml`

### [189] c-l-block-map-explicit-entry(n)

BNF: `c-l-block-map-explicit-entry(n) ::= c-l-block-map-explicit-key(n) ( l-block-map-explicit-value(n) | e-node )`

- **Verdict:** Strict-conformant
- **Spec (§8.2.2):** "If the `?` indicator is specified, the optional value node must be specified on a separate line, denoted by the `:` indicator."
- **Implementation:** Explicit key branch in `event_iter/block/mapping.rs` — `?` followed by optional inline key content; absent value produces `e-node` / empty scalar
- **Tests:** `tests/yaml-test-suite/src/5WE3.yaml`

### [190] c-l-block-map-explicit-key(n)

BNF: `c-l-block-map-explicit-key(n) ::= c-mapping-key s-l+block-indented(n,BLOCK-OUT)`

- **Verdict:** Strict-conformant
- **Spec (§8.2.2):** "If the `?` indicator is specified, the optional value node must be specified on a separate line, denoted by the `:` indicator."
- **Implementation:** `?` followed by whitespace or end-of-line in `event_iter/block/mapping.rs` parsed as explicit key; inline key content prepended as a synthetic line for `s-l+block-indented` handling
- **Tests:** `tests/yaml-test-suite/src/5WE3.yaml`

### [191] l-block-map-explicit-value(n)

BNF: `l-block-map-explicit-value(n) ::= s-indent(n) c-mapping-value s-l+block-indented(n,BLOCK-OUT)`

- **Verdict:** Strict-conformant
- **Spec (§8.2.2):** "If the `?` indicator is specified, the optional value node must be specified on a separate line, denoted by the `:` indicator."
- **Implementation:** `consume_explicit_value_line()` in `event_iter/block/mapping.rs` — a line that is solely a `:` value indicator advances the mapping to Value phase; inline value content prepended as a synthetic line
- **Tests:** `tests/yaml-test-suite/src/5WE3.yaml`

### [192] ns-l-block-map-implicit-entry(n)

BNF: `ns-l-block-map-implicit-entry(n) ::= ( ns-s-block-map-implicit-key | e-node ) c-l-block-map-implicit-value(n)`

- **Verdict:** Strict-conformant
- **Spec (§8.2.2):** "Such keys are subject to the same restrictions; they are limited to a single line and must not span more than 1024 Unicode characters."
- **Implementation:** `consume_mapping_entry()` in `event_iter/block/mapping.rs` — 1024-Unicode-character limit checked against `trimmed[..colon_offset]` before the key span is built; returns `ConsumedMapping::ImplicitKeyTooLongError` on violation
- **Tests:** `rlsp-yaml-parser/tests/implicit_key_length.rs` (groups A–N and H5–H8, 48 cases)

### [193] ns-s-block-map-implicit-key

BNF: `ns-s-block-map-implicit-key ::= c-s-implicit-json-key(BLOCK-KEY) | ns-s-implicit-yaml-key(BLOCK-KEY)`

- **Verdict:** Strict-conformant
- **Spec (§8.2.2):** "Such keys are subject to the same restrictions; they are limited to a single line and must not span more than 1024 Unicode characters."
- **Implementation:** The 1024-char check in `consume_mapping_entry()` in `event_iter/block/mapping.rs` — covers both plain YAML-key and quoted JSON-key forms via the same guard
- **Tests:** `rlsp-yaml-parser/tests/implicit_key_length.rs` (groups A–N and H5–H8, 48 cases)

### [194] c-l-block-map-implicit-value(n)

BNF: `c-l-block-map-implicit-value(n) ::= c-mapping-value ( s-l+block-node(n,BLOCK-OUT) | ( e-node s-l-comments ) )`

- **Verdict:** Strict-conformant
- **Spec (§8.2.2):** "In block mappings the value must never be adjacent to the `:`, as this greatly reduces readability and is not required for JSON compatibility."
- **Implementation:** Value content after `: ` / `:\t` in `event_iter/block/mapping.rs` prepended as a synthetic inline line; absent value content produces empty scalar via `e-node` path
- **Tests:** `tests/yaml-test-suite/src/S3PD.yaml` (Spec Example 8.18. Implicit Block Mapping Entries)

### [195] ns-l-compact-mapping(n)

BNF: `ns-l-compact-mapping(n) ::= ns-l-block-map-entry(n) ( s-indent(n) ns-l-block-map-entry(n) )*`

- **Verdict:** Strict-conformant
- **Spec (§8.2.2):** "A compact in-line notation is also available. This compact notation may be nested inside block sequences and explicit block mapping entries. Note that it is not possible to specify node properties for such a nested mapping."
- **Implementation:** `handle_mapping_entry()` in `event_iter/block/mapping.rs` — compact mapping opened when a `key: value` pair appears inline after `-` or after `? ` indicator
- **Tests:** `tests/yaml-test-suite/src/V9D5.yaml` (Spec Example 8.19. Compact Block Mappings); `tests/yaml-test-suite/src/W42U.yaml` (compact mapping case)

### [196] s-l+block-node(n,c)

BNF: `s-l+block-node(n,c) ::= s-l+block-in-block(n,c) | s-l+flow-in-block(n)`

- **Verdict:** Strict-conformant
- **Spec (§8.3):** "YAML allows flow nodes to be embedded inside block collections (but not vice-versa). Flow nodes must be indented by at least one more space than the parent block collection."
- **Implementation:** `step_in_document()` in `event_iter/step.rs` — dispatches to flow or block handling based on first character of the next line: `[`, `{`, `'`, `"` → flow; `|`, `>` → block scalar; plain/mapping/sequence → block collection
- **Tests:** `tests/yaml-test-suite/src/735Y.yaml` (Spec Example 8.20. Block Node Types)

### [197] s-l+flow-in-block(n)

BNF: `s-l+flow-in-block(n) ::= s-separate(n+1,FLOW-OUT) ns-flow-node(n+1,FLOW-OUT) s-l-comments`

- **Verdict:** Strict-conformant
- **Spec (§8.3):** "YAML allows flow nodes to be embedded inside block collections (but not vice-versa). Flow nodes must be indented by at least one more space than the parent block collection."
- **Implementation:** Flow nodes dispatched from `step_in_document()` in `event_iter/step.rs` when first character is a flow indicator; indentation relative to parent enforced by `close_collections_at_or_above()` in `event_iter/flow.rs`
- **Tests:** `tests/yaml-test-suite/src/735Y.yaml` (`"flow in block"` case)

### [198] s-l+block-in-block(n,c)

BNF: `s-l+block-in-block(n,c) ::= s-l+block-scalar(n,c) | s-l+block-collection(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§8.3):** "The block node's properties may span across several lines. In this case, they must be indented by at least one more space than the block collection."
- **Implementation:** Dispatch in `step_in_document()` in `event_iter/step.rs` — `|` / `>` first character → `try_consume_literal_block_scalar()` / `try_consume_folded_block_scalar()`; otherwise → block collection handling
- **Tests:** `tests/yaml-test-suite/src/735Y.yaml`; `tests/yaml-test-suite/src/M5C3.yaml`

### [199] s-l+block-scalar(n,c)

BNF: `s-l+block-scalar(n,c) ::= s-separate(n+1,c) ( c-ns-properties(n+1,c) s-separate(n+1,c) )? ( c-l+literal(n) | c-l+folded(n) )`

- **Verdict:** Strict-conformant
- **Spec (§8.3):** "The block node's properties may span across several lines."
- **Implementation:** Tag/anchor properties scanned before `|`/`>` dispatch via `step_in_document()` property-handling path in `event_iter/step.rs`; `lexer/block.rs` handles literal and folded
- **Tests:** `tests/yaml-test-suite/src/M5C3.yaml` (Spec Example 8.21. Block Scalar Nodes)

### [200] s-l+block-collection(n,c)

BNF: `s-l+block-collection(n,c) ::= ( s-separate(n+1,c) c-ns-properties(n+1,c) )? s-l-comments ( seq-space(n,c) | l+block-mapping(n) )`

- **Verdict:** Strict-conformant
- **Spec (§8.3):** "Since people perceive the `-` indicator as indentation, nested block sequences may be indented by one less space to compensate."
- **Implementation:** `seq-space` rule in `event_iter/block/sequence.rs` — `CollectionEntry::Mapping(col, MappingPhase::Value, _)` allows a sequence to open at `dash_indent >= col`, implementing the one-less-indent compensation for `BLOCK-OUT` context
- **Tests:** `tests/yaml-test-suite/src/57H4.yaml` (Spec Example 8.22. Block Collection Nodes)

### [201] seq-space(n,c)

BNF: `seq-space(n,BLOCK-OUT) ::= l+block-sequence(n-1)` / `seq-space(n,BLOCK-IN) ::= l+block-sequence(n)`

- **Verdict:** Strict-conformant
- **Spec (§8.3):** "Since people perceive the `-` indicator as indentation, nested block sequences may be indented by one less space to compensate, except, of course, if nested inside another block sequence (BLOCK-OUT context versus BLOCK-IN context)."
- **Implementation:** Implemented implicitly in `event_iter/block/sequence.rs` — when `MappingPhase::Value`, `dash_indent >= col` accepted (n-1 case); when `CollectionEntry::Sequence`, only `dash_indent > parent_col` opens a new sequence (n case)
- **Tests:** `tests/yaml-test-suite/src/57H4.yaml`; `rlsp-yaml-parser/tests/smoke/sequences.rs`
