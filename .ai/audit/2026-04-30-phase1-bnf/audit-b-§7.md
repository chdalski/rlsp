---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: B
section: §7
date: 2026-04-30
---

### [104] c-ns-alias-node

BNF: `c-ns-alias-node ::= "*" ns-anchor-name`
Spec prose (§7.1): "An alias node is denoted by the '*' indicator. The alias refers to the most recent preceding node having the same anchor. […] Note that an alias node must not specify any properties or content, as these were already specified at the first occurrence of the node."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1355-1423` (`*` arm — error if `pending_flow_tag.is_some()` or `pending_flow_anchor.is_some()`, then `scan_anchor_name` and `Event::Alias` emission); `event_iter/properties.rs:23-45` (`scan_anchor_name` rejects empty names and over-long names); `loader.rs:781-827` (`resolve_alias` returns `LoadError::UndefinedAlias` for unknown names).
Reasoning: The flow path validates that `*name` is not preceded by a tag or anchor before emitting `Event::Alias`, matching the spec's "no properties on alias" rule. The name itself is consumed via `scan_anchor_name`, which uses `is_ns_anchor_char` to delimit the name; this composes correctly with [102]/[103]. The "must refer to a previously defined anchor" requirement is enforced at load time by `resolve_alias`. Together these enforce both the syntactic shape `* ns-anchor-name` and the semantic constraints. The conformance doc's classification matches.

### [105] e-scalar

BNF: `e-scalar ::= ""`
Spec prose (§7.2): "YAML allows the node content to be omitted in many cases. Nodes with empty content are interpreted as if they were plain scalars with an empty value. Such nodes are commonly resolved to a 'null' value."
Verdict: Strict-conformant
Evidence: `lib.rs:173-179` (`empty_scalar_event` constructs `Event::Scalar { value: Cow::Borrowed(""), style: ScalarStyle::Plain, meta: None }`); flow emission sites at `event_iter/flow.rs:512-518`, `729-741`, `1185-1197`, `1225` (empty key/value in flow contexts).
Reasoning: `empty_scalar_event()` returns exactly the empty-string plain scalar the spec describes. It is emitted at every flow site where the spec admits an empty node — empty key on `:`, empty value on `,` or `}`, both empty entries on a bare `?` followed by closing `}`. The construction is a `const fn` returning a value-typed event with no allocation, matching the "" literal in the BNF.

### [106] e-node

BNF: `e-node ::= e-scalar`
Spec prose (§7.2): "Both the node's properties and node content are optional. This allows for a completely empty node. Completely empty nodes are only valid when following some explicit indication for their existence."
Verdict: Strict-conformant
Evidence: `lib.rs:173-179` (`empty_scalar_event`); same flow emission sites as [105].
Reasoning: `e-node` collapses to `e-scalar` — the parser implements both with the same `empty_scalar_event()` constructor, so wherever the grammar admits `e-node` the parser emits a plain empty scalar. The "explicit indication for their existence" precondition is handled by callers — empty events are only inserted at structurally significant points (after `:`, after `?`, before `}`, before `,`).

### [107] nb-double-char

BNF: `nb-double-char ::= c-ns-esc-char | ( nb-json - "\\" - '"' )`
Spec prose (§7.3.1): "The double-quoted style is specified by surrounding '\"' indicators. This is the only style capable of expressing arbitrary strings, by using '\\' escape sequences. This comes at the cost of having to escape the '\\' and '\"' characters."
Verdict: Stricter-than-spec
Evidence: `lexer/quoted.rs:618-751` (`scan_double_quoted_line` — `memchr2(b'"', b'\\', …)` accepts everything between `\` and `"`); `lexer/quoted.rs:557-614` (`decode_and_push_escape` calls `is_c_printable` on `\xHH`/`\uHHHH`/`\UHHHHHHHH` results and rejects bidi controls); `lexer/quoted.rs:606-611` (1 MiB cap).
Reasoning: For literal characters the scanner simply skips bytes between escapes via memchr2, so any byte other than `\\` and `"` is admitted — this matches `nb-json - c-escape - c-double-quote`. The spec definition includes `c-ns-esc-char` (the escape branch). The implementation is stricter than the spec on two axes: (i) hex escapes whose decoded codepoint is not `c-printable` are rejected (`is_c_printable` check), and (ii) escapes that decode to bidirectional control characters U+200E…U+202E and U+2066…U+2069 are rejected. The spec lets `c-ns-esc-char` produce any valid Unicode codepoint, including the rejected ones. The 1 MiB scalar cap is a separate hardening but applies to the accumulated string, not single chars. The conformance doc labels this Conformant; the doc's claim is wrong because the printability gate and bidi gate reject input that satisfies the spec BNF.

### [108] ns-double-char

BNF: `ns-double-char ::= nb-double-char - s-white`
Spec prose (§7.3.1): "All leading and trailing white space characters on each line are excluded from the content."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:618-751` (single-line scan); `lexer/quoted.rs:729-745` (incomplete-line trim of trailing `' '|'\t'`); `lexer/quoted.rs:276-302` (continuation-line `trim_start_matches([' ', '\t'])` for leading whitespace strip).
Reasoning: `ns-double-char` is used by [114]/[115] to require at least one non-whitespace character per continuation line and to strip leading/trailing whitespace at line boundaries. The implementation strips trailing literal whitespace on the closing-incomplete branch (line 738) and leading whitespace on the next continuation line (line 276), which is exactly the per-line whitespace exclusion `ns-double-char` enforces. Within a single line, every non-whitespace character is admitted via the same memchr2 path as [107], so the predicate composes correctly.

### [109] c-double-quoted(n,c)

BNF: `c-double-quoted(n,c) ::= '"' nb-double-text(n,c) '"'`
Spec prose (§7.3.1): "The double-quoted style is specified by surrounding '\"' indicators."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:178-242` (`try_consume_double_quoted` — matches opening `"`, dispatches to `scan_double_quoted_line`, wraps multi-line via `collect_double_quoted_continuations`); `lexer/quoted.rs:259-263` (`unterminated double-quoted scalar` error if no closing `"` before EOF).
Reasoning: The function requires an opening `"` (returns `None` otherwise) and consumes the body until a matching unescaped `"` is found, returning an error if EOF arrives first. The wrapper composes [110] correctly. Strict-conformant at the wrapper level; per-character leniency lives at [107].

### [110] nb-double-text(n,c)

