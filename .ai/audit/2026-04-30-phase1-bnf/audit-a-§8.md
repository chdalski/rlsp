---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: A
section: §8
date: 2026-04-30
---

### [162] c-b-block-header(t)

BNF:
```
c-b-block-header(t) ::=
  (
      (
        c-indentation-indicator
        c-chomping-indicator(t)
      )
    | (
        c-chomping-indicator(t)
        c-indentation-indicator
      )
  )
  s-b-comment
```

Spec prose: §8.1.1 "[Block scalars] are controlled by a few [indicators] given in a _header_ preceding the [content] itself. This header is followed by a non-content [line break] with an optional [comment]. This is the only case where a [comment] must not be followed by additional [comment] lines."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:500-619` (`parse_block_header`), `block.rs:602-616` (post-indicator validation: only whitespace + optional comment permitted).

Reasoning: `parse_block_header` accepts the indicators in either order via the loop at 512-600 — `+`/`-` set chomp (543, 558) and `1..='9'` set explicit_indent (584); duplicates of either kind raise an error (533-541, 547-556, 574-582). After indicators, only optional whitespace followed by either a `#` comment or end-of-line is allowed (605-616), which realises `s-b-comment`. The "no additional comment lines" constraint is enforced because the lexer consumes only the header line (lines 64-67) and treats subsequent lines per the content-collection logic — comment lines after the header line are not consumed as part of the header.

### [163] c-indentation-indicator

BNF:
```
c-indentation-indicator ::=
  [x31-x39]    # 1-9
```

Spec prose: §8.1.1.1 "If a block scalar has an _indentation indicator_, then the content indentation level of the block scalar is equal to the indentation level of the block scalar plus the integer value of the indentation indicator character." "It is an error if any non-[empty line] does not begin with a number of spaces greater than or equal to the content indentation level. It is an error for any of the leading [empty lines] to contain more [spaces] than the first non-empty line."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:562-587` (digit handling: `'0'` rejected at 562-572; `'1'..='9'` accepted at 573-587; duplicate digit rejected at 574-582), `block.rs:204-223` (over-indented leading blank line rejected when content_indent_known), `block.rs:455-470` (folded-scalar over-indented blank rejected).

Reasoning: The accepted character class `'1'..='9'` matches `[x31-x39]` exactly. `'0'` is explicitly rejected with a dedicated error at 562-572. The runtime invariants ("non-empty line must begin with content_indent spaces" and "leading empty lines must not exceed content_indent") are enforced at 198-200 (lines below content_indent are not classified as content) and at 204-223 (folded counterpart at 455-470).

### [164] c-chomping-indicator(t)

BNF:
```
c-chomping-indicator(STRIP) ::= '-'
c-chomping-indicator(KEEP)  ::= '+'
c-chomping-indicator(CLIP)  ::= ""
```

Spec prose: §8.1.1.2 "_Stripping_ is specified by the '-' chomping indicator … _Keeping_ is specified by the '+' chomping indicator. _Clipping_ is the default behavior used if no explicit chomping indicator is specified."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:532-561` (chomp recognition), `block.rs:618` (default Clip when chomp is None).

Reasoning: `+` maps to `Chomp::Keep` (543), `-` to `Chomp::Strip` (558), and absence to `Chomp::Clip` via `chomp.unwrap_or(Chomp::Clip)` at 618. Duplicate detection prevents `++`/`--`/`+-`/`-+` (533-541, 547-556). The mapping to the three named chomp modes reproduces the BNF directly.

### [165] b-chomped-last(t)

BNF:
```
b-chomped-last(STRIP) ::= b-non-content  | <end-of-input>
b-chomped-last(CLIP)  ::= b-as-line-feed | <end-of-input>
b-chomped-last(KEEP)  ::= b-as-line-feed | <end-of-input>
```

Spec prose: §8.1.1.2 "The interpretation of the final [line break] of a [block scalar] is controlled by the chomping indicator specified in the [block scalar header]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:634-664` (`apply_chomping`), `block.rs:240-244` (literal: `\n` push deferred when `BreakType::Eof`), `block.rs:443-444` (folded: same EOF logic).

Reasoning: For Strip, `apply_chomping` pops the trailing `\n` (640-647) — equivalent to consuming `b-non-content` without preserving a line feed. For Clip and Keep, the `\n` from the last content line is preserved as line-feed (648-657 for Clip; the Keep arm at 658-661 keeps both the last `\n` and the trailing blanks). The `<end-of-input>` alternative is realized by tracking `BreakType::Eof` so no synthetic `\n` is appended for content that ended without a physical break (242-244 for literal; 444 for folded), and Clip then re-adds a trailing `\n` only when content is non-empty (654-656).

### [166] l-chomped-empty(n,t)

BNF:
```
l-chomped-empty(n,STRIP) ::= l-strip-empty(n)
l-chomped-empty(n,CLIP)  ::= l-strip-empty(n)
l-chomped-empty(n,KEEP)  ::= l-keep-empty(n)
```

