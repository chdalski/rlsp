---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: B
section: §6
date: 2026-04-30
---

### [63] s-indent(n)

BNF: `s-indent(0) ::= <empty>` / `s-indent(n+1) ::= s-space s-indent(n)`
Spec prose (§6.1): "In YAML block styles, structure is determined by indentation. In general, indentation is defined as a zero or more space characters at the start of a line. To maintain portability, tab characters must not be used in indentation."
Verdict: Strict-conformant
Evidence: `lines.rs:142` (`indent = content.chars().take_while(|&ch| ch == ' ').count()`); call sites consume this value in `lines.rs:367` and throughout block-collection parsers in `event_iter/block/`.
Reasoning: The BNF reduces to "n leading SPACE characters" with tab explicitly forbidden. The single-line predicate at `lines.rs:142` literally counts a leading run of `' '` characters and stops at the first non-space, so a leading tab yields indent = 0. The unit tests `indent_counts_only_leading_spaces`, `leading_tab_does_not_count_toward_indent`, and `tab_after_spaces_does_not_count` cover the spec rule directly. The cited line in the conformance doc matches the actual implementation.

### [64] s-indent-less-than(n)

BNF: `s-indent-less-than(1) ::= <empty>` / `s-indent-less-than(n+1) ::= s-space s-indent-less-than(n) | <empty>`
Spec prose (§6.1): "A block style construct is terminated when encountering a line which is less indented than the construct."
Verdict: Strict-conformant
Evidence: `lines.rs:367` (`if base_indent != usize::MAX && line.indent <= base_indent { break }` inside `peek_until_dedent`); same predicate used by literal-block scalar termination via `lines.rs:331-374`.
Reasoning: `peek_until_dedent(base_indent)` halts the lookahead at the first non-blank line whose indent is `<= base_indent`. For the production "less than n", callers pass `base_indent = n - 1`, giving `indent <= n-1`, equivalent to `indent < n`. The companion productions [70] `l-empty(n,c)` and the block-collection parsers all consume the lookahead's pre-filtered output, so the indent comparison itself is enforced. The conformance doc's citation `lines.rs:340` points at the loop start; the actual decision line is 367.

### [65] s-indent-less-or-equal(n)

BNF: `s-indent-less-or-equal(0) ::= <empty>` / `s-indent-less-or-equal(n+1) ::= s-space s-indent-less-or-equal(n) | <empty>`
Spec prose (§6.1): "The productions use the notation `s-indent-less-than(n)` and `s-indent-less-or-equal(n)` to express this."
Verdict: Strict-conformant
Evidence: `lexer/block.rs:181-200` (`if next.indent >= content_indent { … }` plus `is_content_line` requiring `>= content_indent`), with the complement (`< content_indent`) treated as the dedent boundary; same `<= base_indent` shape in `lines.rs:367`.
Reasoning: The implementation uses `indent < n` (literal block scalar) and `indent <= n` (lookahead) as opposite forms of the same family of indent comparisons. Both are integer comparisons against `n`, with no off-by-one drift visible at the cited lines. The conformance doc characterizes the predicate accurately.

### [66] s-separate-in-line