BNF: `nb-double-text(n,FLOW-OUT) ::= nb-double-multi-line(n)` / `nb-double-text(n,FLOW-IN) ::= nb-double-multi-line(n)` / `nb-double-text(n,BLOCK-KEY) ::= nb-double-one-line` / `nb-double-text(n,FLOW-KEY) ::= nb-double-one-line`
Spec prose (§7.3.1): "Double-quoted scalars are restricted to a single line when contained inside an implicit key."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:209-240` (single-line vs multi-line dispatch by `DoubleQuotedLine::Closed` vs `Incomplete`); flow implicit-key single-line enforcement at `event_iter/flow.rs:1128-1135` (DK4H check) and `1151-1160` (1024-char limit catches multi-line accidentally extending across newlines because the buffer slice spans lines); block-key implicit single-line via mapping parser at `event_iter/block/mapping.rs:158-161, 782`.
Reasoning: In `FLOW-OUT`/`FLOW-IN` contexts, `try_consume_double_quoted` enters the multi-line continuation loop when the opening line lacks a closing `"`. In implicit-key contexts (`BLOCK-KEY` and `FLOW-KEY`), the surrounding parsers reject keys that cross lines — `flow.rs:1128` rejects `:` arriving on a different line than the key's last token, and the block mapping handler treats only single-line content as a candidate implicit key. This implements the context-conditional single-line restriction even though `try_consume_double_quoted` itself is context-agnostic. Strict-conformant given the composed enforcement.

### [111] nb-double-one-line

BNF: `nb-double-one-line ::= nb-double-char*`
Spec prose (§7.3.1): "Double-quoted scalars are restricted to a single line when contained inside an implicit key."
Verdict: Stricter-than-spec
Evidence: `lexer/quoted.rs:618-751` (single-line scan path returns `Closed { … tail }` when the closing `"` is found before EOL).
Reasoning: The single-line body is `nb-double-char*`, so leniency/strictness on `nb-double-char` propagates here. Given that [107] is `Stricter-than-spec` on hex-escape printability and bidi controls, the `nb-double-char*` star repetition inherits that strictness. No additional restriction is layered at this production. Conformance doc labels Conformant; that label is wrong for the same reason as [107].

### [112] s-double-escaped(n)

BNF: `s-double-escaped(n) ::= s-white* "\\" b-non-content l-empty(n,FLOW-IN)* s-flow-line-prefix(n)`
Spec prose (§7.3.1): "It is also possible to escape the line break character. In this case, the escaped line break is excluded from the content and any trailing white space characters that precede the escaped line break are preserved."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:670-688` (`\` at end of line yields `DoubleQuotedLine::Incomplete { line_continuation: true }`, preserving prefix verbatim); `lexer/quoted.rs:304-316` (continuation logic — when `line_continuation` is true, the fold separator is suppressed and leading whitespace on the next line is stripped via `trim_start_matches([' ','\t'])` at line 276).
Reasoning: When the body ends with `\\` followed by EOL the parser captures the prefix without trimming and sets `line_continuation = true`. The surrounding loop then advances over blank/empty lines (the `pending_blanks += 1` branch acts like `l-empty(n,FLOW-IN)*` since blank-line preservation is governed by `line_continuation`) and consumes the next line's leading whitespace as `s-flow-line-prefix(n)` (stripped by the leading-whitespace trim). Trailing whitespace before `\` is preserved because the preceding-content path does not trim until EOL is reached without a trailing `\`. Strict-conformant.

### [113] s-double-break(n)

BNF: `s-double-break(n) ::= s-double-escaped(n) | s-flow-folded(n)`
Spec prose (§7.3.1): "In a multi-line double-quoted scalar, line breaks are subject to flow line folding, which discards any trailing white space characters."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:308-316` (line-break dispatch: `line_continuation` branch (`s-double-escaped`) suppresses the fold, otherwise the `s-flow-folded` semantics apply — single LF → space, blank lines → literal newlines).
Reasoning: The parser cleanly composes the two alternatives: an unescaped real newline triggers fold semantics (one fold space, blank-line `\n` accumulation) while a `\<LF>` triggers the escape branch (no fold, no blank-line accumulation). This matches the BNF alternation directly.

### [114] nb-ns-double-in-line

BNF: `nb-ns-double-in-line ::= ( s-white* ns-double-char )*`
Spec prose (§7.3.1): "All leading and trailing white space characters on each line are excluded from the content. Each continuation line must therefore contain at least one non-space character."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:618-751` (in-line scan accepts whitespace between non-whitespace via memchr2 + trailing trim only at EOL); `lexer/quoted.rs:733-745` (Incomplete branch trims trailing literal `' '|'\t'`).
Reasoning: Within a line the scanner accepts arbitrary `s-white` between characters and only trims at the line's end (incomplete branch), which is the in-line `(s-white* ns-double-char)*` shape. Whitespace between characters is preserved verbatim — only the *trailing* run before EOL is excluded.

### [115] s-double-next-line(n)

BNF: `s-double-next-line(n) ::= s-double-break(n) ( ns-double-char nb-ns-double-in-line ( s-double-next-line(n) | s-white* ) )?`
Spec prose (§7.3.1): "Empty lines, if any, are consumed as part of the line folding."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:249-352` (`collect_double_quoted_continuations` — loops over continuation lines, accumulating blanks and processing each non-blank as either Closed or Incomplete).
Reasoning: The continuation loop folds blank lines (counted into `pending_blanks`, output as `\n` repeats) and processes each non-blank line via `scan_double_quoted_line`, then either closes or recurses. The check at line 282 ("non-blank continuation must have indent > n") enforces the indentation requirement implied by `s-flow-folded` composition. The whole loop matches the recursive shape of `s-double-next-line`.

### [116] nb-double-multi-line(n)

BNF: `nb-double-multi-line(n) ::= nb-ns-double-in-line ( s-double-next-line(n) | s-white* )`
Spec prose (§7.3.1): "Empty lines, if any, are consumed as part of the line folding."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:178-242` (multi-line wrapper dispatch); `lexer/quoted.rs:249-352` (continuation loop).
Reasoning: First line is consumed via `nb-ns-double-in-line` (the in-line scan), then either continuation lines follow (`s-double-next-line`) or trailing whitespace (`s-white*`) before the closing `"`. The implementation matches by structure: first-line scan returns Incomplete for multi-line, the continuation loop processes successive lines, and trailing whitespace on each non-final line is trimmed before fold.