Spec prose: §8.1.1.2 "The interpretation of the trailing [empty lines] following a [block scalar] is also controlled by the chomping indicator …"

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:639-661` (Strip and Clip discard `trailing_blank_count`; Keep retains it).

Reasoning: The Strip arm (640-647) and the Clip arm (648-657) both ignore `trailing_blank_count`, preserving at most a single trailing line feed (Strip: zero; Clip: one). Only the Keep arm (658-661) appends `repeat_n('\n', trailing_blank_count)`. This reproduces `l-chomped-empty` switching between `l-strip-empty` (Strip/Clip) and `l-keep-empty` (Keep).

### [167] l-strip-empty(n)

BNF:
```
l-strip-empty(n) ::=
  (
    s-indent-less-or-equal(n)
    b-non-content
  )*
  l-trail-comments(n)?
```

Spec prose: §8.1.1.2 (within Chomping discussion) — defines the strip behavior: trailing empty lines and the final line break are excluded from content.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:248-260` (literal blank-line classification), `block.rs:447-476` (folded blank-line classification — and over-indented blank rejection), `block.rs:639-657` (Strip and Clip discard trailing newlines).

Reasoning: Trailing whitespace-only lines whose indent <= content_indent are absorbed silently (literal 248-260, folded 447-476). Lines with non-whitespace content at indent < content_indent terminate the scalar (249-252, 449-451), which is the boundary `l-trail-comments(n)?` would consume; however the loop instead leaves those bytes unconsumed for the outer parser to classify as the next event. For Strip/Clip, `apply_chomping` (639-657) discards `trailing_blank_count`, matching the spec semantics that trailing empties are omitted under Strip and Clip.

### [168] l-keep-empty(n)

BNF:
```
l-keep-empty(n) ::=
  l-empty(n,BLOCK-IN)*
  l-trail-comments(n)?
```

Spec prose: §8.1.1.2 (Keeping) "_Keeping_ is specified by the '+' chomping indicator. In this case, the final [line break] and any trailing [empty lines] are considered to be part of the [scalar's content]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:658-661` (Keep arm appends `repeat_n('\n', trailing_blank_count)`), `block.rs:253-260` (literal counts blank lines), `block.rs:471-476` (folded counts blank lines).

Reasoning: Whitespace-only lines (at any indent <= content_indent) increment `trailing_newlines` (literal 259, folded 475). The Keep arm of `apply_chomping` appends one `\n` per counted blank to the value (660). Combined with the last content line's preserved `\n` (240-244, 482-484), this yields the keep semantics: final line break + N trailing empties.

### [169] l-trail-comments(n)

BNF:
```
l-trail-comments(n) ::=
  s-indent-less-than(n)
  c-nb-comment-text
  b-comment
  l-comment*
```

Spec prose: §8.1.1.2 "Explicit [comment] lines may follow the trailing [empty lines]. To prevent ambiguity, the first such [comment] line must be less [indented] than the [block scalar content]. Additional [comment] lines, if any, are not so restricted."

Verdict: Lenient

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:129-261` (literal main loop), `block.rs:365-476` (folded main loop) — neither performs any explicit recognition of trailing-comment lines that satisfy `s-indent-less-than(n)`; comments below content_indent are simply left for the outer parser.

Reasoning: The spec mandates that the *first* trailing comment line must be at indent < n (the content indentation level). The parser does not enforce this constraint: a comment at indent >= n that occurs after a blank line is not handled as part of the trail-comments production at all — when the literal/folded loop sees a non-whitespace line at indent < content_indent, it breaks (249-252, 449-451) regardless of whether the line is a comment. There is no check that an indented comment line at indent >= content_indent would be rejected as a trail-comment. The looser interpretation makes some inputs which the spec's trail-comments restriction would reject pass (the block scalar simply terminates and the comment is left to the outer parser), so the production is more permissive than its BNF specifies.

### [170] c-l+literal(n)

BNF:
```
c-l+literal(n) ::=
  c-literal                # '|'
  c-b-block-header(t)
  l-literal-content(n+m,t)
```

Spec prose: §8.1.2 "The _literal style_ is denoted by the '|' indicator. It is the simplest, most restricted and most readable [scalar style]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:41-271` (`try_consume_literal_block_scalar`): pipe detection at 47-50, header parse at 71-75, content collection at 117-261, span end at 268.

Reasoning: The function checks the leading `|` (47-50), parses the header to obtain (chomp, indent_indicator) (71-75), determines content_indent = parent_indent + indicator (or auto-detection per §8.1.1.1) at 95-115, then collects content at 117-261, finally applying chomping at 266. This composition mirrors the BNF: c-literal -> c-b-block-header -> l-literal-content. Sub-production verdicts are reconciled: c-b-block-header is Strict-conformant, l-literal-content is Strict-conformant. The composition itself is correct.

### [171] l-nb-literal-text(n)

BNF:
```
l-nb-literal-text(n) ::=
  l-empty(n,BLOCK-IN)*
  s-indent(n) nb-char+
```

Spec prose: §8.1.2 "Inside literal scalars, all ([indented]) characters are considered to be [content], including [white space] characters. … In addition, [empty lines] are not [folded], though final [line breaks] and trailing [empty lines] are [chomped]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:181-244` (content-line classification and emission); leading `l-empty(n,BLOCK-IN)*` is realized by 198-260 — blank lines preceding the first content line are tracked via `before_first_real_content` and `trailing_newlines`.

