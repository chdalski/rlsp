---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: B
section: §8
date: 2026-04-30
---

### [162] c-b-block-header(t)

BNF: `c-b-block-header(t) ::= ( ( c-indentation-indicator c-chomping-indicator(t) ) | ( c-chomping-indicator(t) c-indentation-indicator ) ) s-b-comment`
Spec prose: §8.1.1: "Block scalars are controlled by a few indicators given in a header preceding the content itself. This header is followed by a non-content line break with an optional comment. This is the only case where a comment must not be followed by additional comment lines."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:500-619` (`parse_block_header`); dispatch sites at `src/lexer/block.rs:71-75` (literal) and `src/lexer/block.rs:310-315` (folded).
Reasoning: `parse_block_header` accepts indicator chars in any order, rejecting duplicate chomp indicators (lines 533-541, 547-557), duplicate indent indicators (lines 574-583), invalid characters (lines 588-598), and zero indent (lines 562-572). After indicator consumption the loop terminates on whitespace or line end, then the trailing-content check at lines 604-616 enforces that only `[ \t]*` followed by optional `#`-comment may appear before EOL. This composes the two BNF alternatives correctly. The `s-b-comment` part terminates parsing at `\n` or `\r`. Conformance doc agrees.

### [163] c-indentation-indicator

BNF: `c-indentation-indicator ::= [x31-x39]    # 1-9`
Spec prose: §8.1.1.1: "If a block scalar has an indentation indicator, then the content indentation level of the block scalar is equal to the indentation level of the block scalar plus the integer value of the indentation indicator character."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:562-587` (`'0'` rejected with explicit error; `'1'..='9'` parsed via `ch as usize - '0' as usize`).
Reasoning: The accepted character class is exactly `b'1'..=b'9'`. The literal-zero arm at lines 562-572 returns an error message naming `'0'` as not valid. Subsequent digit consumption is gated by the `explicit_indent.is_some()` duplicate check, so multi-digit indicators (e.g. `|99`) are rejected. Mapping the digit to a `usize` value is exact. Conformance doc agrees.

### [164] c-chomping-indicator(t)

BNF: `c-chomping-indicator(STRIP) ::= '-'` / `c-chomping-indicator(KEEP) ::= '+'` / `c-chomping-indicator(CLIP) ::= ""`
Spec prose: §8.1.1.2: "Stripping is specified by the '-' chomping indicator. […] Clipping is the default behavior used if no explicit chomping indicator is specified. […] Keeping is specified by the '+' chomping indicator."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:532-561` (`'+'`→`Chomp::Keep`, `'-'`→`Chomp::Strip`); default at line 618 (`chomp.unwrap_or(Chomp::Clip)`).
Reasoning: The match arms for `+` and `-` are exact; the default fallthrough returns `Chomp::Clip`. Duplicate-indicator detection in those arms (`if chomp.is_some()`) prevents `++`, `--`, `+-`, `-+` from being silently accepted — matches the BNF where the indicator slot is single. Conformance doc agrees.

### [165] b-chomped-last(t)