BNF: `s-separate-in-line ::= s-white+ | <start-of-line>`
Spec prose (§6.2): "Outside indentation and scalar content, YAML uses white space characters for separation between tokens within a line. Note that such white space may safely include tab characters."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:107` and `:276` (`trim_start_matches([' ', '\t'])` on flow-scalar continuation lines); `event_iter/directives.rs:88-93` (`find([' ', '\t'])` to split directive name from rest); `lexer/comment.rs:30` (`trim_start_matches([' ', '\t'])` for comment indent); `lexer.rs:181-184` (`trim_start_matches([' ', '\t'])` for comment-line predicate).
Reasoning: `s-separate-in-line` is satisfied by any non-empty run of `s-white` (space or tab) or by being at start-of-line. The implementation consistently uses the character class `[' ', '\t']` for inter-token separation everywhere it matters: directive name/parameter splitting, comment-indent stripping, and flow-scalar continuation prefix stripping. There is no place where a tab-only separator is rejected. The `<start-of-line>` alternative is satisfied implicitly because `LineBuffer` delivers content from the start of each line.

### [67] s-line-prefix(n,c)

BNF: `s-line-prefix(n,BLOCK-OUT) ::= s-block-line-prefix(n)` / `s-line-prefix(n,BLOCK-IN) ::= s-block-line-prefix(n)` / `s-line-prefix(n,FLOW-OUT) ::= s-flow-line-prefix(n)` / `s-line-prefix(n,FLOW-IN) ::= s-flow-line-prefix(n)`
Spec prose (§6.3): "Inside scalar content, each line begins with a non-content line prefix. This prefix always includes the indentation. For flow scalar styles it additionally includes all leading white space, which may contain tab characters."
Verdict: Strict-conformant
Evidence: Block context: `lexer/block.rs:181-200` (continuation lines validated against `content_indent` via `>= content_indent` check). Flow context: `lexer/quoted.rs:107` (single-quoted) and `:276` (double-quoted) both apply `trim_start_matches([' ', '\t'])`.
Reasoning: This production is a context-dependent dispatcher. The implementation correctly differentiates by call site: block-context scanners track `content_indent` and require `indent >= content_indent` for the line to count as content; flow-context scanners strip both spaces and tabs as a unit. Both behaviours match the spec's two-mode rule. There is no shared abstraction; each scanner enforces its own form of the prefix. Verdict is at this composition point because the sub-productions [68] and [69] each conform.

### [68] s-block-line-prefix(n)

BNF: `s-block-line-prefix(n) ::= s-indent(n)`
Spec prose (§6.3): "Inside scalar content, each line begins with a non-content line prefix. This prefix always includes the indentation."
Verdict: Strict-conformant
Evidence: `lexer/block.rs:181-200` — `next.indent >= content_indent` gate selects content lines; on a content line, `line_content.get(content_indent..)` strips exactly `content_indent` bytes (which are spaces by construction of [63]); the residual is the body. `lexer/block.rs:134-145` rejects a leading tab as invalid block indentation.
Reasoning: For a block scalar with header indent n, the implementation strips the n leading spaces to recover the body. Because `Line.indent` counts only spaces (per [63]), a line with `indent >= content_indent` is guaranteed to have at least `content_indent` leading spaces, so the byte-offset slice at line 182 is correct. The leading-tab rejection at line 134 prevents the spec violation "tab character is not valid indentation in a block scalar."

### [69] s-flow-line-prefix(n)

BNF: `s-flow-line-prefix(n) ::= s-indent(n) s-separate-in-line?`
Spec prose (§6.3): "For flow scalar styles it additionally includes all leading white space, which may contain tab characters."
Verdict: Lenient
Evidence: `lexer/quoted.rs:107` (`trim_start_matches([' ', '\t'])`) and `:276` (same) strip ALL leading whitespace without verifying that the first n bytes are spaces. `lexer/quoted.rs:282-291` does enforce `next.indent <= n` rejection for block-context double-quoted continuations, but only when a `block_context_indent` is supplied; single-quoted (`:107`) has no equivalent indent-floor check.
Reasoning: The spec defines `s-flow-line-prefix(n)` as `s-indent(n)` followed by optional `s-separate-in-line`. The implementation's single-quoted continuation scanner strips both spaces and tabs from the line start without validating that the first n characters are spaces. A continuation line that begins with a tab (e.g., `\t\tcontent`) is accepted as if the tab counted toward indentation, which contradicts the n-space requirement that flows from [63]. The double-quoted scanner does enforce `indent <= n` rejection when `block_context_indent` is supplied (`:283`), so it rejects under-indented continuations, but it still strips leading tabs as part of the prefix without distinguishing between the indent portion and the optional separation portion. The conformance doc claims "Conformant" with the same citations; the trim-based prefix stripping does not enforce the spec's two-part structure.

### [70] l-empty(n,c)

BNF: `l-empty(n,c) ::= ( s-line-prefix(n,c) | s-indent-less-than(n) ) b-as-line-feed`
Spec prose (§6.4): "An empty line line consists of the non-content prefix followed by a line break. The semantics of empty lines depend on the scalar style they appear in."
Verdict: Strict-conformant
Evidence: `lexer.rs:104-118` (`skip_empty_lines` consumes lines where `is_blank_not_comment` returns true); `lexer.rs:521-523` (`is_blank_not_comment` is `trim_start_matches([' ', '\t']).is_empty()`); `lexer/quoted.rs:109-112` (single-quoted blank continuation pushes a literal `'\n'`); `lexer/quoted.rs:293-302` (double-quoted blank continuation increments `pending_blanks`); `lexer/block.rs:245-260` (literal-block blank-line counting via `trailing_newlines`).
Reasoning: An l-empty line is "anything-prefix followed by line-break"; semantically blank for the parser. All four scanners agree: a line whose `trim_start_matches([' ', '\t'])` is empty contributes a newline to the scalar value (or none, at top level) without further structural effect. The dual definition of the prefix (s-line-prefix(n,c) | s-indent-less-than(n)) is satisfied by accepting any leading-whitespace-only content, which is exactly what `is_blank_not_comment` does. No scanner rejects an l-empty line for being under- or over-indented before classifying it as blank.

### [71] b-l-trimmed(n,c)

BNF: `b-l-trimmed(n,c) ::= b-non-content l-empty(n,c)+`
Spec prose (§6.5): "If a line break is followed by an empty line, it is trimmed; the first line break is discarded and the rest are retained as content."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:295-316` — `pending_blanks` counter increments on each blank continuation line (`:295`); when a non-blank line follows with `pending_blanks > 0`, the implementation appends exactly `pending_blanks` newlines (`:311`) instead of one fold space; `lexer/quoted.rs:109-112` for single-quoted: blank continuation pushes a literal `'\n'`, and the surrounding logic does not double-add a fold space when `owned.ends_with('\n')` (`:117`).
Reasoning: The spec rule is "the first line break is discarded; the rest are retained." The implementation accumulates N blank-line separators and emits N newlines into the output, which is the same result as "discard the first newline (the one ending the trigger non-empty line) and retain N-1 newlines from the N-1 blank lines + 1 final newline = N newlines." The mathematics are equivalent, and the test coverage cited in the conformance doc exercises both modes.