Reasoning: A line with indent >= content_indent and any non-empty after-indent content is classified as a content line (181-200). The after_indent slice (`line_content.get(content_indent..).unwrap_or("")`) is appended verbatim (238) — every byte of `nb-char+` after the s-indent prefix becomes content, including spaces and tabs. Leading empty lines are accumulated as newlines (245-260, 229) and flushed when the first content line is seen, matching `l-empty(n,BLOCK-IN)*` followed by `s-indent(n) nb-char+`. The tab-as-indentation error (134-145) bars `\t` from acting as part of `s-indent(n)`, consistent with the indent's space-only definition.

### [172] b-nb-literal-next(n)

BNF:
```
b-nb-literal-next(n) ::=
  b-as-line-feed
  l-nb-literal-text(n)
```

Spec prose: §8.1.2 (within Literal Style) — defines continuation line: a line break followed by another l-nb-literal-text(n) produces the next text line.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:240-244` (push `\n` per content line), `block.rs:228-238` (next iteration emits next text line).

Reasoning: Each content line in the main loop pushes the line text (238) and then a `\n` if the line had a physical break (240-244). On the next iteration, classification of the next content line yields the next `l-nb-literal-text(n)`. The push of `\n` is `b-as-line-feed`, and the subsequent text-line emission is `l-nb-literal-text(n)`. The composition reproduces the production.

### [173] l-literal-content(n,t)

BNF:
```
l-literal-content(n,t) ::=
  (
    l-nb-literal-text(n)
    b-nb-literal-next(n)*
    b-chomped-last(t)
  )?
  l-chomped-empty(n,t)
```

Spec prose: §8.1.2 — composition of literal text body, optional, followed by chomping handling.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:117-271` (content collection then chomping); empty-content branch is `out` remaining empty when no content lines exist; `apply_chomping` handles the empty case at 654-657 (Clip leaves "").

Reasoning: When no content line is found, `out` stays empty and `trailing_newlines` may be >0 (counting blank lines). `apply_chomping` then produces "" for Strip and Clip and `"\n" * trailing_blank_count` for Keep (640-661). When at least one content line exists, the loop produces `l-nb-literal-text(n)` followed by zero or more `b-nb-literal-next(n)`, and the `\n` after the last content line plays the role of `b-chomped-last(t)`. The trailing blanks count handles `l-chomped-empty(n,t)`. All sub-productions reconcile to Strict-conformant.

### [174] c-l+folded(n)

BNF:
```
c-l+folded(n) ::=
  c-folded                 # '>'
  c-b-block-header(t)
  l-folded-content(n+m,t)
```

Spec prose: §8.1.3 "The _folded style_ is denoted by the '>' indicator. It is similar to the [literal style]; however, folded scalars are subject to [line folding]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:285-345` (`try_consume_folded_block_scalar`): `>` detection at 289-293, header parse at 311-315, content collection at 337, chomp application at 342.

Reasoning: The function detects the leading `>` (289-293), parses the header at 311-315, computes content_indent by either explicit indicator + parent_indent or auto-detection (322-335), collects folded content via `collect_folded_lines` (337-341), then applies chomping (342). The composition reproduces the BNF directly. Sub-productions reconcile to Strict-conformant.

### [175] s-nb-folded-text(n)

BNF:
```
s-nb-folded-text(n) ::=
  s-indent(n)
  ns-char
  nb-char*
```

Spec prose: §8.1.3 "[Folding] allows long lines to be broken anywhere a single [space] character separates two non-[space] characters."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:396-446` (content-line classification in `collect_folded_lines`).

Reasoning: A folded content line is recognised when indent >= content_indent and `after_indent.trim_end_matches(' ')` is non-empty (396-407). The first character after the indent prefix must be ns-char (non-whitespace) to be a non-spaced folded line; if it begins with whitespace it is classified as `s-nb-spaced-text` (414-415). This mirrors `s-indent(n) ns-char nb-char*` — the indent prefix is stripped, the first non-blank character anchors the line as folded text, and the remainder is appended verbatim (442).

### [176] l-nb-folded-lines(n)

BNF:
```
l-nb-folded-lines(n) ::=
  s-nb-folded-text(n)
  (
    b-l-folded(n,BLOCK-IN)
    s-nb-folded-text(n)
  )*
```

Spec prose: §8.1.3 — folded text lines separated by `b-l-folded(n,BLOCK-IN)` produce a single space when both lines are non-spaced and separated by a single break.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:409-445` (folding decision at 416-435).

Reasoning: For consecutive folded (non-spaced) lines, a single inter-line break is replaced by a space (`out.push(' ')` at 428); N>0 blank lines collapse to N newlines (`out.extend(std::iter::repeat_n('\n', trailing_newlines + extra))` at 422). When either side is more-indented (spaced), the break is preserved as `\n` (425). This reproduces the spec rule for `b-l-folded(n,BLOCK-IN)` between two `s-nb-folded-text(n)` lines.

### [177] s-nb-spaced-text(n)

BNF:
```
s-nb-spaced-text(n) ::=
  s-indent(n)
  s-white
  nb-char*
```

Spec prose: §8.1.3 "Lines starting with [white space] characters (_more-indented_ lines) are not [folded]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:414-415` (more-indented detection: `next.indent > content_indent || after_indent.starts_with([' ', '\t'])`).

Reasoning: A line is classified as more-indented (`is_more_indented`) when either its indent exceeds content_indent (extra leading spaces beyond the content indent) or the first character after the content-indent prefix is whitespace (414-415). This is `s-indent(n) s-white nb-char*` — the s-white catches the whitespace (space or tab) right after the s-indent prefix.