### [117] c-quoted-quote

BNF: `c-quoted-quote ::= "''"`
Spec prose (§7.3.2): "The single-quoted style is specified by surrounding \"'\" indicators. […] Therefore, within a single-quoted scalar, such characters need to be repeated. This is the only form of escaping performed in single-quoted scalars."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:415-455` (`scan_single_quoted_line` — when a `'` is found and the next byte is also `'`, both are consumed and `has_escape = true`); `lexer/quoted.rs:461-481` (`unescape_single_quoted` replaces each `''` with a single `'`).
Reasoning: The scanner explicitly distinguishes `''` (escape) from `'` (close) by lookahead one byte. The unescape routine emits a single `'` for each `''` sequence, exactly matching the production.

### [118] nb-single-char

BNF: `nb-single-char ::= c-quoted-quote | ( nb-json - "'" )`
Spec prose (§7.3.2): "In particular, the '\\' and '\"' characters may be freely used. This restricts single-quoted scalars to printable characters."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:415-455` (any byte other than `'` passes through; `''` is the escape).
Reasoning: The body scan accepts every byte except `'`, where a single `'` closes and `''` escapes — exactly the alternation in the BNF. No printability check is applied to literal single-quoted bytes (unlike the double-quoted hex-escape path), so `c-printable` enforcement here lives only at the file-level character-class layer (`nb-json` itself is permissive). This matches the spec.

### [119] ns-single-char

BNF: `ns-single-char ::= nb-single-char - s-white`
Spec prose (§7.3.2): "In addition, it is only possible to break a long single-quoted line where a space character is surrounded by non-spaces."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:75-77` (multi-line: `owned.truncate(owned.trim_end_matches([' ','\t']).len())` strips trailing whitespace from the first line); `lexer/quoted.rs:107-152` (continuation: `trimmed = line.trim_start_matches([' ','\t'])`, blank lines yield `\n`, non-blank lines stripped of leading whitespace then folded with a space).
Reasoning: At each line boundary, leading and trailing literal whitespace is stripped — equivalent to requiring `ns-single-char` (non-whitespace) at each line edge for fold purposes. The fold logic adds a space between non-blank lines and a `\n` per blank line, matching the spec's "break only at space-surrounded-by-non-spaces" requirement (in practice: trim line edges + add fold space).

### [120] c-single-quoted(n,c)

BNF: `c-single-quoted(n,c) ::= "'" nb-single-text(n,c) "'"`
Spec prose (§7.3.2): "The single-quoted style is specified by surrounding \"'\" indicators."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:27-153` (`try_consume_single_quoted` — opening `'` matched, body via `scan_single_quoted_line`, closing `'` required, EOF without close returns `unterminated single-quoted scalar`).
Reasoning: Wrapper structure exactly mirrors the BNF — opening `'`, body, closing `'`. The body production [121] is delegated to the helper.

### [121] nb-single-text(n,c)

BNF: `nb-single-text(FLOW-OUT) ::= nb-single-multi-line(n)` / `nb-single-text(FLOW-IN) ::= nb-single-multi-line(n)` / `nb-single-text(BLOCK-KEY) ::= nb-single-one-line` / `nb-single-text(FLOW-KEY) ::= nb-single-one-line`
Spec prose (§7.3.2): "Single-quoted scalars are restricted to a single line when contained inside a implicit key."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:60-153` (single-line fast-path when `closed = true` on the first line; multi-line otherwise); single-line implicit-key restriction in flow at `event_iter/flow.rs:1128-1135` and block at `event_iter/block/mapping.rs:158-161,782`.
Reasoning: The body method itself processes both single-line and multi-line scalars; the implicit-key restriction is enforced upstream by the flow and block mapping parsers, which reject `:` separators that span lines. Same composition argument as [110].

### [122] nb-single-one-line

BNF: `nb-single-one-line ::= nb-single-char*`
Spec prose (§7.3.2): "Single-quoted scalars are restricted to a single line when contained inside a implicit key."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:60-72` (single-line path: `if closed { ... return Cow::Borrowed slice ... }`).
Reasoning: When the closing `'` is on the same line as the opening `'`, the result is a borrowed slice equal to `nb-single-char*` — matching the BNF directly.

### [123] nb-ns-single-in-line

BNF: `nb-ns-single-in-line ::= ( s-white* ns-single-char )*`
Spec prose (§7.3.2): "All leading and trailing white space characters are excluded from the content. Each continuation line must therefore contain at least one non-space character."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:415-455` (in-line scan preserves intermediate whitespace); `lexer/quoted.rs:75-77` (trailing-whitespace trim at first-line end).
Reasoning: Within a line, whitespace between non-whitespace characters is included verbatim. Trailing whitespace at the line end is excluded before fold — exactly the behaviour the BNF describes.

### [124] s-single-next-line(n)

BNF: `s-single-next-line(n) ::= s-flow-folded(n) ( ns-single-char nb-ns-single-in-line ( s-single-next-line(n) | s-white* ) )?`
Spec prose (§7.3.2): "Each continuation line must therefore contain at least one non-space character. Empty lines, if any, are consumed as part of the line folding."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:79-153` (continuation loop: blank lines emit `\n`, non-blank lines start with stripped leading whitespace and contribute their content prefixed by either a fold space or accumulated newlines).
Reasoning: The loop applies `s-flow-folded` (blank-line fold + leading-whitespace trim + fold space) before processing each continuation line, which matches the BNF. A continuation line is required to contain non-whitespace content (after `trim_start_matches`, a non-empty `trimmed` is checked). Strict-conformant.

### [125] nb-single-multi-line(n)

BNF: `nb-single-multi-line(n) ::= nb-ns-single-in-line ( s-single-next-line(n) | s-white* )`
Spec prose (§7.3.2): "Empty lines, if any, are consumed as part of the line folding."
Verdict: Strict-conformant
Evidence: `lexer/quoted.rs:60-153` (first-line in-line content + continuation loop).
Reasoning: First line is `nb-ns-single-in-line`; subsequent lines are `s-single-next-line(n)*` until close; trailing whitespace before the close is stripped (line 152). Composition matches BNF.

### [126] ns-plain-first(c)