### [72] b-as-space

BNF: `b-as-space ::= b-break`
Spec prose (§6.5): "Otherwise (the following line is not empty), the line break is converted to a single space (x20)."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:308-315` — when `pending_blanks == 0` and `line_continuation` is false, `owned.push(' ')` (line 314) inserts the fold space; `lexer/quoted.rs:117-119` (single-quoted: `if !owned.ends_with('\n') { owned.push(' ') }`).
Reasoning: A non-blank fold without preceding empty lines becomes a single space (U+0020), exactly as the spec mandates. The check `!owned.ends_with('\n')` correctly distinguishes the b-as-space case from the b-l-trimmed case (which would have left a newline in the output).

### [73] b-l-folded(n,c)

BNF: `b-l-folded(n,c) ::= b-l-trimmed(n,c) | b-as-space`
Spec prose (§6.5): "A folded non-empty line may end with either of the above line breaks."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:308-316` — the `pending_blanks` counter selects between trimmed (`pending_blanks > 0` → N newlines) and as-space (`pending_blanks == 0` → single space) on each fold boundary. Single-quoted: `lexer/quoted.rs:115-119` uses `owned.ends_with('\n')` for the same dispatch.
Reasoning: This is the union of [71] and [72]; both branches are correctly implemented (verdict per [71] and [72]). The dispatch logic at the cited lines correctly selects between the two modes based on whether blank continuation lines have intervened.

### [74] s-flow-folded(n)

BNF: `s-flow-folded(n) ::= s-separate-in-line? b-l-folded(n,FLOW-IN) s-flow-line-prefix(n)`
Spec prose (§6.5): "Once all such spaces have been discarded, all line breaks are folded without exception."
Verdict: Lenient
Evidence: `lexer/quoted.rs:75-78` (single-quoted: `truncate(owned.trim_end_matches([' ', '\t']).len())` strips trailing whitespace from prior line); `lexer/quoted.rs:107` and `:276` (leading whitespace stripped from each continuation line); the fold itself: `:117-119` and `:308-315`.
Reasoning: The trailing-whitespace and leading-whitespace strips correctly implement the "discard preceding/following spaces, then fold all breaks" rule of §6.5. However, this production composes [69] `s-flow-line-prefix(n)`, which I verdicted Lenient because the implementation does not enforce that the first n leading characters are spaces (tabs are accepted in the indent portion). That leniency propagates here: a continuation line with tab-indentation is folded as if it had n-space indentation. The spec sentence about fold semantics is honored, but the prefix structure is not.

### [75] c-nb-comment-text