### [178] b-l-spaced(n)

BNF:
```
b-l-spaced(n) ::=
  b-as-line-feed
  l-empty(n,BLOCK-IN)*
```

Spec prose: §8.1.3 — surrounding break of a spaced (more-indented) line is preserved as a line feed, not folded.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:417-425` (`prev_more_indented || is_more_indented` triggers `\n` rather than space).

Reasoning: When the previous content line was spaced or the current line is spaced, the inter-line break is preserved as `\n` (425). When N blank lines intervene and either neighbour is spaced, the count is `trailing_newlines + 1` newlines emitted (421-422), realising `b-as-line-feed` followed by `l-empty(n,BLOCK-IN)*`. The implementation matches the spec's "surrounding breaks are not folded" rule.

### [179] l-nb-spaced-lines(n)

BNF:
```
l-nb-spaced-lines(n) ::=
  s-nb-spaced-text(n)
  (
    b-l-spaced(n)
    s-nb-spaced-text(n)
  )*
```

Spec prose: §8.1.3 — sequence of more-indented text lines separated by preserved breaks.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:409-445` (folding decision treats spaced lines uniformly with the `prev_more_indented || is_more_indented` flag).

Reasoning: The same loop handles spaced lines: each more-indented line is appended verbatim (442), and the inter-line break between two spaced lines is preserved as `\n` (425). The composition reproduces the BNF — repeated `s-nb-spaced-text(n)` separated by `b-l-spaced(n)`.

### [180] l-nb-same-lines(n)

BNF:
```
l-nb-same-lines(n) ::=
  l-empty(n,BLOCK-IN)*
  (
      l-nb-folded-lines(n)
    | l-nb-spaced-lines(n)
  )
```

Spec prose: §8.1.3 — leading empty lines followed by either folded text or spaced text.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:430-435` (leading blank lines before first content emit literal newlines), `block.rs:447-476` (blank lines).

Reasoning: Before the first content line (`has_content == false`), each blank line increments `trailing_newlines` and is then flushed as N newlines once the first content line arrives (430-435). After the first content line, the loop dispatches each subsequent line to the folded or spaced rules. Either folded or spaced lines run as a homogenous block via `is_more_indented`; the implementation does not switch between them within `l-nb-same-lines` because the choice between `l-nb-folded-lines` and `l-nb-spaced-lines` is decided per the first content line's spaced-ness — this matches the alternation in the BNF (one of the two is chosen).

### [181] l-nb-diff-lines(n)

BNF:
```
l-nb-diff-lines(n) ::=
  l-nb-same-lines(n)
  (
    b-as-line-feed
    l-nb-same-lines(n)
  )*
```

Spec prose: §8.1.3 "[Line breaks] and [empty lines] separating folded and more-indented lines are also not [folded]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:417-435` (transition handling: when `prev_more_indented` differs from `is_more_indented`, the break is preserved as `\n`).

Reasoning: The folding decision at 423-425 only emits a space when both the previous and current content lines are non-spaced; any transition (folded -> spaced or spaced -> folded) preserves the break as `\n`. Combined with the blank-line accumulation (417-422), this realises `b-as-line-feed` separating consecutive `l-nb-same-lines(n)` blocks. The parser does not split content into discrete same-lines blocks; instead it produces the equivalent character stream by per-line decision, which yields the same string.

### [182] l-folded-content(n,t)

BNF:
```
l-folded-content(n,t) ::=
  (
    l-nb-diff-lines(n)
    b-chomped-last(t)
  )?
  l-chomped-empty(n,t)
```

Spec prose: §8.1.3 "The final [line break] and trailing [empty lines] if any, are subject to [chomping] and are never [folded]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/block.rs:337-344` (top-level folded body: `collect_folded_lines` -> `apply_chomping`), `block.rs:478-486` (final break appended; trailing blank count returned).

Reasoning: `collect_folded_lines` appends the final content line's `\n` (482-484) — this is `b-chomped-last(t)` for Clip/Keep semantics. Trailing blank lines are returned as `trailing_newlines` and consumed by `apply_chomping`. The empty-content case (no content lines) leaves `out` empty and emits only `l-chomped-empty(n,t)`. Sub-productions reconcile to Strict-conformant.

### [183] l+block-sequence(n)

BNF:
```
l+block-sequence(n) ::=
  (
    s-indent(n+1+m)
    c-l-block-seq-entry(n+1+m)
  )+
```

Spec prose: §8.2.1 "A _block sequence_ is simply a series of [nodes], each denoted by a leading '-' indicator. The '-' indicator must be [separated] from the [node] by [white space]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/sequence.rs:32-54` (`peek_sequence_entry` detects each `- ` entry), `sequence.rs:110-215` (`handle_sequence_entry` opens or continues a sequence), `sequence.rs:129-143` (indent rule: `dash_indent > col` to nest, `>=` only when seq-spaces or explicit-key contexts apply), `sequence.rs:167-175` (sibling-entry indent must equal parent).

Reasoning: The handler opens a new sequence when the dash indent is strictly greater than the surrounding collection's indent (133), implementing `n+1+m` (the `+1+m` is "any indent greater than parent"). Subsequent entries at the same column are siblings (167-175 enforces this). The `+` plurality is realised by the iterative dispatch — `step_in_document` re-enters this handler for each `- ` line. The auto-detected `m` is implicit in "any column > parent_col qualifies."