BNF: `ns-plain-first(c) ::= ( ns-char - c-indicator ) | ( ( "?" | ":" | "-" ) [ lookahead = ns-plain-safe(c) ] )`
Spec prose (§7.3.3): "Plain scalars must not begin with most indicators, as this would cause ambiguity with other YAML constructs. However, the ':', '?' and '-' indicators may be used as the first character if followed by a non-space 'safe' character, as this causes no ambiguity."
Verdict: Strict-conformant
Evidence: `lexer/plain.rs:287-302` (`ns_plain_first_block` — explicit `is_c_indicator` check; `?`, `:`, `-` admitted only if the following char satisfies `ns_plain_safe_block`); flow-context first-char dispatch at `event_iter/flow.rs:1536-1562` (excludes `, [ ] { } # & * ! | > ' " % @ \``; admits `? : -` only when followed by a non-separator).
Reasoning: Block first-char check uses `is_c_indicator` to reject 19 indicators, then specifically allows `?`, `:`, `-` if the subsequent char is `ns-plain-safe`. The flow path inlines the same logic but augments the lookahead set with flow indicators (`,[]{}`) — which is correct for `ns-plain-safe(FLOW-IN)`. Conformance doc's classification matches.

### [127] ns-plain-safe(c)

BNF: `ns-plain-safe(FLOW-OUT) ::= ns-plain-safe-out` / `ns-plain-safe(FLOW-IN) ::= ns-plain-safe-in` / `ns-plain-safe(BLOCK-KEY) ::= ns-plain-safe-out` / `ns-plain-safe(FLOW-KEY) ::= ns-plain-safe-in`
Spec prose (§7.3.3): "Plain scalars must never contain the ': ' and ' #' character combinations. […] In addition, inside flow collections, or when used as implicit keys, plain scalars must not contain the '[', ']', '{', '}' and ',' characters."
Verdict: Strict-conformant
Evidence: `lexer/plain.rs:307-309` (`ns_plain_safe_block ::= is_ns_char(ch)` — used for FLOW-OUT/BLOCK-KEY); `lexer/plain.rs:431-502` (`scan_plain_line_flow` — additionally stops at `,[]{}` for FLOW-IN/FLOW-KEY).
Reasoning: Two distinct scanners switch on context — block scanner uses `is_ns_char` (no flow-indicator exclusion), flow scanner adds `,[]{}` to the terminator set. This matches the four-arm context dispatch in the BNF: FLOW-OUT and BLOCK-KEY use `ns-plain-safe-out`; FLOW-IN and FLOW-KEY use `ns-plain-safe-in`. The block-key dispatch within the flow scanner is satisfied because flow-key lookups also pass through `scan_plain_line_flow`.

### [128] ns-plain-safe-out

BNF: `ns-plain-safe-out ::= ns-char`
Spec prose (§7.3.3): "Plain scalars must never contain the ': ' and ' #' character combinations. Such combinations would cause ambiguity with mapping key/value pairs and comments."
Verdict: Strict-conformant
Evidence: `lexer/plain.rs:307-309` (`ns_plain_safe_block(ch) ::= is_ns_char(ch)`); `chars.rs:67-76` (`is_ns_char` — non-whitespace, non-BOM, c-printable).
Reasoning: Direct delegation to `is_ns_char`, which is exactly `ns-char`. Production [128] is the identity reduction `ns-plain-safe-out ::= ns-char`, satisfied trivially.

### [129] ns-plain-safe-in

BNF: `ns-plain-safe-in ::= ns-char - c-flow-indicator`
Spec prose (§7.3.3): "In addition, inside flow collections, or when used as implicit keys, plain scalars must not contain the '[', ']', '{', '}' and ',' characters."
Verdict: Strict-conformant
Evidence: `lexer/plain.rs:467` (`b',' | b'[' | b']' | b'{' | b'}' | 0x00..=0x1F | 0x7F => return &content[..committed_end]` — terminates flow plain scan at any flow indicator).
Reasoning: The flow scanner explicitly stops at every flow indicator, plus all controls, plus DEL. Combined with the non-ASCII path checking `ns_plain_safe_block` (= `is_ns_char`), the result is exactly `ns-char - c-flow-indicator`. Strict-conformant.

### [130] ns-plain-char(c)

BNF: `ns-plain-char(c) ::= ( ns-plain-safe(c) - ":" - "#" ) | ( [ lookbehind = ns-char ] "#" ) | ( ":" [ lookahead = ns-plain-safe(c) ] )`
Spec prose (§7.3.3): "Plain scalars must never contain the ': ' and ' #' character combinations. Such combinations would cause ambiguity with mapping key/value pairs and comments."
Verdict: Strict-conformant
Evidence: `lexer/plain.rs:320-330` (`ns_plain_char_block` — `#` admitted only when `prev_was_ws` is false; `:` admitted only when followed by `ns_plain_safe_block`); `lexer/plain.rs:340-416` (block scanner enforces both predicates while walking the line); `lexer/plain.rs:431-502` (flow scanner enforces same logic with flow-aware terminators).
Reasoning: The BNF disjunction has three branches — non-`:`/`#` safe char, `#` with non-whitespace lookbehind, `:` with safe lookahead. The implementation tracks `prev_was_ws` to gate `#` and looks one char ahead at every `:` to decide whether to consume it. Trailing-whitespace runs are excluded via `committed_end` not advancing through `' '|'\t'`. This matches the spec exactly.

### [131] ns-plain(n,c)

BNF: `ns-plain(n,FLOW-OUT) ::= ns-plain-multi-line(n,FLOW-OUT)` / `ns-plain(n,FLOW-IN) ::= ns-plain-multi-line(n,FLOW-IN)` / `ns-plain(n,BLOCK-KEY) ::= ns-plain-one-line(BLOCK-KEY)` / `ns-plain(n,FLOW-KEY) ::= ns-plain-one-line(FLOW-KEY)`
Spec prose (§7.3.3): "Plain scalars are further restricted to a single line when contained inside an implicit key."
Verdict: Strict-conformant
Evidence: `lexer/plain.rs:31-143` (`try_consume_plain_scalar` — multi-line via `collect_plain_continuations`); `lexer/plain.rs:431-502` (flow path is single-line per call — multi-line in flow is handled by the continuation extension logic at `event_iter/flow.rs:1441-1528`); `event_iter/block/mapping.rs:158-161` and `event_iter/flow.rs:1128` (single-line restriction in implicit-key contexts).
Reasoning: In FLOW-OUT and FLOW-IN, multi-line continuation accumulates content via `collect_plain_continuations` (block) or via the flow-context multi-line extension. In BLOCK-KEY and FLOW-KEY, the surrounding parsers reject `:` separators across lines, restricting effective consumption to one line. Composition matches the four-context dispatch.