BNF: `c-nb-comment-text ::= c-comment nb-char*`
Spec prose (§6.6): "An explicit comment is marked by a `#` indicator. Comments must be separated from other tokens by white space characters."
Verdict: Lenient
Evidence: `lexer/comment.rs:30-33` (comment lexer triggers on `#` after optional leading whitespace); `lexer/comment.rs:50-51` (`text: &'input str = &line.content[text_start..]` — the comment body is everything after the `#` up to the line break, regardless of character content).
Reasoning: The body of a comment is `nb-char*`, where `nb-char` excludes line breaks but includes printable Unicode (including DEL `\x7F` per [27] in the same project's audit findings). The implementation slices everything after the `#` up to the line break without filtering. Because the slice is bounded by `LineBuffer`'s line splitter, line breaks are excluded by construction, satisfying the `nb-char` constraint to that extent. However, `nb-char` (production [27]) excludes the BOM (`\u{FEFF}`); the implementation does not strip BOM occurrences from comment bodies. A BOM character in a comment body would be retained verbatim. This is the same lenience identified for [27] in the §5 audit. The conformance doc claims "Conformant" with no mention of this.

### [76] b-comment

BNF: `b-comment ::= b-non-content | <end-of-input>`
Spec prose (§6.6): "Note: To ensure JSON compatibility, YAML processors must allow for the omission of the final comment line break of the input stream."
Verdict: Strict-conformant
Evidence: `lexer/comment.rs:50-51` (the comment text slice excludes the line terminator); `lines.rs:139` (`detect_break` advances past the terminator when present); the line buffer treats EOF as a valid line terminator via `BreakType::Eof`.
Reasoning: A comment terminates at b-non-content or end-of-input. The lexer reads up to `line_end` (the position of the first `\n`/`\r` or end-of-content) and consumes the line via `consume_next`; whether the terminator is `\n`, `\r\n`, `\r`, or end-of-input, the comment is correctly closed. The JSON-compatibility note about omitting the final line break is honoured because `BreakType::Eof` is treated as a valid line end.

### [77] s-b-comment

BNF: `s-b-comment ::= ( s-separate-in-line c-nb-comment-text? )? b-comment`
Spec prose (§6.6): "Comments must be separated from other tokens by white space characters."
Verdict: Lenient
Evidence: `lexer.rs:354-381` (`handle_plain_scalar_inline`: `residual.is_empty() || residual.starts_with('#')` — accepts a `#` immediately after a plain scalar with intervening whitespace stripped); `event_iter/directives.rs:126-133` (after parsing version, residual is checked: empty or `#`-prefixed → ok); `lexer/comment.rs:30-31` (comment line trigger does not require any preceding whitespace context).
Reasoning: The s-b-comment production requires that a comment be preceded by `s-separate-in-line` when present (and a separation IS required by the broader rule "Comments must be separated from other tokens"). The implementation strips whitespace via `trim_start_matches([' ', '\t'])` before checking for `#`, but does not verify that AT LEAST ONE whitespace character was present between the prior token and the `#`. For an inline `--- key#comment` (no space before `#`), the residual after `key` is `#comment`, which starts with `#` and is treated as a valid trailing comment. The spec requires whitespace separation between content and `#`. The conformance doc claims "Conformant" but the citation does not address the "must be separated by white space" requirement; the residual check accepts zero-separator cases.

### [78] l-comment

BNF: `l-comment ::= s-separate-in-line c-nb-comment-text? b-comment`
Spec prose (§6.6): "Outside scalar content, comments may appear on a line of their own, independent of the indentation level. Note that outside scalar content, a line containing only white space characters is taken to be a comment line."
Verdict: Strict-conformant
Evidence: `lexer/comment.rs:30-31` (`trim_start_matches([' ', '\t'])` followed by `starts_with('#')`); `lexer.rs:521-523` (`is_blank_not_comment` distinguishes blank from comment); `lexer.rs:179-184` (`is_comment_line` accepts any indentation before `#`).
Reasoning: A standalone comment line is recognized at any indentation level, satisfying "independent of the indentation level." The whitespace-only line is correctly classified as blank (not comment) by `is_blank_not_comment` at `lexer.rs:521-523`, and `skip_empty_lines` at `lexer.rs:104-118` consumes it without emitting a Comment event. The s-separate-in-line at the start of l-comment is satisfied by start-of-line plus the optional leading whitespace.

### [79] s-l-comments

BNF: `s-l-comments ::= ( s-b-comment | <start-of-line> ) l-comment*`
Spec prose (§6.6): "In most cases, when a line may end with a comment, YAML allows it to be followed by additional comment lines. The only exception is a comment ending a block scalar header."
Verdict: Strict-conformant
Evidence: `event_iter/directives.rs:33-64` (`consume_preamble_between_docs` loops over blank/comment/directive lines); `event_iter/directives.rs:237-256` (`skip_and_collect_comments_in_doc` does the in-document equivalent); `lexer/block.rs:69-75` reads block-scalar header on a single line and does not loop into trailing comment-only lines.
Reasoning: The implementation treats sequences of comment lines as a single comment cluster in both BetweenDocs and InDocument contexts via two near-identical loops. The block-scalar header parses inline content only, so comments after the header line are not absorbed into the header — matching the "exception is a comment ending a block scalar header" rule.

### [80] s-separate(n,c)

BNF: `s-separate(n,BLOCK-OUT) ::= s-separate-lines(n)` / `s-separate(n,BLOCK-IN) ::= s-separate-lines(n)` / `s-separate(n,FLOW-OUT) ::= s-separate-lines(n)` / `s-separate(n,FLOW-IN) ::= s-separate-lines(n)` / `s-separate(n,BLOCK-KEY) ::= s-separate-in-line` / `s-separate(n,FLOW-KEY) ::= s-separate-in-line`
Spec prose (§6.7): "Implicit keys are restricted to a single line. In all other cases, YAML allows tokens to be separated by multi-line (possibly empty) comments."
Verdict: Strict-conformant
Evidence: `event_iter/directives.rs:237-256` (multi-line comment+blank separation in document body); `event_iter/flow.rs:160-168` (flow-collection prelude trims `[' ', '\t']` for in-line key separation); the implicit-key 1024-character restriction at `event_iter/flow.rs:1152-1160` enforces "implicit keys are restricted to a single line" (but at character count rather than physical line count).
Reasoning: The dispatch by context is implicit in the implementation: block/flow node parsers call into `skip_and_collect_comments_in_doc` (multi-line) when between non-key tokens, and call inline-only trim functions (single-line) when scanning an implicit key. The character-count check at `flow.rs:1152` is the project's chosen surrogate for the "single line" rule from §7.4.3, which matches the YAML spec's normative phrasing for implicit flow keys. Sub-productions [81] and `s-separate-in-line` ([66]) each conform, so the parent composition conforms.

### [81] s-separate-lines(n)

BNF: `s-separate-lines(n) ::= ( s-l-comments s-flow-line-prefix(n) ) | s-separate-in-line`
Spec prose (§6.7): "Note that structures following multi-line comment separation must be properly indented, even though there is no such restriction on the separation comment lines themselves."
Verdict: Strict-conformant
Evidence: `event_iter/directives.rs:33-64` (multi-line comment-then-content path); `lexer.rs:156-180` (single-line whitespace path); the indentation of the structure following the comments is enforced at the call site (block sequence/mapping parsers verify indent against parent).
Reasoning: Two alternatives are implemented — comment-line cluster followed by a properly-indented next structure (verified by the consuming block parser), and inline whitespace separation. The comment cluster does not have its own indentation restriction (per spec). Sub-production conformance: [79] s-l-comments is Strict-conformant, [69] s-flow-line-prefix is Lenient — but the leniency in [69] is about which characters count as the prefix on flow continuation lines, not about block-context indent enforcement here. In s-separate-lines, the prefix-matching role is enforced by the next-structure consumer.

### [82] l-directive

BNF: `l-directive ::= c-directive ( ns-yaml-directive | ns-tag-directive | ns-reserved-directive ) s-l-comments`
Spec prose (§6.8): "Directives are instructions to the YAML processor. This specification defines two directives, `YAML` and `TAG`, and reserves all other directives for future use."
Verdict: Strict-conformant
Evidence: `event_iter/directives.rs:70-104` (`parse_directive` dispatches on name); `lexer.rs:148-154` (`is_directive_line` checks `starts_with('%')`); `lexer.rs:156-174` (`try_consume_directive_line` consumes the entire line, leaving `s-l-comments` to be consumed by the surrounding `consume_preamble_between_docs` loop).
Reasoning: The parser correctly recognises the `%` prefix, splits the line, dispatches to `parse_yaml_directive`, `parse_tag_directive`, or the reserved-directive branch, and the surrounding loop consumes any trailing comment lines. The composition matches the production exactly. Note that the implementation forbids directives in InDocument state (a `%`-prefixed line in document body is regular content, not a directive), which matches the spec rule that directives appear only in the document prefix.

### [83] ns-reserved-directive

BNF: `ns-reserved-directive ::= ns-directive-name ( s-separate-in-line ns-directive-parameter )*`
Spec prose (§6.8): "A YAML processor should ignore unknown directives with an appropriate warning."
Verdict: Strict-conformant
Evidence: `event_iter/directives.rs:97-103` (unknown directive names increment `directive_count` and return `Ok(())` without emitting a warning).
Reasoning: The spec uses "should ignore … with an appropriate warning" — "should" allows the warning to be omitted. The implementation silently ignores unknown directives, which is permitted. The directive name and parameter list are not parsed beyond the name extraction (the body is discarded), which is acceptable because the production specifies a structure but not any normative content for reserved directives.

### [84] ns-directive-name

BNF: `ns-directive-name ::= ns-char+`
Spec prose (§6.8): "Each directive is specified on a separate non-indented line starting with the `%` indicator, followed by the directive name."
Verdict: Lenient
Evidence: `event_iter/directives.rs:88-92` (`name_end = after_percent.find([' ', '\t']).unwrap_or(after_percent.len())`; `name = &after_percent[..name_end]`).
Reasoning: The implementation treats the directive name as everything after `%` up to the first whitespace, without validating that those bytes are `ns-char+`. The spec restricts the name to one or more `ns-char` characters, which excludes line breaks, BOM, and (per [34]) any non-printable Unicode. A directive line `%a\u{0001}b 1.2` would be parsed with name `a\u{0001}b` and silently treated as a reserved directive (since it does not match `"YAML"` or `"TAG"`). The implementation accepts non-`ns-char` bytes in the name. The conformance doc claims "Conformant" but the cited code does not enforce the `ns-char+` constraint.

### [85] ns-directive-parameter

BNF: `ns-directive-parameter ::= ns-char+`
Spec prose (§6.8): "Each directive is specified on a separate non-indented line starting with the `%` indicator, followed by the directive name and a list of parameters."
Verdict: Lenient
Evidence: `event_iter/directives.rs:93` (`rest = after_percent[name_end..].trim_start_matches([' ', '\t'])` — `rest` is the parameter blob without per-parameter character validation); for the `YAML` directive, `parse_yaml_directive` validates only the digits and `.`; for `TAG`, `parse_tag_directive` rejects only control characters in the prefix (`directives.rs:207-215`).
Reasoning: A parameter is `ns-char+`, but the only enforcement of `ns-char` content occurs for tag prefixes (and only against ASCII control characters and DEL, not against the full `ns-char` exclusion set including BOM). For the `YAML` directive, the version digits are bounded by `parse::<u8>()` which rejects non-ASCII digits, but the surrounding split allows trailing whitespace/comment. For reserved directives, parameters are not validated at all. This is Lenient against the spec's `ns-char+` constraint. The conformance doc claims "Conformant" without addressing the constraint.

### [86] ns-yaml-directive

BNF: `ns-yaml-directive ::= "YAML" s-separate-in-line ns-yaml-version`
Spec prose (§6.8.1): "A version 1.2 YAML processor must accept documents with an explicit `%YAML 1.2` directive, as well as documents lacking a `YAML` directive."
Verdict: Stricter-than-spec
Evidence: `event_iter/directives.rs:107-156` (`parse_yaml_directive`): line 108 rejects a duplicate directive; lines 116-119 require a `.` separator; lines 136-143 require both major and minor to parse as `u8`; lines 146-151 reject `major != 1`. The spec requires acceptance of `1.2` and (per the same section) `1.1` and any `1.x`; higher major versions warrant rejection.
Reasoning: The strictness rationale: rejecting `major == 0` is not specified by the spec (which only says "higher major versions … require rejection"). A `%YAML 0.5` directive is rejected as "unsupported YAML version" by the implementation. This is stricter than the spec, which is silent on major 0. The spec is unambiguous about major ≥ 2 (must reject), so the cited code is correct for that case. The duplicate-directive rejection is not in the spec but is consistent with the spec's general intent that a single document have at most one `%YAML` directive (multiple directives would create ambiguity). The version-1.0/0.x rejection is the strictness that exceeds spec; the rejection is reasonable conservatism.

### [87] ns-yaml-version

BNF: `ns-yaml-version ::= ns-dec-digit+ '.' ns-dec-digit+`
Spec prose (§6.8.1): "A version 1.2 YAML processor must also accept documents with an explicit `%YAML 1.1` directive."
Verdict: Strict-conformant
Evidence: `event_iter/directives.rs:116-143` (`find('.')` splits on dot; `parse::<u8>()` validates each side as decimal digits and limits to 0-255).
Reasoning: The version is required to be `digit+ '.' digit+`. The split-on-dot plus `u8::from_str` correctly enforces "one or more decimal digits before and after a single dot," with the additional constraint that each side fits in u8 (max 255). The spec does not bound the digit count, but practically no version exceeds 255.999. The implementation rejects "1.2.3" because the trailing content after the second number must be empty or a `#` comment (line 127). Composition with [35] `ns-dec-digit` is via `u8::from_str` which only accepts ASCII digits.

### [88] ns-tag-directive

BNF: `ns-tag-directive ::= "TAG" s-separate-in-line c-tag-handle s-separate-in-line ns-tag-prefix`
Spec prose (§6.8.2): "The `TAG` directive establishes a tag shorthand notation for specifying node tags. Each `TAG` directive associates a handle with a prefix."
Verdict: Strict-conformant
Evidence: `event_iter/directives.rs:159-229` (`parse_tag_directive`): line 165 splits handle and prefix on whitespace; line 182 validates handle via `is_valid_tag_handle`; line 218 rejects duplicate handles; line 225-227 stores in scope.
Reasoning: The composition handle-then-prefix-with-mandatory-separation is enforced. The handle parses via `find([' ', '\t'])` (line 165), which ensures at least one whitespace between handle and prefix. The handle validity check at line 182 enforces [89]–[92]. The prefix validity is checked weakly (control characters only — see [93]–[95]), but that leniency belongs to the sub-productions, not [88]. Verdict at this composition is Strict-conformant given the sub-production verdicts assigned at their own entries.

### [89] c-tag-handle

BNF: `c-tag-handle ::= c-named-tag-handle | c-secondary-tag-handle | c-primary-tag-handle`
Spec prose (§6.8.2.1): "The tag handle exactly matches the prefix of the affected tag shorthand. There are three tag handle variants."
Verdict: Strict-conformant
Evidence: `event_iter/properties.rs:281-295` (`is_valid_tag_handle` matches `"!"`, `"!!"`, or `!<word>!`).
Reasoning: The three variants are dispatched correctly: `"!"` → primary, `"!!"` → secondary, otherwise it must start with `!` and end with `!` with non-empty word characters in between. The composition enumerates all three variants exclusively. Sub-productions [90], [91], [92] are individually correct.

### [90] c-primary-tag-handle

BNF: `c-primary-tag-handle ::= '!'`
Spec prose (§6.8.2.1): "The primary tag handle is a single `!` character."
Verdict: Strict-conformant
Evidence: `event_iter/properties.rs:283` (`"!" => true` arm of `is_valid_tag_handle`).
Reasoning: A literal single-character match. No edge cases.

### [91] c-secondary-tag-handle

BNF: `c-secondary-tag-handle ::= "!!"`
Spec prose (§6.8.2.1): "The secondary tag handle is written as `!!`."
Verdict: Strict-conformant
Evidence: `event_iter/properties.rs:283` (`"!!" => true` arm); `event_iter/directive_scope.rs:93-108` (resolution: lookup of `!!` handle, defaulting to `tag:yaml.org,2002:`).
Reasoning: A literal two-character match. The default prefix `tag:yaml.org,2002:` is exactly the spec's default at §6.8.2.2.

### [92] c-named-tag-handle

BNF: `c-named-tag-handle ::= c-tag ns-word-char+ c-tag`
Spec prose (§6.8.2.1): "A named tag handle surrounds a non-empty name with `!` characters."
Verdict: Strict-conformant
Evidence: `event_iter/properties.rs:286-294` (strips leading and trailing `!`, requires inner non-empty, validates each inner char with `is_ascii_alphanumeric() || c == '-'`); unit tests at `event_iter/properties.rs:482-499` cover hyphen acceptance and underscore rejection.
Reasoning: Per [38] in §5, `ns-word-char ::= ns-dec-digit | ns-ascii-letter | '-'`. The implementation's allowance set `[a-zA-Z0-9-]` matches that exactly: alphanumeric is digits + letters; `-` is included; `_` is correctly excluded. The leading/trailing `!` is stripped by `strip_prefix('!').and_then(|s| s.strip_suffix('!'))`. The non-empty inner is enforced by the `if !word.is_empty()` guard.

### [93] ns-tag-prefix

BNF: `ns-tag-prefix ::= c-ns-local-tag-prefix | ns-global-tag-prefix`
Spec prose (§6.8.2.2): "There are two tag prefix variants."
Verdict: Lenient
Evidence: `event_iter/directives.rs:170` (`prefix = params[handle_end..].trim_start_matches([' ', '\t'])`); `event_iter/directives.rs:200-205` (length cap); `event_iter/directives.rs:207-215` (rejects only control characters and DEL); no verification that `prefix.starts_with('!')` matches the local-prefix branch's grammar nor that global prefixes contain only `ns-uri-char`.
Reasoning: The spec splits `ns-tag-prefix` into two alternatives: local (starts with `!`, body is `ns-uri-char*`) or global (`ns-tag-char` then `ns-uri-char*`). The implementation accepts whatever sequence of non-control characters appears in the prefix without dispatching on the leading `!` versus tag-char first character, and without restricting the body to `ns-uri-char`. A prefix like `tag:^evil` (containing the disallowed `^` character) would be accepted. The conformance doc itself notes this in its line 968 ("not strictly checked against local vs global tag prefix grammar (both forms accepted)") yet still classifies as "Conformant" — the doc's classification contradicts its own implementation note.

### [94] c-ns-local-tag-prefix

BNF: `c-ns-local-tag-prefix ::= c-tag ns-uri-char*`
Spec prose (§6.8.2.2): "If the prefix begins with a `!` character, shorthands using the handle are expanded to a local tag."
Verdict: Lenient
Evidence: `event_iter/directives.rs:172-215` (prefix stored as-is after the control-char check); `event_iter/directive_scope.rs:134-151` (`!suffix` resolution concatenates the stored prefix with the percent-decoded suffix without re-validating the prefix against `ns-uri-char`).
Reasoning: Same leniency as [93]. The implementation stores any non-control-char prefix beginning with `!` as a local tag prefix, without enforcing that the body is `ns-uri-char*` (which excludes `<`, `>`, `[`, `]`, etc.). At resolution time, the stored prefix is concatenated verbatim. The conformance doc claims "Conformant"; the cited code does no `ns-uri-char` validation.

### [95] ns-global-tag-prefix

BNF: `ns-global-tag-prefix ::= ns-tag-char ns-uri-char*`
Spec prose (§6.8.2.2): "If the prefix begins with a character other than `!`, it must be a valid URI prefix, and should contain at least the scheme."
Verdict: Lenient
Evidence: `event_iter/directives.rs:207-215` (only control characters rejected); `event_iter/directive_scope.rs:92-132` (resolution concatenates the stored URI prefix with percent-decoded suffix).
Reasoning: A global tag prefix requires `ns-tag-char` as the first character (which excludes flow indicators `[`, `]`, `{`, `}`, `,` and `!`), and the rest must be `ns-uri-char` (which excludes `<`, `>`, `^`, etc.). The implementation's only validation is that no ASCII control characters or DEL are present. A prefix like `[example` or `tag^evil` is accepted. The "should contain at least the scheme" clause is non-normative ("should") and may be omitted, but the character-class constraints ARE normative. The conformance doc claims "Conformant".

### [96] c-ns-properties(n,c)

BNF: `c-ns-properties(n,c) ::= ( c-ns-tag-property ( s-separate(n,c) c-ns-anchor-property )? ) | ( c-ns-anchor-property ( s-separate(n,c) c-ns-tag-property )? )`
Spec prose (§6.9): "Each node may have two optional properties, anchor and tag, in addition to its content. Node properties may be specified in any order before the node's content. Either or both may be omitted."
Verdict: Strict-conformant
Evidence: `event_iter/state.rs` and `event_iter/properties.rs` define `pending_tag` and `pending_anchor` slots; `event_iter/base.rs` and `event_iter/block.rs`/`event_iter/flow.rs` accept `&` and `!` indicators in either order before a node and emit them as part of the next collection-start or scalar event.
Reasoning: The "any order" requirement is satisfied by independent `pending_tag` / `pending_anchor` accumulator slots. The "either or both may be omitted" is satisfied because the slots default to None. The composition correctly enforces that a tag and an anchor can each be present at most once before a node by using single-slot accumulators (a second indicator of the same kind on the same node is an error, which is enforced when properties are read again). The s-separate(n,c) between the two properties is the same separation enforced by the surrounding tokenizer.

### [97] c-ns-tag-property

BNF: `c-ns-tag-property ::= c-verbatim-tag | c-ns-shorthand-tag | c-non-specific-tag`
Spec prose (§6.9.1): "The tag property identifies the type of the native data structure presented by the node. A tag is denoted by the `!` indicator."
Verdict: Strict-conformant
Evidence: `event_iter/properties.rs:85-233` (`scan_tag` dispatches: line 91 verbatim if content starts with `<`; line 167 primary/secondary if content starts with `!`; line 186 non-specific if no tag chars; line 192-216 named/secondary shorthand otherwise).
Reasoning: The four-way dispatch on the character following `!` enumerates the verbatim, shorthand (primary/secondary/named), and non-specific cases. Sub-production conformance is assessed at [98], [99], [100].

### [98] c-verbatim-tag

BNF: `c-verbatim-tag ::= "!<" ns-uri-char+ '>'`
Spec prose (§6.9.1): "A tag may be written verbatim by surrounding it with the `<` and `>` characters."
Verdict: Strict-conformant
Evidence: `event_iter/properties.rs:91-164` — the loop at lines 100-147 validates each byte against `is_ns_uri_char_single` or accepts a `%HH` percent-encoded sequence; line 110 detects the closing `>`; line 149 rejects empty URI.
Reasoning: The body is scanned byte-by-byte; non-ASCII leading bytes fail `is_ns_uri_char_single`; `%HH` sequences are validated for two ASCII hex digits; an unmatched `<` results in an error. The empty-URI rejection at line 149 enforces the `ns-uri-char+` (one or more) requirement. The unit tests `scan_tag_verbatim_*` at `properties.rs:592-769` cover the cases.

### [99] c-ns-shorthand-tag

BNF: `c-ns-shorthand-tag ::= c-tag-handle ns-tag-char+`
Spec prose (§6.9.1): "A tag shorthand consists of a valid tag handle followed by a non-empty suffix."
Verdict: Lenient
Evidence: `event_iter/properties.rs:167-181` (primary `!!suffix` allows empty suffix); `event_iter/properties.rs:192-216` (named handle / secondary shorthand: scanning loop accepts `!handle!` with an empty suffix when followed by a non-tag-char or end of content).
Reasoning: The spec requires a NON-EMPTY suffix (`ns-tag-char+`, with `+` meaning one or more). The implementation's primary-handle branch at line 170-176 explicitly comments "`!!` alone with no suffix is valid (empty suffix shorthand)." This contradicts `ns-tag-char+`. The named-handle branch at line 200-203 sets `end = i + 1; end += scan_tag_suffix(&content[i + 1..])` and on a zero-byte suffix returns the bare `!handle!` form. The spec considers `!!` and `!handle!` (without suffix) malformed shorthand tags. The conformance doc claims "Conformant".

### [100] c-non-specific-tag

BNF: `c-non-specific-tag ::= '!'`
Spec prose (§6.9.1): "If a node has no tag property, it is assigned a non-specific tag that needs to be resolved to a specific one."
Verdict: Strict-conformant
Evidence: `event_iter/properties.rs:184-189` (when `scan_tag_suffix` returns 0 and content does not start with `<` or `!`, returns one-byte slice `&tag_start[..1]`).
Reasoning: A literal single-character match for the `!` indicator alone, no suffix.

### [101] c-ns-anchor-property

BNF: `c-ns-anchor-property ::= c-anchor ns-anchor-name`
Spec prose (§6.9.2): "An anchor is denoted by the `&` indicator. Anchor names must not contain the `[`, `]`, `{`, `}` and `,` characters."
Verdict: Strict-conformant
Evidence: `event_iter/properties.rs:23-45` (`scan_anchor_name` called with content immediately after the `&` indicator); `chars.rs:149-159` (`is_ns_anchor_char` excludes flow indicators and whitespace).
Reasoning: The composition `c-anchor` + `ns-anchor-name` is enforced at the call site (caller has consumed the `&`) and the body is scanned via `is_ns_anchor_char`, which is the indicator-excluding version of `ns-char`. Empty anchor names are rejected at line 32-37.

### [102] ns-anchor-char

BNF: `ns-anchor-char ::= ns-char - c-flow-indicator`
Spec prose (§6.9.2): "Anchor names must not contain the `[`, `]`, `{`, `}` and `,` characters."
Verdict: Strict-conformant
Evidence: `chars.rs:149-159` — `is_ns_anchor_char` evaluates the conjunction `!is_whitespace && !is_c_flow_indicator && in ns-char range`; the `ns-char` range covers `\x21..=\x7E | \u{85} | \u{A0}..=\u{D7FF} | \u{E000}..=\u{FFFD} | \u{10000}..=\u{10FFFF}`; the explicit whitespace exclusion at line 150 covers `' '`, `'\t'`, `'\n'`, `'\r'`, BOM.
Reasoning: The set computation matches the spec: `ns-char` (printable Unicode minus whitespace) minus the five flow indicators. Note that DEL `\x7F` is NOT in `ns-char` (the upper bound is `\x7E`), correctly excluding it. This is the strict definition. Unit tests at `chars.rs:322-348` cover both acceptance and the flow-indicator/whitespace rejection.

### [103] ns-anchor-name

BNF: `ns-anchor-name ::= ns-anchor-char+`
Spec prose (§6.9.2): "An alias node can then be used to indicate additional inclusions of the anchored node."
Verdict: Strict-conformant
Evidence: `event_iter/properties.rs:27-31` — `take_while(|&(_, ch)| is_ns_anchor_char(ch))` then `.last().map_or(0, |(i, ch)| i + ch.len_utf8())`; line 32-37 rejects empty result with "anchor name must not be empty".
Reasoning: The "+" (one or more) requirement is enforced by the empty-result rejection at line 32-37. Each character is validated against `is_ns_anchor_char` ([102]). The maximum-length check at line 38-43 is a security-hardening cap and does not affect spec conformance for any realistic input.