### [184] c-l-block-seq-entry(n)

BNF:
```
c-l-block-seq-entry(n) ::=
  c-sequence-entry    # '-'
  [ lookahead ≠ ns-char ]
  s-l+block-indented(n,BLOCK-IN)
```

Spec prose: §8.2.1 "The '-' indicator must be [separated] from the [node] by [white space]. This allows '-' to be used as the first character in a [plain scalar] if followed by a non-space character (e.g. '-42')."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/sequence.rs:40-45` (lookahead: after-dash must be empty/space/tab — equivalent to `≠ ns-char`), `sequence.rs:259-274` (tab-after-dash is rejected as block indentation), `sequence.rs:60-103` (`consume_sequence_dash` handles inline content and prepends synthetic line for `s-l+block-indented` follow-up).

Reasoning: The handler accepts `-` only when followed by space, tab, or end-of-line (41-42); a `-` followed by any other character (a non-space ns-char) returns None and the line falls through to plain-scalar parsing. After consumption, inline content is prepended as a synthetic line with column = dash + offset (89-95), which subsequent dispatch handles as `s-l+block-indented(n+1+m, BLOCK-IN)` content.

### [185] s-l+block-indented(n,c)

BNF:
```
s-l+block-indented(n,c) ::=
    (
      s-indent(m)
      (
          ns-l-compact-sequence(n+1+m)
        | ns-l-compact-mapping(n+1+m)
      )
    )
  | s-l+block-node(n,c)
  | (
      e-node    # ""
      s-l-comments
    )
```

Spec prose: §8.2.1 "The entry [node] may be either [completely empty], be a nested [block node] or use a _compact in-line notation_. The compact notation may be used when the entry is itself a nested [block collection]. … Note that it is not possible to specify [node properties] for such a [collection]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/sequence.rs:60-103` (`consume_sequence_dash` decides empty vs inline-prepend), `sequence.rs:453-477` (bare `-` with no inline and no further indented content emits `e-node`), `sequence.rs:78-97` (inline content -> synthetic line at column dash_indent + offset, handled on next iteration as either compact collection or block scalar/scalar).

Reasoning: The three alternatives map onto:
- **e-node alternative** — bare `-` with nothing inline and no deeper content emits an empty plain scalar (453-476).
- **s-l+block-node alternative** — bare `-` followed by an indented block node (`next_indent > dash_indent`, 459-460) lets the main step loop process the node normally.
- **compact alternative** — inline content after `- ` is prepended at column `dash_indent + offset` (89-95). When the inline content is `key: value` or another `- ` indicator, the next iteration recognises it as `ns-l-compact-mapping` or `ns-l-compact-sequence` at that column, which is `n+1+m`.

The spec's restriction that node properties cannot be specified for compact collections is mirrored: inline content prepended after `- ` cannot carry an outer-mapping anchor/tag because there is no separator between the dash and the inline that would consume properties at the dash's level.

### [186] ns-l-compact-sequence(n)

BNF:
```
ns-l-compact-sequence(n) ::=
  c-l-block-seq-entry(n)
  (
    s-indent(n)
    c-l-block-seq-entry(n)
  )*
```

Spec prose: §8.2.1 (compact form usable when sequence entry is itself a nested block collection).

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/sequence.rs:78-97` (synthetic inline at column `dash_indent + offset_from_dash`), `sequence.rs:129-143` (subsequent `- ` at the same column = sibling entry).

Reasoning: After `- - one` inline-prepend, the inner `- one` arrives on the next dispatch as a synthetic line at column `dash_indent + 2`. The sequence handler then treats `dash_indent_inner > current_top` as opening a new (inner) sequence (133), then siblings of that inner sequence appear at the same column `n+1+m_inner` and are matched by the `dash_indent != parent_col` check at 168. The composition `c-l-block-seq-entry(n) (s-indent(n) c-l-block-seq-entry(n))*` is realised by the iterative dispatch.

### [187] l+block-mapping(n)

BNF:
```
l+block-mapping(n) ::=
  (
    s-indent(n+1+m)
    ns-l-block-map-entry(n+1+m)
  )+
```

Spec prose: §8.2.2 "A _Block mapping_ is a series of entries, each [presenting] a [key/value pair]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/mapping.rs:34-70` (`peek_mapping_entry` detects each entry), `mapping.rs:371-786` (`handle_mapping_entry` opens a mapping when not already in one at the indent), `mapping.rs:431-435` (mapping at this indent recognised on subsequent entries).

Reasoning: A new mapping is opened when no `Mapping` exists at the current `effective_key_indent` on the stack (431-563). Subsequent entries at the same column are continued (565-786). The implicit `n+1+m` is auto-detected: any indent strictly greater than the parent collection's indent qualifies. The `+` plurality is realised by re-entry of the step loop for each subsequent key line.

### [188] ns-l-block-map-entry(n)

BNF:
```
ns-l-block-map-entry(n) ::=
    c-l-block-map-explicit-entry(n)
  | ns-l-block-map-implicit-entry(n)
```