### [132] nb-ns-plain-in-line(c)

BNF: `nb-ns-plain-in-line(c) ::= ( s-white* ns-plain-char(c) )*`
Spec prose (§7.3.3): "In addition to a restricted character set, a plain scalar must not be empty or contain leading or trailing white space characters."
Verdict: Strict-conformant
Evidence: `lexer/plain.rs:340-416` (block scanner inner loop); `lexer/plain.rs:431-502` (flow scanner inner loop) — both track `committed_end` (last accepted non-whitespace position), so trailing whitespace runs are excluded.
Reasoning: Whitespace between non-whitespace characters is included; trailing whitespace at line end is excluded by `committed_end` lagging behind `pos` when the loop exits. This matches the `(s-white* ns-plain-char)*` shape: any number of internal whitespace runs are admitted, but a trailing run is not committed.

### [133] ns-plain-one-line(c)

BNF: `ns-plain-one-line(c) ::= ns-plain-first(c) nb-ns-plain-in-line(c)`
Spec prose (§7.3.3): "Plain scalars are further restricted to a single line when contained inside an implicit key."
Verdict: Strict-conformant
Evidence: `lexer/plain.rs:242-273` (`peek_plain_scalar_first_line` checks `ns_plain_first_block` then runs `scan_plain_line_block` for the rest); flow first-char + scan composition at `event_iter/flow.rs:1536-1597`.
Reasoning: The first-character predicate is applied separately, then the body scanner produces `nb-ns-plain-in-line`. Composition matches the BNF.

### [134] s-ns-plain-next-line(n,c)

BNF: `s-ns-plain-next-line(n,c) ::= s-flow-folded(n) ns-plain-char(c) nb-ns-plain-in-line(c)`
Spec prose (§7.3.3): "Empty lines, if any, are consumed as part of the line folding."
Verdict: Strict-conformant
Evidence: `lexer/plain.rs:149-230` (`collect_plain_continuations` — blank lines accumulate as pending newlines, non-blank continuation must produce non-empty `scan_plain_line_block`, fold separator is `\n`-repeats or space).
Reasoning: A continuation line is processed only when it contains at least one valid plain char (`cont_value.is_empty() => break`), and the leading whitespace is implicitly handled by `trim_start_matches`. Blank-line accumulation drives the `\n` repetition. Strict-conformant.

### [135] ns-plain-multi-line(n,c)

BNF: `ns-plain-multi-line(n,c) ::= ns-plain-one-line(c) s-ns-plain-next-line(n,c)*`
Spec prose (§7.3.3): "It is only possible to break a long plain line where a space character is surrounded by non-spaces."
Verdict: Strict-conformant
Evidence: `lexer/plain.rs:31-143` (`try_consume_plain_scalar` — one-line scan + zero-or-more continuations).
Reasoning: First line is consumed via `peek_plain_scalar_first_line` + `scan_plain_line_block` (=`ns-plain-one-line`); continuations via `collect_plain_continuations` (=`s-ns-plain-next-line*`). Termination conditions (dedent, marker, blank-then-`:`) match the spec's plain-scalar termination semantics.

### [136] in-flow(n,c)

BNF: `in-flow(n,FLOW-OUT) ::= ns-s-flow-seq-entries(n,FLOW-IN)` / `in-flow(n,FLOW-IN) ::= ns-s-flow-seq-entries(n,FLOW-IN)` / `in-flow(n,BLOCK-KEY) ::= ns-s-flow-seq-entries(n,FLOW-KEY)` / `in-flow(n,FLOW-KEY) ::= ns-s-flow-seq-entries(n,FLOW-KEY)`
Spec prose (§7.4): "A flow collection may be nested within a block collection (FLOW-OUT context), nested within another flow collection (FLOW-IN context) or be a part of an implicit key (FLOW-KEY context or BLOCK-KEY context). Flow collection entries are terminated by the ',' indicator."
Verdict: Not-applicable
Evidence: n/a.
Reasoning: `in-flow` is a context-mapping rule that only renames the outer context to the appropriate inner context for the entries production — it has no observable parser obligation distinct from the productions it forwards to. The implementation handles context propagation implicitly: the flow main loop in `event_iter/flow.rs` always parses entries with FLOW-IN-equivalent rules (flow indicators are terminators) and applies the implicit-key length and single-line checks where the context is FLOW-KEY-derived. Production is meta-notational for the parser.

### [137] c-flow-sequence(n,c)

BNF: `c-flow-sequence(n,c) ::= "[" s-separate(n,c)? in-flow(n,c)? "]"`
Spec prose (§7.4.1): "Flow sequence content is denoted by surrounding '[' and ']' characters."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:394-446` (`'['` arm — depth check, `Event::SequenceStart` push, frame push); `event_iter/flow.rs:475-500` (`']'` arm — frame pop, `Event::SequenceEnd` push); `event_iter/flow.rs:528-534` (mismatch error if `}` arrives instead of `]`).
Reasoning: Open and close are matched and emit the expected start/end events. The optional separator and entries are consumed by the main-loop dispatch on `,` / scalars / nested collections. Strict-conformant.

### [138] ns-s-flow-seq-entries(n,c)

BNF: `ns-s-flow-seq-entries(n,c) ::= ns-flow-seq-entry(n,c) s-separate(n,c)? ( "," s-separate(n,c)? ns-s-flow-seq-entries(n,c)? )?`
Spec prose (§7.4.1): "Sequence entries are separated by a ',' character."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:662-831` (`,` arm — leading-comma check at line 666-680, double-comma check at 693-701, frame reset at 789-822); trailing comma allowed because `,` followed by `]` is valid (the next iteration reads `]` and pops the frame).
Reasoning: Comma is the entry separator. Leading commas (`[,]`) and double commas (`[a,,b]`) are rejected. Trailing comma (`[a,]`) is permitted because closing `]` is dispatched separately. The recursion in the BNF is implemented as iteration on the main loop. Strict-conformant.

### [139] ns-flow-seq-entry(n,c)