BNF: `b-chomped-last(STRIP) ::= b-non-content | <end-of-input>` / `b-chomped-last(CLIP) ::= b-as-line-feed | <end-of-input>` / `b-chomped-last(KEEP) ::= b-as-line-feed | <end-of-input>`
Spec prose: §8.1.1.2: "The interpretation of the final line break of a block scalar is controlled by the chomping indicator specified in the block scalar header."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:634-664` (`apply_chomping`).
Reasoning: For Strip, the trailing `\n` from the last content line is popped (lines 640-647), suppressing it as `b-non-content`. For Clip, exactly one trailing `\n` is preserved, with the EOF case handled at lines 654-656 — so `<end-of-input>` after a content line still produces a single `\n` per spec note "the final line break, if any, is preserved." (This is the specced clip behavior — the YAML 1.2.2 erratum corrects production [165] CLIP to allow `<end-of-input>` and have an implied trailing line feed). For Keep, the trailing `\n` is retained and `trailing_blank_count` is appended via `repeat_n` (lines 658-661). All three branches match the BNF semantics.

### [166] l-chomped-empty(n,t)

BNF: `l-chomped-empty(n,STRIP) ::= l-strip-empty(n)` / `l-chomped-empty(n,CLIP) ::= l-strip-empty(n)` / `l-chomped-empty(n,KEEP) ::= l-keep-empty(n)`
Spec prose: §8.1.1.2: "The interpretation of the trailing empty lines following a block scalar is also controlled by the chomping indicator specified in the block scalar header."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:245-260` (literal blank-line accumulation); `src/lexer/block.rs:447-476` (folded blank-line accumulation); `src/lexer/block.rs:639-661` (`apply_chomping` consumes vs. preserves).
Reasoning: Trailing whitespace-only lines are counted in `trailing_newlines` for both literal and folded. `apply_chomping` then maps the count by chomp mode: Strip discards the `trailing_newlines` field implicitly (it doesn't append them), Clip discards them likewise, Keep appends them via `extend(repeat_n('\n', trailing_blank_count))`. This matches the dispatch in the BNF (Strip and Clip → strip-empty; Keep → keep-empty). Conformance doc agrees.

### [167] l-strip-empty(n)

BNF: `l-strip-empty(n) ::= ( s-indent-less-or-equal(n) b-non-content )* l-trail-comments(n)?`
Spec prose: §8.1.1.2: "The interpretation of the trailing empty lines following a block scalar is also controlled by the chomping indicator specified in the block scalar header."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:245-260` (literal); `src/lexer/block.rs:447-476` (folded); discard via Strip/Clip branches at `src/lexer/block.rs:640-657`.
Reasoning: Whitespace-only blank lines below `content_indent` are absorbed by the blank-line branch in the loop and counted but not pushed into `out`. The Strip and Clip branches in `apply_chomping` ignore `trailing_blank_count`, so the blank lines disappear from the value — matching `l-strip-empty`'s "empty lines stripped, no contribution to content." `l-trail-comments(n)?` (production [169]) is delegated to higher-level comment handling (the loop terminates when content dedents to a non-whitespace line, allowing the comment lexer to reach those lines). Conformance doc agrees.

### [168] l-keep-empty(n)

BNF: `l-keep-empty(n) ::= l-empty(n,BLOCK-IN)* l-trail-comments(n)?`
Spec prose: §8.1.1.2: "Keeping is specified by the '+' chomping indicator. In this case, the final line break and any trailing empty lines are considered to be part of the scalar's content."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:245-260` (blank-line counter); `src/lexer/block.rs:658-661` (`Chomp::Keep` branch).
Reasoning: Keep's branch in `apply_chomping` calls `content.extend(std::iter::repeat_n('\n', trailing_blank_count))`, which preserves N newlines for N trailing blank lines. Combined with the preserved last-content `\n`, this matches "final line break and any trailing empty lines are considered part of the scalar's content." Conformance doc agrees.

### [169] l-trail-comments(n)

BNF: `l-trail-comments(n) ::= s-indent-less-than(n) c-nb-comment-text b-comment l-comment*`
Spec prose: §8.1.1.2: "Explicit comment lines may follow the trailing empty lines. To prevent ambiguity, the first such comment line must be less indented than the block scalar content. Additional comment lines, if any, are not so restricted. This is the only case where the indentation of comment lines is constrained."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:245-252` (literal): a non-whitespace line whose indent is below `content_indent` terminates the scalar via `break`, leaving subsequent comment lines for the document-level comment scanner; `src/lexer/block.rs:447-451` (folded) does the same.
Reasoning: The block-scalar loop hands control back to the document-level dispatcher when a less-indented non-blank line appears, where the comment scanner (`skip_and_collect_comments_in_doc` referenced from `step.rs:26`) processes `#`-prefixed lines. The BNF `s-indent-less-than(n)` is enforced by the dedent-terminator branch (line 248-251) only firing when `next.indent < content_indent` AND content is non-whitespace — a comment line at less indent dedents the scalar. The "less indented" requirement of the first comment line is precisely the boundary the loop uses to exit. Conformance doc agrees.

### [170] c-l+literal(n)

BNF: `c-l+literal(n) ::= c-literal c-b-block-header(t) l-literal-content(n+m,t)`
Spec prose: §8.1.2: "The literal style is denoted by the '|' indicator. It is the simplest, most restricted and most readable scalar style."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:41-271` (`try_consume_literal_block_scalar`), `c-literal` recognized at lines 47-50; `c-b-block-header` at lines 71-75 (delegates to [162]); `l-literal-content` driven by the loop at 117-261 plus `apply_chomping` at 266; auto-indent detection at 95-115.
Reasoning: The function dispatches on `|`, parses the header (production [162]), computes `content_indent = parent_indent + m` where `m` is either the explicit indicator value or auto-detected via `peek_until_dedent` and the first non-blank line's indent (lines 109-115). The composition of header + content production matches the BNF. Auto-detection of `m` matches the spec's "if no indicator, auto-detect from first non-empty line" rule. Conformance doc agrees.

### [171] l-nb-literal-text(n)

BNF: `l-nb-literal-text(n) ::= l-empty(n,BLOCK-IN)* s-indent(n) nb-char+`
Spec prose: §8.1.2: "Inside literal scalars, all (indented) characters are considered to be content, including white space characters. Note that all line break characters are normalized."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:181-244`: line classification logic identifies content lines as `indent >= content_indent` AND non-empty after stripping the indent prefix.
Reasoning: Leading `l-empty(n,BLOCK-IN)*` is realized by the blank-line branch (lines 245-260) accumulating into `trailing_newlines` and flushing them at line 229 before the next content line is appended. `s-indent(n)` is enforced by `next.indent >= content_indent` (line 198). `nb-char+` content is `after_indent = line_content.get(content_indent..)` (line 182) and pushed verbatim (line 238) — `nb-char` includes spaces and tabs, which is what the parser preserves. The over-indented-blank-line guard at lines 208-223 enforces the §8.1.1.1 invariant that leading empty lines must not exceed the first non-empty line's indent. Conformance doc agrees.

### [172] b-nb-literal-next(n)

BNF: `b-nb-literal-next(n) ::= b-as-line-feed l-nb-literal-text(n)`
Spec prose: §8.1.2: "Inside literal scalars, all (indented) characters are considered to be content, including white space characters. Note that all line break characters are normalized."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:228-244` (the content-line emission path inside the loop).
Reasoning: Each content line pushes its `after_indent` slice and then appends `\n` if `consumed.break_type != BreakType::Eof` (line 242-244) — this is the `b-as-line-feed` between successive `l-nb-literal-text` segments. The line-break normalization to `\n` is performed at `lines.rs` ingest time (the `Line` records expose `break_type` independent of the original CR/CRLF), and only `\n` is ever written to the output. Conformance doc agrees.

### [173] l-literal-content(n,t)

BNF: `l-literal-content(n,t) ::= ( l-nb-literal-text(n) b-nb-literal-next(n)* b-chomped-last(t) )? l-chomped-empty(n,t)`
Spec prose: §8.1.2: "In addition, empty lines are not folded, though final line breaks and trailing empty lines are chomped."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:117-271` (the full collection loop); `apply_chomping` invocation at line 266.
Reasoning: The optional first content section is realized by the loop running zero or more iterations on content lines. The final newline is appended by the last content line's emit at lines 242-244 (or omitted at EOF). `apply_chomping(out, trailing_newlines, chomp)` then evaluates `b-chomped-last(t)` and `l-chomped-empty(n,t)` together — Strip pops the final `\n`, Clip preserves exactly one (with EOF handling), Keep appends the trailing-blank count. The composition matches the BNF. Conformance doc agrees.

### [174] c-l+folded(n)

BNF: `c-l+folded(n) ::= c-folded c-b-block-header(t) l-folded-content(n+m,t)`
Spec prose: §8.1.3: "The folded style is denoted by the '>' indicator. It is similar to the literal style; however, folded scalars are subject to line folding."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:285-345` (`try_consume_folded_block_scalar`); `c-folded` recognized at line 291; header at lines 311-315; content via `collect_folded_lines` at line 337; chomping at line 342.
Reasoning: The structure mirrors literal but uses `collect_folded_lines` for the body. The header parser is the same `parse_block_header` (production [162]), so indicator handling is identical. Auto-detect `m` follows the same first-non-blank-line rule (lines 329-335). Conformance doc agrees.

### [175] s-nb-folded-text(n)

BNF: `s-nb-folded-text(n) ::= s-indent(n) ns-char nb-char*`
Spec prose: §8.1.3: "Folding allows long lines to be broken anywhere a single space character separates two non-space characters."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:396-407` in `collect_folded_lines` — `is_content_line` requires `indent >= content_indent` AND `!after_indent.trim_end_matches(' ').is_empty()`.
Reasoning: The classifier's "trim trailing spaces and require non-empty" predicate enforces that the line begins with at least one non-space character within the first `content_indent`-stripped slice — directly matching `ns-char nb-char*`. Tabs are intentionally treated as content (per the comment at lines 401-405): a `\t`-prefixed `after_indent` IS a content line. Note this means a line like `\t  ` (tabs + spaces) would be a content line while `   ` would not — consistent with §8.1.1's `nb-char` set which includes tab. Conformance doc agrees.

### [176] l-nb-folded-lines(n)

BNF: `l-nb-folded-lines(n) ::= s-nb-folded-text(n) ( b-l-folded(n,BLOCK-IN) s-nb-folded-text(n) )*`
Spec prose: §8.1.3: "Folding allows long lines to be broken anywhere a single space character separates two non-space characters."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:415-446` — between two content lines that are NOT more-indented and have no intervening blank lines, the inter-line break becomes a single space (`out.push(' ')` at line 428).
Reasoning: When `prev_more_indented` is false, current line is not more-indented, and `trailing_newlines == 0`, the deferred break folds to a space. This is the spec's `b-l-folded(BLOCK-IN)` semantics for the "single break, both lines equally indented" case. The `else` arm (line 426-429) is exactly this case. Conformance doc agrees.

### [177] s-nb-spaced-text(n)

BNF: `s-nb-spaced-text(n) ::= s-indent(n) s-white nb-char*`
Spec prose: §8.1.3: "Lines starting with white space characters (more-indented lines) are not folded."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:414-415`: `is_more_indented = next.indent > content_indent || after_indent.starts_with([' ', '\t'])`.
Reasoning: `is_more_indented` flags lines whose effective content begins with a space or tab — which is `s-white` per the spec. The composition with `is_content_line` (lines 406-407) means the predicate fires on a line that has BOTH indent ≥ content_indent AND a leading whitespace character on its content prefix. Conformance doc agrees.

### [178] b-l-spaced(n)

BNF: `b-l-spaced(n) ::= b-as-line-feed l-empty(n,BLOCK-IN)*`
Spec prose: §8.1.3: "Lines starting with white space characters (more-indented lines) are not folded."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:417-425`: when blank lines (`trailing_newlines > 0`) lie between content lines and either side is more-indented, `extra = 1` is added to `trailing_newlines` so the break adjacent to the spaced line is preserved as `\n`.
Reasoning: The branching logic at lines 416-429 matches the §8.1.3 rule: a line break adjacent to a more-indented line is preserved as a literal newline rather than folded to a space. The N-blank-lines case (`trailing_newlines > 0`) appends `N + extra` newlines, where `extra` accounts for the preserved boundary break. This corresponds to `b-l-spaced(n)`'s "the break is preserved." Conformance doc agrees.

### [179] l-nb-spaced-lines(n)

BNF: `l-nb-spaced-lines(n) ::= s-nb-spaced-text(n) ( b-l-spaced(n) s-nb-spaced-text(n) )*`
Spec prose: §8.1.3: "Lines starting with white space characters (more-indented lines) are not folded."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:415-446` — consecutive `is_more_indented` content lines join with `\n` (lines 423-425) rather than a space.
Reasoning: When two consecutive content lines are both more-indented and there are no blank lines between, `prev_more_indented || is_more_indented` is true and the path at lines 423-425 fires, pushing a single `\n`. This is the BNF's `( b-l-spaced(n) s-nb-spaced-text(n) )*` case where each `b-l-spaced(n)` collapses to one preserved newline (no blanks → `l-empty*` is empty). Conformance doc agrees.

### [180] l-nb-same-lines(n)

BNF: `l-nb-same-lines(n) ::= l-empty(n,BLOCK-IN)* ( l-nb-folded-lines(n) | l-nb-spaced-lines(n) )`
Spec prose: §8.1.3: "Line breaks and empty lines separating folded and more-indented lines are also not folded."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:430-435` — for the first content line, leading blank lines are flushed as literal `\n`s before the line content is appended.
Reasoning: The `else` branch at lines 430-434 (`!has_content`) handles the leading-`l-empty*` prefix: each accumulated `trailing_newline` becomes one literal `\n` in the output before the first content line lands. The subsequent classification (folded vs. spaced) is then driven by `is_more_indented`, which selects which production [176]/[179] applies. Conformance doc agrees.

### [181] l-nb-diff-lines(n)

BNF: `l-nb-diff-lines(n) ::= l-nb-same-lines(n) ( b-as-line-feed l-nb-same-lines(n) )*`
Spec prose: §8.1.3: "Line breaks and empty lines separating folded and more-indented lines are also not folded."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:415-446` — the full body loop, with `prev_more_indented` tracking transitions between folded and spaced groups.
Reasoning: When the loop transitions from a folded line to a spaced line (or vice versa), `prev_more_indented` carries the previous classification and, combined with the current `is_more_indented`, selects the preserve-as-`\n` path (line 423-425) or the `extra+1` path (line 421-422) when blanks intervene. This realizes `( b-as-line-feed l-nb-same-lines(n) )*` between groups: the boundary break is preserved literally rather than folded. Conformance doc agrees.

### [182] l-folded-content(n,t)

BNF: `l-folded-content(n,t) ::= ( l-nb-diff-lines(n) b-chomped-last(t) )? l-chomped-empty(n,t)`
Spec prose: §8.1.3: "The final line break and trailing empty lines if any, are subject to chomping and are never folded."
Verdict: Strict-conformant
Evidence: `src/lexer/block.rs:480-485` (final break appended) and `src/lexer/block.rs:340-342` (`apply_chomping` invocation on assembled content).
Reasoning: After the loop, if `has_content && last_had_break`, the final `\n` is appended to `out` so `apply_chomping` sees a canonically `\n`-terminated string — that final `\n` is `b-chomped-last`. The `(content, trailing_newlines)` return then drives `apply_chomping` which evaluates both the final break (Strip pops it; Clip preserves; Keep preserves) and the trailing empties (Strip/Clip discard; Keep appends). Conformance doc agrees.

### [183] l+block-sequence(n)

BNF: `l+block-sequence(n) ::= ( s-indent(n+1+m) c-l-block-seq-entry(n+1+m) )+`
Spec prose: §8.2.1: "A block sequence is simply a series of nodes, each denoted by a leading '-' indicator. The '-' indicator must be separated from the node by white space."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/sequence.rs:129-143` (`opens_new` predicate); subsequent iterations stay open via `opens_new=false` when `dash_indent == seq_col` (line 131).
Reasoning: A block sequence opens when `dash_indent > parent_col` (the n+1+m rule) — this is the strict-greater check at line 131. The same opener serves all entries: each subsequent dash at the same column is recognized as a sibling entry rather than a new collection. The `seq-space` (production [201]) variant for BLOCK-OUT context is composed via the `MappingPhase::Value` arm (line 142). The `+` (one-or-more) is implicit in the sequence opening: once opened, the loop stays open until dedent. Conformance doc agrees.

### [184] c-l-block-seq-entry(n)

BNF: `c-l-block-seq-entry(n) ::= c-sequence-entry [ lookahead ≠ ns-char ] s-l+block-indented(n,BLOCK-IN)`
Spec prose: §8.2.1: "A block sequence is simply a series of nodes, each denoted by a leading '-' indicator. The '-' indicator must be separated from the node by white space. This allows '-' to be used as the first character in a plain scalar if followed by a non-space character (e.g. '-42')."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/sequence.rs:37-45` (`peek_sequence_entry` requires the byte after `-` to be empty / `' '` / `'\t'`).
Reasoning: The lookahead constraint `[ lookahead ≠ ns-char ]` is implemented by `is_entry = after_dash.is_empty() || after_dash.starts_with(' ') || after_dash.starts_with('\t')` — anything else (including any `ns-char`) returns `None`, leaving the line for plain-scalar parsing. The `\n`/`\r` cases are subsumed by `is_empty()` since `after_dash` is the trimmed line content (no terminator). Conformance doc agrees.

### [185] s-l+block-indented(n,c)

BNF: `s-l+block-indented(n,c) ::= ( s-indent(m) ( ns-l-compact-sequence(n+1+m) | ns-l-compact-mapping(n+1+m) ) ) | s-l+block-node(n,c) | ( e-node s-l-comments )`
Spec prose: §8.2.1: "The entry node may be either completely empty, be a nested block node or use a compact in-line notation. The compact notation may be used when the entry is itself a nested block collection."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/sequence.rs:275-477` — after `consume_sequence_dash` prepends a synthetic line for inline content, the next dispatch in `step_in_document` handles it as a block node (compact mapping/sequence, scalar, etc.). The empty-entry path is at lines 453-477.
Reasoning: When `had_inline=true`, the synthetic line carries the inline content at column `dash_indent + 1 + spaces_after_dash`, which is `n+1+m` — the next dispatch chooses between compact sequence (`-` followed by inline `-` becomes another nested dash), compact mapping (`key: value` inline), or a block scalar. When `had_inline=false` and the next line is not more-indented than `dash_indent`, an empty plain scalar is emitted (lines 459-477) — this is `e-node s-l-comments`. The fast-path scalar emission at 297-451 covers the inline plain-scalar case. Composition matches the BNF.

### [186] ns-l-compact-sequence(n)

BNF: `ns-l-compact-sequence(n) ::= c-l-block-seq-entry(n) ( s-indent(n) c-l-block-seq-entry(n) )*`
Spec prose: §8.2.1: "The compact notation may be used when the entry is itself a nested block collection. In this case, both the '-' indicator and the following spaces are considered to be part of the indentation of the nested collection. Note that it is not possible to specify node properties for such a collection."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/sequence.rs:60-103` (`consume_sequence_dash` builds the synthetic inline at `dash_indent + 1 + spaces_after_dash`); subsequent dash at the synthetic column triggers `peek_sequence_entry` and opens a nested sequence at that column via `handle_sequence_entry`.
Reasoning: The synthetic-line column corresponds to "the dash and following spaces are part of the indentation" — the inline becomes its own `Line` with `indent` set to the dash column plus the dash-and-spacing offset. When this inline line begins with another `-`, `peek_sequence_entry` matches at that column and `handle_sequence_entry` opens a nested sequence (`opens_new = true` because `dash_indent > parent_col`). The repetition `( s-indent(n) c-l-block-seq-entry(n) )*` is realized by physical sibling lines at the same column staying within the open sequence. Conformance doc agrees.

### [187] l+block-mapping(n)

BNF: `l+block-mapping(n) ::= ( s-indent(n+1+m) ns-l-block-map-entry(n+1+m) )+`
Spec prose: §8.2.2: "A Block mapping is a series of entries, each presenting a key/value pair."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/mapping.rs:431-516` — opens a new mapping when `is_in_mapping_at_this_indent` is false (line 435), with the new mapping's column being `effective_key_indent`.
Reasoning: The mapping opens on the first key at column `n+1+m` relative to its parent: at line 435, when no mapping is already open at this indent and the dispatcher sees a key line, `MappingStart` is emitted and a new `CollectionEntry::Mapping(effective_key_indent, Key, false)` is pushed (lines 512-516). Subsequent keys at the same column reuse the existing mapping (the predicate at line 431 returns true). The dispatch from `step_in_document` reaches `handle_mapping_entry` only when `peek_mapping_entry` returned a key line. Conformance doc agrees.

### [188] ns-l-block-map-entry(n)

BNF: `ns-l-block-map-entry(n) ::= c-l-block-map-explicit-entry(n) | ns-l-block-map-implicit-entry(n)`
Spec prose: §8.2.2: "If the '?' indicator is specified, the optional value node must be specified on a separate line, denoted by the ':' indicator."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/mapping.rs:34-69` (`peek_mapping_entry`); `src/event_iter/block/mapping.rs:107-150` (explicit branch in `consume_mapping_entry`); `src/event_iter/block/mapping.rs:153-310` (implicit branch).
Reasoning: `peek_mapping_entry` accepts either `?` (explicit) or `:` (implicit) shapes. `consume_mapping_entry` then dispatches: `if let Some(after_q) = trimmed.strip_prefix('?')` opens the explicit branch; otherwise it falls through to the implicit branch using `find_value_indicator_offset`. The two BNF alternatives map to these two branches exactly. Conformance doc agrees.

### [189] c-l-block-map-explicit-entry(n)

BNF: `c-l-block-map-explicit-entry(n) ::= c-l-block-map-explicit-key(n) ( l-block-map-explicit-value(n) | e-node )`
Spec prose: §8.2.2: "If the '?' indicator is specified, the optional value node must be specified on a separate line, denoted by the ':' indicator."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/mapping.rs:107-150` (`?` branch in `consume_mapping_entry`); `src/event_iter/block/mapping.rs:644-668` and `src/event_iter/base.rs:94-127` (e-node fallback when the value indicator is missing and the mapping closes via dedent).
Reasoning: The explicit `?` is consumed and either inline key content is prepended as a synthetic line (lines 127-145) or the bare `?` triggers `complex_key_inline` tracking (line 729-737). When no `:` value-indicator follows before the mapping closes, the close path at `base.rs:94-127` emits the missing key/value as null scalars. The `e-node` alternative is correctly realized. Conformance doc agrees.

### [190] c-l-block-map-explicit-key(n)

BNF: `c-l-block-map-explicit-key(n) ::= c-mapping-key s-l+block-indented(n,BLOCK-OUT)`
Spec prose: §8.2.2: "If the '?' indicator is specified, the optional value node must be specified on a separate line, denoted by the ':' indicator."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/mapping.rs:115-150` (recognizes `?` as the key indicator only when followed by whitespace/EOL, then prepends inline content as a synthetic line).
Reasoning: The check `is_explicit_key = after_q.is_empty() || after_q.starts_with(' ') || ...` matches `c-mapping-key`'s lookahead constraint exactly. Inline key content becomes a synthetic line at the key-indent column for the next dispatch to handle as a block-indented node — directly composing with `s-l+block-indented(n, BLOCK-OUT)`. Conformance doc agrees.

### [191] l-block-map-explicit-value(n)

BNF: `l-block-map-explicit-value(n) ::= s-indent(n) c-mapping-value s-l+block-indented(n,BLOCK-OUT)`
Spec prose: §8.2.2: "If the '?' indicator is specified, the optional value node must be specified on a separate line, denoted by the ':' indicator."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/mapping.rs:792-859` (`consume_explicit_value_line`) and the dispatch at `src/event_iter/block/mapping.rs:583-639`.
Reasoning: The `is_value_indicator_line` check at line 792-806 verifies the line begins with `:` followed by whitespace or EOL, after stripping leading spaces — i.e., `s-indent(n) c-mapping-value`. The function then either prepends inline value content as a synthetic line (lines 832-849) for subsequent block-node dispatch, or advances to Value phase to collect the value from following lines. The composition matches `c-mapping-value s-l+block-indented(n, BLOCK-OUT)`. Conformance doc agrees.

### [192] ns-l-block-map-implicit-entry(n)

BNF: `ns-l-block-map-implicit-entry(n) ::= ( ns-s-block-map-implicit-key | e-node ) c-l-block-map-implicit-value(n)`
Spec prose: §8.2.2: "If the '?' indicator is omitted, parsing needs to see past the implicit key, in the same way as in the single key/value pair flow mapping. Hence, such keys are subject to the same restrictions; they are limited to a single line and must not span more than 1024 Unicode characters."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/mapping.rs:161-172` (1024-char limit check); `src/event_iter/block/mapping.rs:153-310` (key extraction + value-line synthetic insertion); empty-key (`e-node`) path via `: value` recognized at `step.rs:886-888` followed by handle_mapping_entry's empty-key emission paths.
Reasoning: `trimmed[..colon_offset].chars().count() > 1024` enforces the §8.2.2 character limit using Unicode `chars()` (not bytes). The empty-key alternative (`e-node` for the key) is realized when a line consists of `:` followed by whitespace — then peek_mapping_entry sees a value-indicator-only line and the empty scalar is emitted from the Key-phase guard at lines 595-614. The single-line restriction is enforced because `find_value_indicator_offset` only scans the current physical line. Conformance doc agrees.

### [193] ns-s-block-map-implicit-key

BNF: `ns-s-block-map-implicit-key ::= c-s-implicit-json-key(BLOCK-KEY) | ns-s-implicit-yaml-key(BLOCK-KEY)`
Spec prose: §8.2.2: "Hence, such keys are subject to the same restrictions; they are limited to a single line and must not span more than 1024 Unicode characters."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/mapping.rs:161-172` (length check); `src/event_iter/block/mapping.rs:200-269` (quoted vs. plain key dispatch via `key_is_quoted = matches!(key_content.as_bytes().first(), Some(b'"' | b'\''))`).
Reasoning: The key class is detected by leading byte: `"` or `'` routes to the quoted-key decoder (lines 224-266) which calls `try_consume_single_quoted` / `try_consume_double_quoted` (the JSON-key class); otherwise the key is emitted as a plain scalar borrowing the slice. The 1024-char limit applies uniformly to both classes — same `trimmed[..colon_offset].chars().count() > 1024` guard. The single-physical-line restriction is enforced by `consume_mapping_entry` operating on one `Line`. Conformance doc agrees.

### [194] c-l-block-map-implicit-value(n)

BNF: `c-l-block-map-implicit-value(n) ::= c-mapping-value ( s-l+block-node(n,BLOCK-OUT) | ( e-node s-l-comments ) )`
Spec prose: §8.2.2: "In this case, the value may be specified on the same line as the implicit key. Note however that in block mappings the value must never be adjacent to the ':', as this greatly reduces readability and is not required for JSON compatibility (unlike the case in flow mappings). There is no compact notation for in-line values. Also, while both the implicit key and the value following it may be empty, the ':' indicator is mandatory. This prevents a potential ambiguity with multi-line plain scalars."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/mapping.rs:296-303` (inline value synthetic-line prepend); `src/event_iter/block/mapping.rs:271-294` (rejects illegal inline shapes); `src/event_iter/block/mapping.rs:670-696` (Value-phase empty-scalar path when no inline value and the next line is another key).
Reasoning: After the `:` is consumed, inline non-comment content is prepended as a synthetic line at column `value_col = key_indent + 1 + spaces_after_colon` for the next dispatch to handle as a block node. When no inline content follows (or only a comment follows), the Value phase is entered and a subsequent line either supplies the block node or — when the next entry is another key at the same indent — the Value-phase guard at lines 670-696 emits an `e-node` empty scalar. The "value must never be adjacent to the ':'" rule is enforced indirectly via `find_value_indicator_offset` only matching `:` followed by `[ \t\n\r]` (not by `ns-char`). Conformance doc agrees.

### [195] ns-l-compact-mapping(n)

BNF: `ns-l-compact-mapping(n) ::= ns-l-block-map-entry(n) ( s-indent(n) ns-l-block-map-entry(n) )*`
Spec prose: §8.2.2: "A compact in-line notation is also available. This compact notation may be nested inside block sequences and explicit block mapping entries. Note that it is not possible to specify node properties for such a nested mapping."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/sequence.rs:78-100` and `src/event_iter/block/mapping.rs:127-145` (synthetic inline lines for content following `-` and `?` carry column positions that allow the next dispatch to open a mapping at that synthetic column).
Reasoning: When the inline content after `-` or `?` is itself a `key: value`, the synthetic line is positioned at the inline column. The next dispatch into `peek_mapping_entry` recognizes the implicit-mapping shape and `handle_mapping_entry` opens a new mapping at that column (since none is already open there). Subsequent sibling keys at the same column attach to the same compact mapping. Properties (anchors/tags) on the compact mapping are inherently disallowed because the inline column is already past where standalone properties could appear — matching the spec note about no properties on compact collections. Conformance doc agrees.

### [196] s-l+block-node(n,c)

BNF: `s-l+block-node(n,c) ::= s-l+block-in-block(n,c) | s-l+flow-in-block(n)`
Spec prose: §8.3: "YAML allows flow nodes to be embedded inside block collections (but not vice-versa). Flow nodes must be indented by at least one more space than the parent block collection. Note that flow nodes may begin on a following line."
Verdict: Strict-conformant
Evidence: `src/event_iter/step.rs:282-304` — byte-prefix dispatch in `step_in_document`: `[`/`{` → `handle_flow_collection`; `|`/`>`/`'`/`"` → `try_consume_scalar` (block scalar or quoted); `-` → sequence; mapping/plain key falls through.
Reasoning: The dispatcher selects flow-in-block when the first non-whitespace character is `[` or `{`; block-in-block otherwise. Both alternatives are realized through the same step-and-dispatch loop. Conformance doc agrees.

### [197] s-l+flow-in-block(n)

BNF: `s-l+flow-in-block(n) ::= s-separate(n+1,FLOW-OUT) ns-flow-node(n+1,FLOW-OUT) s-l-comments`
Spec prose: §8.3: "YAML allows flow nodes to be embedded inside block collections (but not vice-versa). Flow nodes must be indented by at least one more space than the parent block collection."
Verdict: Strict-conformant
Evidence: `src/event_iter/step.rs:297-299` (dispatch); `src/event_iter/flow.rs` (handler — verified to exist via earlier grep showing `fn handle_flow_collection` at line 49). The `s-separate(n+1)` indent constraint is enforced by `close_collections_at_or_above` at the parent dispatcher (`step.rs:892-913`) which closes any collection whose indent ≥ `line_indent+1`, leaving only collections at strictly less indent — so flow nodes inside a block collection stay at parent_col+1 or deeper.
Reasoning: `[`/`{` is dispatched only after the dedent step has identified the correct enclosing collection. The flow handler then runs with that as parent. The trailing `s-l-comments` is handled at scalar/event emission time via `drain_trailing_comment` in the parent dispatcher. Conformance doc agrees.

### [198] s-l+block-in-block(n,c)

BNF: `s-l+block-in-block(n,c) ::= s-l+block-scalar(n,c) | s-l+block-collection(n,c)`
Spec prose: §8.3: "The block node's properties may span across several lines. In this case, they must be indented by at least one more space than the block collection, regardless of the indentation of the block collection entries."
Verdict: Strict-conformant
Evidence: `src/event_iter/base.rs:307-355` (`try_consume_scalar` for `|`/`>`); `src/event_iter/step.rs:287-289` (sequence) and post-match implicit-mapping detection at `step.rs:886-888`.
Reasoning: The dispatcher selects the block-scalar branch (via first byte `|` or `>`) or the block-collection branch (via sequence dash or implicit mapping detection). The two BNF alternatives map to these two paths. Conformance doc agrees.

### [199] s-l+block-scalar(n,c)

BNF: `s-l+block-scalar(n,c) ::= s-separate(n+1,c) ( c-ns-properties(n+1,c) s-separate(n+1,c) )? ( c-l+literal(n) | c-l+folded(n) )`
Spec prose: §8.3: "The block node's properties may span across several lines. In this case, they must be indented by at least one more space than the block collection, regardless of the indentation of the block collection entries."
Verdict: Strict-conformant
Evidence: `src/event_iter/step.rs:457-637` (`!` tag handling) and `src/event_iter/step.rs:640-867` (`&` anchor handling) emit `pending_tag`/`pending_anchor` which are consumed by `try_consume_scalar` at `src/event_iter/base.rs:315-330` (literal) and `src/event_iter/base.rs:340-354` (folded) when the next dispatch encounters `|`/`>`. The standalone-property indent check is at `src/event_iter/base.rs:493-499` (`min_standalone_property_indent`).
Reasoning: Properties are scanned ahead of the block scalar via the `!`/`&` arms, attached as `pending_tag`/`pending_anchor`, and then folded into the `Event::Scalar` meta when the literal/folded handler emits the scalar. Standalone properties (on their own line) require `line_indent >= min` per `step.rs:603-611` and `step.rs:832-841`, where `min = parent_col + 1` for Mapping-Value/Sequence contexts — this is `s-separate(n+1, c)` for the property. The optional `c-ns-properties s-separate` is composed correctly. Conformance doc agrees.

### [200] s-l+block-collection(n,c)

BNF: `s-l+block-collection(n,c) ::= ( s-separate(n+1,c) c-ns-properties(n+1,c) )? s-l-comments ( seq-space(n,c) | l+block-mapping(n) )`
Spec prose: §8.3: "Since people perceive the '-' indicator as indentation, nested block sequences may be indented by one less space to compensate, except, of course, if nested inside another block sequence (BLOCK-OUT context versus BLOCK-IN context)."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/sequence.rs:131-143` (`opens_new` predicate distinguishes BLOCK-IN strict-greater from BLOCK-OUT same-indent — production [201]); property-handling at `src/event_iter/step.rs:551-635, 798-861` propagates pending tag/anchor into the next `MappingStart`/`SequenceStart`.
Reasoning: The composition is: (optional standalone properties on prior lines, indent-checked via `min_standalone_property_indent`) → (the `-` indicator or implicit-mapping line opens the collection). Properties are attached to the resulting `MappingStart`/`SequenceStart` event meta via `pending_collection_anchor`/`pending_collection_tag` displacement bookkeeping (`mapping.rs:531-562`, `sequence.rs:191-214`). The two branches `seq-space(n,c)` and `l+block-mapping(n)` correspond to the sequence-handler and mapping-handler dispatches. Conformance doc agrees.

### [201] seq-space(n,c)

BNF: `seq-space(n,BLOCK-OUT) ::= l+block-sequence(n-1)` / `seq-space(n,BLOCK-IN) ::= l+block-sequence(n)`
Spec prose: §8.3: "Since people perceive the '-' indicator as indentation, nested block sequences may be indented by one less space to compensate, except, of course, if nested inside another block sequence (BLOCK-OUT context versus BLOCK-IN context)."
Verdict: Strict-conformant
Evidence: `src/event_iter/block/sequence.rs:129-143` (`opens_new` predicate).
Reasoning: The two contexts split as: (a) parent is `Sequence(col, _)` — BLOCK-IN — opens only when `dash_indent > col` (strict greater, the n+1 case); (b) parent is `Mapping(col, MappingPhase::Value, _)` — BLOCK-OUT — opens when `dash_indent >= col` (same-indent allowed, the n case where the sequence "compensates" by appearing at the parent key's column). Empty stack and Mapping-in-Key-phase are handled separately. The two-context split exactly realizes the BNF's two cases; conformance doc agrees.