Spec prose: §8.2.2 — alternative between explicit (`?`) and implicit forms.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/mapping.rs:52-67` (`peek_mapping_entry` detects `?` indicator vs implicit-key), `mapping.rs:115-149` (consume explicit key path), `mapping.rs:155-311` (consume implicit key path).

Reasoning: `peek_mapping_entry` returns Some when the line either starts with `?` followed by whitespace/EOL (52-60) or contains a value indicator (62-65). Consumption then dispatches to the explicit branch at 115-149 (`? key` returns `ConsumedMapping::ExplicitKey`) or the implicit branch at 155-311 (returns `ImplicitKey`). The two-way alternation matches the BNF.

### [189] c-l-block-map-explicit-entry(n)

BNF:
```
c-l-block-map-explicit-entry(n) ::=
  c-l-block-map-explicit-key(n)
  (
      l-block-map-explicit-value(n)
    | e-node                        # ""
  )
```

Spec prose: §8.2.2 — explicit-key entry composed of a `?` key followed by either an explicit `:` value or an empty value.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/mapping.rs:728-738` (`ConsumedMapping::ExplicitKey` sets `complex_key_inline` and `explicit_key_pending`), `mapping.rs:583-640` (`:` value indicator consumed when in Value phase or with `complex_key_inline`), `mapping.rs:648-668` (when next entry is another key without an intervening `:`, the previous explicit key is given an empty (e-node) value).

Reasoning: The explicit form proceeds through `ConsumedMapping::ExplicitKey` (728-738) which leaves the mapping in Key phase pending the value indicator. When `:` arrives at the same column, `consume_explicit_value_line` is invoked (638) — the explicit value alternative. When a new key arrives without the `:` (648), an empty scalar is emitted as the value (652-665) — the e-node alternative. Both BNF alternatives are realised.

### [190] c-l-block-map-explicit-key(n)

BNF:
```
c-l-block-map-explicit-key(n) ::=
  c-mapping-key                     # '?' (not followed by non-ws char)
  s-l+block-indented(n,BLOCK-OUT)
```