BNF: `ns-flow-seq-entry(n,c) ::= ns-flow-pair(n,c) | ns-flow-node(n,c)`
Spec prose (§7.4.1): "Any flow node may be used as a flow sequence entry. In addition, YAML provides a compact notation for the case where a flow sequence entry is a mapping with a single key/value pair."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1210-1250` (when `:` arrives inside a `Sequence` frame, `MappingStart` is inserted at `key_start_idx` to wrap the key — single-pair compact mapping path); same dispatch site otherwise emits a normal flow node.
Reasoning: The implementation defers the choice: each entry is parsed as a node, and if a `:` value separator arrives before the next `,` or `]`, the entry is retroactively wrapped in `MappingStart`/`MappingEnd`. The `key_start_idx` is recorded to support exactly this insertion. Strict-conformant.

### [140] c-flow-mapping(n,c)

BNF: `c-flow-mapping(n,c) ::= "{" s-separate(n,c)? ns-s-flow-map-entries(n,in-flow(c))? "}"`
Spec prose (§7.4.2): "Flow mappings are denoted by surrounding '{' and '}' characters."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:447-468` (`'{'` arm pushes `Mapping` frame and `Event::MappingStart`); `event_iter/flow.rs:501-520` (`'}'` arm — emits trailing empty key/value if needed, then `Event::MappingEnd`); `event_iter/flow.rs:521-527` (mismatch error if `]` arrives instead of `}`).
Reasoning: Open and close are matched, with appropriate completion of dangling pairs (`?` with no key → null/null; Value-phase close → empty value). Strict-conformant.

### [141] ns-s-flow-map-entries(n,c)

BNF: `ns-s-flow-map-entries(n,c) ::= ns-flow-map-entry(n,c) s-separate(n,c)? ( "," s-separate(n,c)? ns-s-flow-map-entries(n,c)? )?`
Spec prose (§7.4.2): "Mapping entries are separated by a ',' character."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:662-831` (shared `,` handling for both Sequence and Mapping frames; mapping reset at 804-820 sets phase back to Key).
Reasoning: Same comma-separator infrastructure as [138], with phase reset to Key on each comma (so the next entry begins a fresh key/value pair). Trailing comma admitted. Strict-conformant.

### [142] ns-flow-map-entry(n,c)

BNF: `ns-flow-map-entry(n,c) ::= ( "?" s-separate(n,c) ns-flow-map-explicit-entry(n,c) ) | ns-flow-map-implicit-entry(n,c)`
Spec prose (§7.4.2): "If the optional '?' mapping key indicator is specified, the rest of the entry may be completely empty."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1057-1083` (`?` arm — sets `explicit_key_pending = true` on the current Mapping frame, advances past `?`); the implicit branch falls through to scalar/quoted/collection dispatch.
Reasoning: `?` is consumed only when followed by whitespace/EOL (otherwise treated as plain-scalar start), exactly matching `(? s-separate ...)`. The flag `explicit_key_pending` enables the empty-key empty-value emission at `}` (production [143]).

### [143] ns-flow-map-explicit-entry(n,c)

BNF: `ns-flow-map-explicit-entry(n,c) ::= ns-flow-map-implicit-entry(n,c) | ( e-node e-node )`
Spec prose (§7.4.2): "If the optional '?' mapping key indicator is specified, the rest of the entry may be completely empty."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:511-513` (when `}` arrives in Key phase with `explicit_key_pending`, emit two `empty_scalar_event()`s — empty key, empty value).
Reasoning: The two-empty-node path is hit specifically after `?` with no following content, before `}` or `,`. The implicit-entry alternative is the same path that runs without `explicit_key_pending`. Both alternatives of the BNF are covered.

### [144] ns-flow-map-implicit-entry(n,c)

BNF: `ns-flow-map-implicit-entry(n,c) ::= ns-flow-map-yaml-key-entry(n,c) | c-ns-flow-map-empty-key-entry(n,c) | c-ns-flow-map-json-key-entry(n,c)`
Spec prose (§7.4.2): "Normally, YAML insists the ':' mapping value indicator be separated from the value by white space."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1088-1252` (dispatch on `:` for empty-key entry; dispatch to quoted scalar for JSON-key entry — line 877+; dispatch to plain scalar for YAML-key entry — line 1532+).
Reasoning: All three sub-productions are reached by the main loop's character dispatch — `:` first (empty key), `'`/`"` (JSON key), or any `ns-plain-first` (YAML key). The first-character dispatch order matches the BNF alternatives.

### [145] ns-flow-map-yaml-key-entry(n,c)

BNF: `ns-flow-map-yaml-key-entry(n,c) ::= ns-flow-yaml-node(n,c) ( ( s-separate(n,c)? c-ns-flow-map-separate-value(n,c) ) | e-node )`
Spec prose (§7.4.2): "A benefit of this restriction is that the ':' character can be used inside plain scalars, as long as it is not followed by white space."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1532-1675` (plain scalar in Key phase advances to Value phase and marks `has_value = true`); `event_iter/flow.rs:1086-1251` (`:` arm in Value phase consumes the separator); empty-value path at line 514-518 (Value-phase close emits empty scalar).
Reasoning: After a plain key is emitted, either `:` appears as the value separator (followed by either an explicit value or empty), or the entry ends with no `:` (rare — only valid in compact form with trailing `,` or `}` where empty value is implicit). Both branches are handled.

### [146] c-ns-flow-map-empty-key-entry(n,c)

BNF: `c-ns-flow-map-empty-key-entry(n,c) ::= e-node c-ns-flow-map-separate-value(n,c)`
Spec prose (§7.4.2): "Note that the value may be completely empty since its existence is indicated by the ':'."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1182-1199` (when `:` arrives in Mapping Key phase with `has_value = false`, emit an empty plain scalar as the key, then proceed to value).
Reasoning: The empty-key path is triggered by `:` in Key phase before any key content. An `empty_scalar_event` is pushed at the position of `:`, then the phase advances to Value, where the value may be a node or empty.

### [147] c-ns-flow-map-separate-value(n,c)