Spec prose: §8.2.2 "If the '?' indicator is specified, the optional value node must be specified on a separate line, denoted by the ':' indicator."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/mapping.rs:52-60` (`?` lookahead enforces non-ns-char), `mapping.rs:115-149` (consume explicit key), `mapping.rs:127-145` (inline content prepended as synthetic line at column `key_indent + 1 + spaces_after_q`).

Reasoning: `?` is only treated as the explicit-key indicator when followed by space, tab, or EOL (52-60); a `?foo` is not an explicit key. After consuming the indicator, inline content is either prepended as a synthetic line at column `key_indent + 1 + spaces_after_q` (127-145) — `s-indent(m')` of `s-l+block-indented(n,BLOCK-OUT)` — or no synthetic line is emitted when only a comment follows (146-148). The synthetic-line dispatch in subsequent iterations realises `s-l+block-indented(n,BLOCK-OUT)`.

### [191] l-block-map-explicit-value(n)

BNF:
```
l-block-map-explicit-value(n) ::=
  s-indent(n)
  c-mapping-value                   # ':' (not followed by non-ws char)
  s-l+block-indented(n,BLOCK-OUT)
```

Spec prose: §8.2.2 — explicit value follows a `:` indicator at indent n, with the same compact-or-block follow-up as explicit-key.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/mapping.rs:792-806` (`is_value_indicator_line` enforces `:` followed by ws/EOL — c-mapping-value rule), `mapping.rs:812-859` (`consume_explicit_value_line` advances and prepends inline content at column `key_indent + 1 + spaces_after_colon`).

Reasoning: The `:` is only recognised when followed by whitespace or EOL (799-805). On consume, inline value content is prepended at the appropriate column for `s-l+block-indented(n,BLOCK-OUT)` (832-849); when there is no inline, the next iteration handles the value as `s-l+block-indented` from a deeper-indented line, and an empty inline yields an e-node trigger via the Value-phase guard at 670-697.

### [192] ns-l-block-map-implicit-entry(n)

BNF:
```
ns-l-block-map-implicit-entry(n) ::=
  (
      ns-s-block-map-implicit-key
    | e-node    # ""
  )
  c-l-block-map-implicit-value(n)
```

Spec prose: §8.2.2 — implicit-key entry composed of either a non-empty key (plain or quoted) or an empty key, followed by a `: value` form.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/mapping.rs:62-65` (implicit detection via `is_implicit_mapping_line`), `mapping.rs:174` (`key_content` extracted; can be empty), `mapping.rs:739-761` (key emission), `mapping.rs:172-200` (value indicator consumption follows).

Reasoning: When a line matches `is_implicit_mapping_line`, `consume_mapping_entry` extracts `key_content = trimmed[..colon_offset].trim_end_matches([' ', '\t'])` (174). If the key portion is empty (e.g. `: value`), `key_content` is empty and an empty scalar is emitted as the key (739-760) — the e-node alternative. Otherwise the plain or quoted key text is emitted. The `c-l-block-map-implicit-value(n)` follows by the value-content prepending (296-303) or by the Value-phase value-emission path.

### [193] ns-s-block-map-implicit-key

BNF:
```
ns-s-block-map-implicit-key ::=
    c-s-implicit-json-key(BLOCK-KEY)
  | ns-s-implicit-yaml-key(BLOCK-KEY)
```

Spec prose: §8.2.2 "If the '?' indicator is omitted, [parsing] needs to see past the [implicit key], in the same way as in the [single key/value pair] [flow mapping]. Hence, such [keys] are subject to the same restrictions; they are limited to a single line and must not span more than 1024 Unicode characters."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/mapping.rs:161-172` (1024-char limit enforced before key emission), `mapping.rs:200-269` (quoted-key path uses `try_consume_single_quoted` / `try_consume_double_quoted`; non-quoted keys are emitted as Plain).

Reasoning: The 1024-char limit on the implicit key is enforced by counting chars in `trimmed[..colon_offset]` and yielding `ImplicitKeyTooLongError` at 161-172. Quoted keys are decoded via the same flow-context machinery (235, 251), satisfying `c-s-implicit-json-key(BLOCK-KEY)`. Plain/non-quoted keys are emitted as `Plain` style (267-269), satisfying `ns-s-implicit-yaml-key(BLOCK-KEY)`. Single-line restriction is enforced because the parser only inspects a single physical `Line` — multi-line implicit keys are impossible by construction.

### [194] c-l-block-map-implicit-value(n)

BNF:
```
c-l-block-map-implicit-value(n) ::=
  c-mapping-value           # ':' (not followed by non-ws char)
  (
      s-l+block-node(n,BLOCK-OUT)
    | (
        e-node    # ""
        s-l-comments
      )
  )
```

Spec prose: §8.2.2 "the [value] may be specified on the same line as the [implicit key]. Note however that in block mappings the [value] must never be adjacent to the ':', as this greatly reduces readability and is not required for [JSON compatibility] (unlike the case in [flow mappings])."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/mapping.rs:271-304` (inline value content -> synthetic line; `: -` and `: 'b': c` rejected as inline block forms), `mapping.rs:670-697` (Value-phase + new key triggers empty scalar emission for the previous key's value).

Reasoning: The `:` followed by ws/EOL is enforced by `find_value_indicator_offset` and `is_value_indicator_line`. Inline value content is prepended at the value's column (296-303), yielding `s-l+block-node(n,BLOCK-OUT)` on the next dispatch. When no inline value follows and the next entry is a new key, the previous value is emitted as e-node (672-697). The "must never be adjacent" rule is enforced by `find_value_indicator_offset` requiring `:` to be followed by ws/EOL — `key:value` (no space) is not recognised as a mapping entry.

### [195] ns-l-compact-mapping(n)

BNF:
```
ns-l-compact-mapping(n) ::=
  ns-l-block-map-entry(n)
  (
    s-indent(n)
    ns-l-block-map-entry(n)
  )*
```

Spec prose: §8.2.2 "A [compact in-line notation] is also available. This compact notation may be nested inside [block sequences] and explicit block mapping entries. Note that it is not possible to specify [node properties] for such a nested mapping."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/sequence.rs:78-97` (inline mapping entry prepended at `dash_indent + offset_from_dash`), `mapping.rs:127-145` (inline explicit-key content prepended at `key_indent + 1 + spaces_after_q`), `mapping.rs:431-563` (mapping opens at the synthetic column).

Reasoning: A `- key: value` line prepends `key: value` at column `dash_indent + offset` and the next iteration processes it as an implicit mapping entry at that column (n+1+m). Subsequent entries at the same column are siblings of the compact mapping. The same mechanism applies to `? key: value` for explicit-key entries. The composition reproduces `ns-l-block-map-entry(n) (s-indent(n) ns-l-block-map-entry(n))*` via iterative dispatch.

### [196] s-l+block-node(n,c)

BNF:
```
s-l+block-node(n,c) ::=
    s-l+block-in-block(n,c)
  | s-l+flow-in-block(n)
```

Spec prose: §8.2.3 "YAML allows [flow nodes] to be embedded inside [block collections] (but not vice-versa). [Flow nodes] must be [indented] by at least one more [space] than the parent [block collection]. Note that [flow nodes] may begin on a following line."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/step.rs:282-1049` (top-level dispatch in `step_in_document` covers both alternatives), `step.rs:297-298` (flow-collection dispatch — flow-in-block), `step.rs:287-293` (block-sequence — block-in-block), `step.rs:886-887` (block-mapping — block-in-block), `step.rs:1007` (scalar dispatch — covers both block scalars and plain scalars / flow scalars).

Reasoning: The dispatcher composes all five top-level alternatives:
- `[`, `{` -> flow collections (`s-l+flow-in-block`)
- `'`, `"` -> flow scalars (`s-l+flow-in-block`)
- `|`, `>` -> block scalars (`s-l+block-scalar`, part of `s-l+block-in-block`)
- `-` (separated) -> block sequence (`s-l+block-collection`, part of `s-l+block-in-block`)
- mapping-entry detection -> block mapping (`s-l+block-collection`, part of `s-l+block-in-block`)
The composition correctly covers both branches of the BNF.

### [197] s-l+flow-in-block(n)

BNF:
```
s-l+flow-in-block(n) ::=
  s-separate(n+1,FLOW-OUT)
  ns-flow-node(n+1,FLOW-OUT)
  s-l-comments
```

Spec prose: §8.2.3 "[Flow nodes] must be [indented] by at least one more [space] than the parent [block collection]."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/step.rs:297-298` (flow-collection branch), `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs` (full flow handler, audited in §7), `step.rs:1007` (flow scalars also dispatched via `try_consume_scalar`).

Reasoning: Flow-in-block dispatch occurs when the current line begins with `[`, `{`, `'`, or `"` after the leading separator (286-298 and 1007). The separator (s-separate) is the leading whitespace already trimmed by the lexer's line classification; the flow node parser handles indent/comments per the FLOW-OUT context. `s-l-comments` is realised by the `drain_trailing_comment` call after the scalar/collection emits (1011). Sub-productions reconcile to Strict-conformant.

### [198] s-l+block-in-block(n,c)

BNF:
```
s-l+block-in-block(n,c) ::=
    s-l+block-scalar(n,c)
  | s-l+block-collection(n,c)
```

Spec prose: §8.2.3 — alternative between block scalar and block collection.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/base.rs:307-356` (`|` and `>` branches in `try_consume_scalar` -> block scalar), `/workspace/rlsp-yaml-parser/src/event_iter/step.rs:287-289, 886-887` (block-sequence and block-mapping dispatch -> block collection).

Reasoning: The dispatcher routes `|`/`>` to `try_consume_literal_block_scalar`/`try_consume_folded_block_scalar` (block-scalar alternative), and `-`/mapping-key lines to `handle_sequence_entry`/`handle_mapping_entry` (block-collection alternative). The two-way alternation is correctly composed.

### [199] s-l+block-scalar(n,c)

BNF:
```
s-l+block-scalar(n,c) ::=
  s-separate(n+1,c)
  (
    c-ns-properties(n+1,c)
    s-separate(n+1,c)
  )?
  (
      c-l+literal(n)
    | c-l+folded(n)
  )
```

Spec prose: §8.2.3 "The block [node's properties] may span across several lines. In this case, they must be [indented] by at least one more [space] than the [block collection], regardless of the [indentation] of the [block collection] entries."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/base.rs:307-356` (block-scalar dispatch with `pa = self.pending_anchor.take()` at 315, `pt = self.pending_tag.take()` at 316 — properties consumed as the scalar emits), `/workspace/rlsp-yaml-parser/src/event_iter/properties.rs` (anchor/tag scanning, audited in §6 for c-ns-properties), `/workspace/rlsp-yaml-parser/src/event_iter/step.rs:1007` (dispatcher).

Reasoning: When `|` or `>` is encountered, the pending anchor/tag captured from earlier lines (or the same line) is consumed and attached to the emitted scalar event (315-330, 340-355). The optional `c-ns-properties(n+1,c) s-separate(n+1,c)` is realised by the property-pending machinery — properties accumulate before the scalar arrives. The literal-or-folded alternation is the `|`/`>` branch dispatch. Sub-productions (c-ns-properties, c-l+literal, c-l+folded) reconcile to Strict-conformant.

### [200] s-l+block-collection(n,c)

BNF:
```
s-l+block-collection(n,c) ::=
  (
    s-separate(n+1,c)
    c-ns-properties(n+1,c)
  )?
  s-l-comments
  (
      seq-space(n,c)
    | l+block-mapping(n)
  )
```

Spec prose: §8.2.3 "Since people perceive the '-' indicator as [indentation], nested [block sequences] may be [indented] by one less [space] to compensate, except, of course, if nested inside another [block sequence] ([`BLOCK-OUT` context] versus [`BLOCK-IN` context])."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/sequence.rs:191-214` (anchor/tag consumed when sequence opens — block-collection properties), `/workspace/rlsp-yaml-parser/src/event_iter/block/mapping.rs:531-562` (mapping anchor/tag consumed when mapping opens), `mapping.rs:531-540` (Standalone-anchor resolution distinguishes collection-attached vs key-attached properties).

Reasoning: Pending properties are consumed when the collection's start event is emitted: `SequenceStart` at sequence.rs:208-213 includes `seq_anchor` / `seq_tag`; `MappingStart` at mapping.rs:551-562 includes `mapping_anchor` / `mapping_tag`. The optional properties branch is realised by `pending_collection_anchor` / `pending_anchor` Standalone vs Inline distinction (mapping.rs:531-540), where Standalone properties on a separate line bind to the collection. The `seq-space(n,c) | l+block-mapping(n)` choice is the dash-vs-mapping-key dispatch in step.rs.

### [201] seq-space(n,c)

BNF:
```
seq-space(n,BLOCK-OUT) ::= l+block-sequence(n-1)
seq-space(n,BLOCK-IN)  ::= l+block-sequence(n)
```

Spec prose: §8.2.3 "Since people perceive the '-' indicator as [indentation], nested [block sequences] may be [indented] by one less [space] to compensate, except, of course, if nested inside another [block sequence] ([`BLOCK-OUT` context] versus [`BLOCK-IN` context])."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/block/sequence.rs:120-143` (seq-spaces handling: the `Mapping` Value-phase branch at 142 allows `dash_indent >= col`, which is `n-1+1 = n` — the BLOCK-OUT relaxation; the `Sequence` branch at 131 requires `dash_indent > col`, which is the BLOCK-IN restriction).

Reasoning: When the parent on the stack is a Sequence, opening a nested sequence requires `dash_indent > col` (131) — the BLOCK-IN case (no compensation). When the parent is a Mapping in Value phase, `dash_indent >= col` (142) — the BLOCK-OUT case (one-less-indent compensation). This exactly corresponds to the two BNF arms: BLOCK-OUT yields `l+block-sequence(n-1)` (one less indent), BLOCK-IN yields `l+block-sequence(n)` (full indent). The closing logic at mapping.rs:401-429 mirrors the seq-spaces close when a same-indent key arrives.