BNF: `c-ns-flow-map-separate-value(n,c) ::= ":" [ lookahead ≠ ns-plain-safe(c) ] ( ( s-separate(n,c) ns-flow-node(n,c) ) | e-node )`
Spec prose (§7.4.2): "Normally, YAML insists the ':' mapping value indicator be separated from the value by white space. A benefit of this restriction is that the ':' character can be used inside plain scalars, as long as it is not followed by white space."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1098-1119` (`:` is recognised as a value separator only when (a) followed by whitespace/`,]}/EOL` (`is_standard_sep`), or (b) `is_adjacent_json_sep` for JSON-like keys, or (c) Value-phase mapping with `after_colon = false` after a JSON key); `lexer/plain.rs:404-408` (block plain scan terminates at `:` followed by non-`ns_plain_safe_block`).
Reasoning: The `is_standard_sep` lookahead matches `lookahead ≠ ns-plain-safe(c)` — `:` is a separator when followed by a non-safe char (whitespace, `,`, `]`, `}`, EOL). The exception for JSON-key adjacency is the [149] rule, layered on top. After the separator, either a node follows or an empty value is implicit — the empty-value path is the Value-phase `,`/`}` dispatch. Strict-conformant.

### [148] c-ns-flow-map-json-key-entry(n,c)

BNF: `c-ns-flow-map-json-key-entry(n,c) ::= c-flow-json-node(n,c) ( ( s-separate(n,c)? c-ns-flow-map-adjacent-value(n,c) ) | e-node )`
Spec prose (§7.4.2): "To ensure JSON compatibility, if a key inside a flow mapping is JSON-like, YAML allows the following value to be specified adjacent to the ':'. This causes no ambiguity, as all JSON-like keys are surrounded by indicators."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:877-1051` (quoted-scalar key path emits `Event::Scalar` and advances Mapping Key→Value phase); `event_iter/flow.rs:1109-1117` (`is_mapping_value_phase` allows `:` adjacent to value when `after_colon = false`).
Reasoning: After a JSON-like key (quoted scalar in this implementation; nested flow collections work because the close-bracket arm advances to Value phase too — line 564-572), `:` is consumed even when not followed by whitespace, because `is_mapping_value_phase` covers that case. Mapping phase then advances to Value, and the next node is the value. Empty-value branch via the Value-phase `,`/`}` path.

### [149] c-ns-flow-map-adjacent-value(n,c)

BNF: `c-ns-flow-map-adjacent-value(n,c) ::= ":" ( ( s-separate(n,c)? ns-flow-node(n,c) ) | e-node )`
Spec prose (§7.4.2): "To ensure JSON compatibility, if a key inside a flow mapping is JSON-like, YAML allows the following value to be specified adjacent to the ':'."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1109-1117` (Value-phase `:` accepted with no whitespace lookahead requirement); `event_iter/flow.rs:514-518` (`}` in Value phase emits empty value).
Reasoning: Adjacent `:` after a JSON-like key is admitted via the `is_mapping_value_phase` branch, which does not require the standard whitespace lookahead. The value may be a flow node (next iteration of dispatch) or empty (closing delimiter / comma).

### [150] ns-flow-pair(n,c)

BNF: `ns-flow-pair(n,c) ::= ( "?" s-separate(n,c) ns-flow-map-explicit-entry(n,c) ) | ns-flow-pair-entry(n,c)`
Spec prose (§7.4.3): "A more compact notation is usable inside flow sequences, if the mapping contains a single key/value pair. […] Note that it is not possible to specify any node properties for the mapping in this case."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1057-1083` (in a Sequence frame, `?` sets `explicit_key_in_seq = true` so the corresponding `:` may span lines); `event_iter/flow.rs:1210-1246` (`:` in Sequence frame inserts `MappingStart` at `key_start_idx` and sets `in_implicit_map = true`); `event_iter/flow.rs:497-499` (`MappingEnd` emitted before `SequenceEnd` when sequence frame closes with `in_implicit_map = true`); same `MappingEnd` emission before `,` at line 779-786.
Reasoning: A single-pair mapping inside a sequence is detected when `:` arrives without an enclosing `{...}`. The implementation retroactively inserts `MappingStart` at the key's start position and emits `MappingEnd` before the next `,` or before `]`. Properties on the synthesized mapping are not allowed (no `make_meta` call when inserting MappingStart at line 1233 — `meta: None`), matching the spec's prohibition.

### [151] ns-flow-pair-entry(n,c)

BNF: `ns-flow-pair-entry(n,c) ::= ns-flow-pair-yaml-key-entry(n,c) | c-ns-flow-map-empty-key-entry(n,c) | c-ns-flow-pair-json-key-entry(n,c)`
Spec prose (§7.4.3): "A more compact notation is usable inside flow sequences, if the mapping contains a single key/value pair."
Verdict: Strict-conformant
Evidence: Same dispatch as [144] in Sequence-frame context: `:` first → empty-key entry; quoted → JSON-key entry; plain → YAML-key entry. See `event_iter/flow.rs:1210-1246` for empty-key dispatch (`if !*has_value { events.push(empty_scalar_event(), …); }`).
Reasoning: Inside a sequence with a `:` before any key content, an empty key is synthesized. With a quoted scalar before `:`, the JSON-key-entry path is taken (adjacent `:` accepted via `is_adjacent_json_sep` flag at line 1100-1108 which checks for synthetic line — quoted-scalar tail prepended as synthetic line). With a plain scalar before `:` separated by space, the YAML-key path is taken. All three alternatives covered.

### [152] ns-flow-pair-yaml-key-entry(n,c)

BNF: `ns-flow-pair-yaml-key-entry(n,c) ::= ns-s-implicit-yaml-key(FLOW-KEY) c-ns-flow-map-separate-value(n,c)`
Spec prose (§7.4.3): "A more compact notation is usable inside flow sequences, if the mapping contains a single key/value pair."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1532-1597` (plain scalar in Sequence frame — captures `key_start_byte` when `is_key_pos = true`); `event_iter/flow.rs:1118-1163` (DK4H single-line check rejects multi-line plain key inside sequence; 1024-char limit enforced via `key_start_byte..colon_byte` slice).
Reasoning: A plain scalar in a Sequence Key position is recorded as an implicit key candidate. The DK4H check rejects keys spanning lines (matching the FLOW-KEY restriction in `ns-s-implicit-yaml-key`). The 1024-char check then validates length. The separator handling matches [147].

### [153] c-ns-flow-pair-json-key-entry(n,c)

BNF: `c-ns-flow-pair-json-key-entry(n,c) ::= c-s-implicit-json-key(FLOW-KEY) c-ns-flow-map-adjacent-value(n,c)`
Spec prose (§7.4.3): "A more compact notation is usable inside flow sequences, if the mapping contains a single key/value pair."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:877-1051` (quoted scalar in Sequence frame); `event_iter/flow.rs:1100-1108` (`is_adjacent_json_sep` allows `:` immediately after a synthetic-line-prepended quoted scalar — the closing-quote tail becomes synthetic).
Reasoning: Quoted scalars in Sequence frames are followed by an adjacent `:` via the synthetic-line mechanism. The 1024-char limit applies via `key_start_byte` recorded when the quoted scalar opens (line 898).

### [154] ns-s-implicit-yaml-key(c)

BNF: `ns-s-implicit-yaml-key(c) ::= ns-flow-yaml-node(0,c) s-separate-in-line? /* At most 1024 characters altogether */`
Spec prose (§7.4.3): "To limit the amount of lookahead required, the ':' indicator must appear at most 1024 Unicode characters beyond the start of the key. In addition, the key is restricted to a single line."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1118-1135` (single-line check — DK4H — rejects implicit key whose `:` is on a different line than the last key token); `event_iter/flow.rs:1136-1161` (1024-char check — `self.input[key_start_byte..colon_byte].chars().count() > 1024` rejects long implicit keys; `key_is_explicit` and `explicit_key_in_seq` exempt explicit keys).
Reasoning: The two normative requirements — single line and 1024 chars — are explicitly enforced at the `:` separator dispatch. The character counter uses `chars().count()` (Unicode chars, not bytes), matching the spec's "1024 Unicode characters". Tests in `tests/implicit_key_length.rs` cover both ASCII and multi-byte cases.

### [155] c-s-implicit-json-key(c)

BNF: `c-s-implicit-json-key(c) ::= c-flow-json-node(0,c) s-separate-in-line? /* At most 1024 characters altogether */`
Spec prose (§7.4.3): "To limit the amount of lookahead required, the ':' indicator must appear at most 1024 Unicode characters beyond the start of the key."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:884-899` (when a quoted scalar starts in a key position, `key_start_byte` is set to the opening quote's byte offset); `event_iter/flow.rs:1136-1161` (shared 1024-char check).
Reasoning: Quoted JSON-like keys participate in the same length check as plain keys, anchored at the opening quote. Single-line restriction is also shared (multi-line quoted scalar followed by `:` would fall under the DK4H check). Conformance doc's note about multi-line flow keys (`flow.rs:606`) covers the case where a flow collection (e.g. `[...]`) is the key.

### [156] ns-flow-yaml-content(n,c)

BNF: `ns-flow-yaml-content(n,c) ::= ns-plain(n,c)`
Spec prose (§7.5): "JSON-like flow styles all have explicit start and end indicators. The only flow style that does not have this property is the plain scalar."
Verdict: Strict-conformant
Evidence: `lexer/plain.rs:431-502` (flow plain scan); `event_iter/flow.rs:1532-1675` (flow plain dispatch).
Reasoning: Direct delegation to [131] `ns-plain` via the flow scanner.

### [157] c-flow-json-content(n,c)

BNF: `c-flow-json-content(n,c) ::= c-flow-sequence(n,c) | c-flow-mapping(n,c) | c-single-quoted(n,c) | c-double-quoted(n,c)`
Spec prose (§7.5): "JSON-like flow styles all have explicit start and end indicators."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:394-468` (sequence/mapping start dispatch); `event_iter/flow.rs:877-1051` (quoted scalar dispatch).
Reasoning: All four JSON-like content forms are handled by the main dispatch. Each starts with a unique indicator (`[`, `{`, `'`, `"`) that the dispatch matches first. Strict-conformant by composition.

### [158] ns-flow-content(n,c)

BNF: `ns-flow-content(n,c) ::= ns-flow-yaml-content(n,c) | c-flow-json-content(n,c)`
Spec prose (§7.5): "A complete flow node also has optional node properties, except for alias nodes which refer to the anchored node properties."
Verdict: Strict-conformant
Evidence: Same dispatch — plain scalar branch + JSON-content branches.
Reasoning: Disjunction covered by the union of [156] and [157] dispatches.

### [159] ns-flow-yaml-node(n,c)

BNF: `ns-flow-yaml-node(n,c) ::= c-ns-alias-node | ns-flow-yaml-content(n,c) | ( c-ns-properties(n,c) ( ( s-separate(n,c) ns-flow-yaml-content(n,c) ) | e-scalar ) )`
Spec prose (§7.5): "A complete flow node also has optional node properties, except for alias nodes which refer to the anchored node properties."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1355-1423` (alias `*` arm — alias node with no properties allowed); `event_iter/flow.rs:1258-1350` (tag `!` arm) and `1317-1350` (anchor `&` arm) — properties accumulate in `pending_flow_tag` / `pending_flow_anchor`; properties-without-content dispatch at line 723-741 — when `,` arrives with `pending_flow_tag.is_some() || pending_flow_anchor.is_some()`, an empty scalar with the properties attached is emitted.
Reasoning: Alias path explicitly rejects pending tag/anchor (lines 1359-1365, 1366-1372) — matching "alias nodes refer to the anchored node properties" (no fresh properties allowed). Content-with-properties path attaches `pending_flow_tag` / `pending_flow_anchor` to the next emitted scalar via `make_meta`. The empty-scalar fallback handles the `c-ns-properties e-scalar` alternative.

### [160] c-flow-json-node(n,c)

BNF: `c-flow-json-node(n,c) ::= ( c-ns-properties(n,c) s-separate(n,c) )? c-flow-json-content(n,c)`
Spec prose (§7.5): "A complete flow node also has optional node properties, except for alias nodes which refer to the anchored node properties."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:1258-1350` (tag `!` and anchor `&` consume properties before collection/scalar dispatch); pending properties are attached to the next emitted scalar/collection via `make_meta` at every emission site.
Reasoning: Properties may appear before any of the four JSON-like forms; the parser dispatches the property arms first and stores them as pending until the next content emits. Composition matches.

### [161] ns-flow-node(n,c)

BNF: `ns-flow-node(n,c) ::= c-ns-alias-node | ns-flow-content(n,c) | ( c-ns-properties(n,c) ( ( s-separate(n,c) ns-flow-content(n,c) ) | e-scalar ) )`
Spec prose (§7.5): "A complete flow node also has optional node properties, except for alias nodes which refer to the anchored node properties."
Verdict: Strict-conformant
Evidence: `event_iter/flow.rs:394-1675` (top-level dispatch in main loop covers alias, content forms, and property accumulation); empty-scalar-with-properties emission at `event_iter/flow.rs:723-741` when `,` arrives with pending properties.
Reasoning: Top-level node parser is the union of [159] and [160] dispatches, which themselves cover all sub-cases. Property handling is uniform across all content forms via the shared pending-tag/anchor mechanism. Strict-conformant by composition.
