---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: A
section: §7
date: 2026-04-30
---

### [104] c-ns-alias-node

BNF:
```
c-ns-alias-node ::=
  c-alias           # '*'
  ns-anchor-name
```

Spec prose: §7.1 — "An alias node is denoted by the `*` indicator. The alias refers to the most recent preceding node having the same anchor. ... Note that an alias node must not specify any properties or content, as these were already specified at the first occurrence of the node."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/step.rs:314-455`, `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1355-1423`, `/workspace/rlsp-yaml-parser/src/event_iter/properties.rs:23-45`.

Reasoning: Both block-context (`step.rs:314`) and flow-context (`flow.rs:1355`) handlers detect the `*` indicator, then dispatch to `scan_anchor_name` to extract the `ns-anchor-name` per [102]. The "no properties" rule is enforced explicitly: a pending tag yields "alias node cannot have a tag property" (`step.rs:328-334`, `flow.rs:1359-1365`); an inline pending anchor yields "alias node cannot have an anchor property" (`step.rs:338-344`, `flow.rs:1366-1372`). The emitted `Event::Alias { name }` carries no content, matching the spec's prohibition. The `ns-anchor-name` constituent is governed by [102]/[103] (audited under §6); leniency in `is_ns_anchor_char` (`chars.rs:149-159`) — which inherits the §5 leniency on `ns-char` allowing literal non-printables — lives at those productions, not here.

### [105] e-scalar

BNF:
```
e-scalar ::= ""
```

Spec prose: §7.2 — "YAML allows the node content to be omitted in many cases. Nodes with empty content are interpreted as if they were plain scalars with an empty value."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter.rs` via `empty_scalar_event()`; emitted from `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:512-518` (empty key/value at `}`), `flow.rs:728-741` (comma-terminated empty), `flow.rs:1183-1198` (empty key on `:`), `flow.rs:1224-1226` (empty key in sequence).

Reasoning: The parser models the empty scalar as a `Cow::Borrowed("")` plain-style scalar with `Plain` style — a zero-length string emitted at the appropriate position. Every site that needs `e-scalar` (empty key with `?` indicator unfollowed by content, value-omission after `:`, key-omission in `:value`, trailing `?` in `{...}`) emits this node. The constructor `empty_scalar_event()` uses `value: Cow::Borrowed("")` which is exactly the spec's `""` definition.

### [106] e-node

BNF:
```
e-node ::=
  e-scalar    # ""
```

Spec prose: §7.2 — "Both the node's properties and node content are optional. This allows for a completely empty node. Completely empty nodes are only valid when following some explicit indication for their existence."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:511-513` (null key + null value pair on `{?}`), `flow.rs:1224-1227` (empty key in sequence single-pair), `flow.rs:514-518` (empty value).

Reasoning: The parser emits a single empty plain scalar event for `e-node`, matching the BNF reduction `e-node ::= e-scalar`. The "explicit indication" precondition is satisfied by the call sites: a `?` explicit-key indicator that reaches `}` with no key content (`flow.rs:511`), a `:` value-separator with no following value (`flow.rs:514`), or a comma in Value phase (`flow.rs:709-741`). Each emission is gated by a syntactic indicator, not by silent fabrication.

### [107] nb-double-char

BNF:
```
nb-double-char ::=
    c-ns-esc-char
  | (
        nb-json
      - c-escape          # '\'
      - c-double-quote    # '"'
    )
```

Spec prose: §7.3.1 — "The double-quoted style is specified by surrounding `\"` indicators. This is the only style capable of expressing arbitrary strings, by using `\\` escape sequences. This comes at the cost of having to escape the `\\` and `\"` characters."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:618-751` (`scan_double_quoted_line`), `quoted.rs:633` (memchr2 on `"` and `\\`), `chars.rs:173-199` (`decode_escape`).

Reasoning: `scan_double_quoted_line` uses `memchr2(b'"', b'\\', ...)` to locate the next `"` or `\\` boundary; everything between successive boundaries is accumulated as `nb-json - c-escape - c-double-quote`, while a `\\` triggers `decode_and_push_escape` which consumes the escape body via `decode_escape`. The two BNF alternatives are exactly the two paths through the loop. Whether literal non-`nb-json` codepoints (e.g. raw control characters, surrogate-paired text) reach this scanner depends on upstream `c-printable`/`nb-json` gating; the §5 audit attributed lenient acceptance of literal non-printables to the underlying character predicates, so this composing production is not the locus of leniency.

### [108] ns-double-char

BNF:
```
ns-double-char ::=
  nb-double-char - s-white
```

Spec prose: §7.3.1 (continuation of double-quoted style restriction).

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:716-727` (trailing whitespace trimming), `quoted.rs:733-745` (trim trailing literal `' '` / `'\t'` from continuation lines).

Reasoning: `ns-double-char` differs from `nb-double-char` only in excluding `s-white`. The parser does not need a standalone `ns-double-char` predicate because all spec sites that reference it (`s-double-next-line`, `nb-ns-double-in-line`) are implemented through trim-then-scan: leading whitespace is stripped before line content is examined, and trailing whitespace is stripped (`quoted.rs:738`) before fold concatenation. The white-space exclusion is enforced positionally, equivalent to the BNF subtraction.

### [109] c-double-quoted(n,c)

BNF:
```
c-double-quoted(n,c) ::=
  c-double-quote         # '"'
  nb-double-text(n,c)
  c-double-quote         # '"'
```

Spec prose: §7.3.1 — c-double-quoted wraps double-quoted text between matching `"` indicators.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:178-242` (`try_consume_double_quoted`), `quoted.rs:186` (open `"` check), `quoted.rs:654-668` (close `"` detection).

Reasoning: `try_consume_double_quoted` requires the trimmed content to start with `"` and consumes the opening byte at line 207. The body is delegated to `scan_double_quoted_line`, which terminates on a matching `"` (line 654) and sets `DoubleQuotedLine::Closed { close_pos, tail }`. EOF before a closing quote raises "unterminated double-quoted scalar" (`quoted.rs:259-264`). The three BNF symbols (`"`, body, `"`) map one-to-one to opener detection, body delegation, and closer detection.

### [110] nb-double-text(n,c)

BNF:
```
nb-double-text(n,FLOW-OUT)  ::= nb-double-multi-line(n)
nb-double-text(n,FLOW-IN)   ::= nb-double-multi-line(n)
nb-double-text(n,BLOCK-KEY) ::= nb-double-one-line
nb-double-text(n,FLOW-KEY)  ::= nb-double-one-line
```

Spec prose: §7.3.1 — "Double-quoted scalars are restricted to a single line when contained inside an implicit key."

Verdict: Indeterminate

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:178-242` (entry), `quoted.rs:249-352` (continuation collector), `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:884-922` (flow-context dispatch), `flow.rs:1136-1161` (1024-char implicit-key check).

Reasoning: The single-line restriction in `BLOCK-KEY` / `FLOW-KEY` contexts is not enforced by passing a context parameter into the double-quoted lexer — `try_consume_double_quoted` always falls through to `collect_double_quoted_continuations` regardless of key position. The flow parser's 1024-character implicit-key cap (§7.4.3) and the multi-line key check at `flow.rs:1128-1134` partially gate the BLOCK-KEY/FLOW-KEY restriction by rejecting `:` separators on a different physical line than the preceding key. Whether every multi-line double-quoted scalar used as an implicit key is rejected — versus accepted as multi-line content — depends on whether `last_token_line` tracking matches the key's last line. Without a dedicated property test or conformance-suite cross-reference, the verdict between Strict-conformant and Lenient cannot be reached on code reading alone.

### [111] nb-double-one-line

BNF:
```
nb-double-one-line ::=
  nb-double-char*
```

Spec prose: §7.3.1 (single-line variant of nb-double-text).

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:618-751` (`scan_double_quoted_line` operates on a single line slice).

Reasoning: `scan_double_quoted_line` consumes a body slice (one physical line's worth) and walks it character-by-character via `memchr2` on `"` / `\\`. The repetition `nb-double-char*` corresponds to the loop at line 633, and a closing `"` exits the loop while end-of-slice (no further `"` or `\\` found) returns `Incomplete`. Composition matches the BNF zero-or-more repetition.

### [112] s-double-escaped(n)

BNF:
```
s-double-escaped(n) ::=
  s-white*
  c-escape         # '\'
  b-non-content
  l-empty(n,FLOW-IN)*
  s-flow-line-prefix(n)
```

Spec prose: §7.3.1 — "It is also possible to escape the line break character. In this case, the escaped line break is excluded from the content and any trailing white space characters that precede the escaped line break are preserved."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:676-689` (line-continuation `\<EOL>`), `quoted.rs:308-316` (suppression of fold separator on `line_continuation`), `quoted.rs:677-687` (preserves `prefix` content including trailing `\\t`/`' '`).

Reasoning: When the scanner encounters `\\` at end of line (`after_backslash.is_empty()` at line 676), it returns `Incomplete { line_continuation: true }`, and the `prefix` (line 684) is the body up to but not including the `\\` — preserving any trailing literal whitespace per the spec. The continuation collector then suppresses both the fold space (`quoted.rs:308-316`) and any leading whitespace on the next line (already stripped into `trimmed` at line 276, discarded if `line_continuation`). The `l-empty(n,FLOW-IN)*` repetition is realized by the blank-line counter (`pending_blanks`); however with `line_continuation = true` the blank-line emission path is bypassed, matching the spec's "excluded from content" semantics.

### [113] s-double-break(n)

BNF:
```
s-double-break(n) ::=
    s-double-escaped(n)
  | s-flow-folded(n)
```

Spec prose: §7.3.1 (alternation of escaped vs. natural line break in double-quoted text).

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:307-317` (alternation of suppressed vs. fold semantics), `quoted.rs:676-689` (escape branch), `quoted.rs:308-315` (natural-break branch).

Reasoning: The parser's continuation loop branches on `line_continuation`: when true the break and following whitespace are dropped (the `s-double-escaped` arm), when false the fold logic runs — single break → space, blank lines → newlines (the `s-flow-folded` arm). The two BNF alternatives map directly to these two branches.

### [114] nb-ns-double-in-line

BNF:
```
nb-ns-double-in-line ::=
  (
    s-white*
    ns-double-char
  )*
```

Spec prose: §7.3.1 — "All leading and trailing white space characters on each line are excluded from the content. Each continuation line must therefore contain at least one non-space character."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:276` (trim leading whitespace on continuation), `quoted.rs:733-744` (trim trailing literal whitespace via `owned_non_ws_len`).

Reasoning: Continuation lines apply `trim_start_matches([' ', '\t'])` (line 276) and trailing-whitespace truncation (line 738 borrow path; line 742 owned path) so that interior `s-white* ns-double-char` runs are preserved while leading/trailing whitespace is excluded. The "at least one non-space character" prerequisite is partially enforced by routing all-whitespace lines into `pending_blanks` instead of treating them as `nb-ns-double-in-line` content (`quoted.rs:293-302`). Implementation matches the BNF semantics.

### [115] s-double-next-line(n)

BNF:
```
s-double-next-line(n) ::=
  s-double-break(n)
  (
    ns-double-char nb-ns-double-in-line
    (
        s-double-next-line(n)
      | s-white*
    )
  )?
```

Spec prose: §7.3.1 — recursive definition of subsequent lines in a multi-line double-quoted scalar.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:249-352` (`collect_double_quoted_continuations` loop), `quoted.rs:293-301` (blank line accumulation), `quoted.rs:319-339` (non-blank continuation).

Reasoning: The loop iterates one continuation line per turn, replacing the recursive BNF with explicit iteration. A break is consumed (the previous line's terminator), the line is classified (blank → counter; non-blank → fold + scan). The `(... s-double-next-line | s-white*)?` optional tail is realized by the loop continuation versus return. The functional equivalence to the recursive BNF is preserved while avoiding stack-depth concerns.

### [116] nb-double-multi-line(n)

BNF:
```
nb-double-multi-line(n) ::=
  nb-ns-double-in-line
  (
      s-double-next-line(n)
    | s-white*
  )
```

Spec prose: §7.3.1 — first line of multi-line double-quoted text plus optional continuation.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:178-242` (first-line scan + optional continuation), `quoted.rs:226-239` (`Incomplete` branch dispatches to `collect_double_quoted_continuations`).

Reasoning: The first call to `scan_double_quoted_line` consumes the equivalent of `nb-ns-double-in-line` on the opening line. If the line is `Closed`, no continuation runs (the BNF's `s-white*` empty alternative). If `Incomplete`, `collect_double_quoted_continuations` runs (the BNF's `s-double-next-line` alternative). The two-phase structure matches the BNF.

### [117] c-quoted-quote

BNF:
```
c-quoted-quote ::= "''"
```

Spec prose: §7.3.2 — "within a single-quoted scalar, such characters need to be repeated. This is the only form of escaping performed in single-quoted scalars."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:415-455` (`scan_single_quoted_line`), `quoted.rs:428-432` (`''` escape branch), `quoted.rs:461-481` (`unescape_single_quoted` produces a single `'`).

Reasoning: `scan_single_quoted_line` peeks one byte after each `'` (line 428): two consecutive `'` bytes are treated as the escape and `has_escape` is set; a single `'` terminates the scalar. The unescape function emits a single `'` per `''` pair. The two-byte literal `''` of the BNF is recognized exactly.

### [118] nb-single-char

BNF:
```
nb-single-char ::=
    c-quoted-quote
  | (
        nb-json
      - c-single-quote    # "'"
    )
```

Spec prose: §7.3.2 — single-quoted character set: any non-line-break printable except a bare `'`.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:415-455` (`scan_single_quoted_line` walks bytes, treating any non-`'` byte as content; `''` as escape).

Reasoning: The scanner uses `memchr(b'\'', ...)` to find the next `'` and accepts everything in between as content (the `nb-json - c-single-quote` branch). Two adjacent `'`s become a `c-quoted-quote`. The two BNF alternatives map to the two scanner branches at line 428-444. The `nb-json` constraint (codepoint range) is not enforced at this scanner — leniency for literal non-printable characters lives at the §5 character predicates ([1]/[27]/[34]).

### [119] ns-single-char

BNF:
```
ns-single-char ::=
  nb-single-char - s-white
```

Spec prose: §7.3.2 — same as ns-double-char: single-quoted body excluding white space.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:74-78` (post-scan trailing whitespace trim on first line), `quoted.rs:107` (leading whitespace trim on continuation), `quoted.rs:152` (continuation trailing whitespace trim).

Reasoning: As with `ns-double-char`, the parser does not need a separate `ns-single-char` predicate; the white-space exclusion is enforced positionally — leading whitespace is trimmed before line scanning begins, and trailing whitespace is trimmed after scanning. The BNF subtraction is realized by these trim operations rather than a per-character test.

### [120] c-single-quoted(n,c)

BNF:
```
c-single-quoted(n,c) ::=
  c-single-quote    # "'"
  nb-single-text(n,c)
  c-single-quote    # "'"
```

Spec prose: §7.3.2 — single-quoted text between matching `'` indicators.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:27-154` (`try_consume_single_quoted`), `quoted.rs:35` (open `'` check), `quoted.rs:60-72` (close `'` on first line), `quoted.rs:122-145` (close `'` on continuation).

Reasoning: The opener is verified at line 35 (`content.starts_with('\'')`) and consumed at line 50. The body is processed by `scan_single_quoted_line` and the continuation loop; a non-escape `'` is the closer. EOF without a closing `'` produces "unterminated single-quoted scalar" (line 82-85). The opener-body-closer triple matches the BNF.

### [121] nb-single-text(n,c)

BNF:
```
nb-single-text(FLOW-OUT)  ::= nb-single-multi-line(n)
nb-single-text(FLOW-IN)   ::= nb-single-multi-line(n)
nb-single-text(BLOCK-KEY) ::= nb-single-one-line
nb-single-text(FLOW-KEY)  ::= nb-single-one-line
```

Spec prose: §7.3.2 — "Single-quoted scalars are restricted to a single line when contained inside an implicit key."

Verdict: Indeterminate

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:27-154` (entry has no key-context parameter), `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1128-1134` (multi-line implicit key error in flow sequences).

Reasoning: The parser does not pass a context discriminator into `try_consume_single_quoted`; the lexer always allows multi-line continuation. The `BLOCK-KEY` / `FLOW-KEY` single-line restriction is enforced indirectly via flow-parser checks (the multi-line implicit key error at `flow.rs:1128`) and the 1024-char limit at `flow.rs:1136-1161`. Whether every multi-line single-quoted scalar used as an implicit key is rejected — versus silently accepted as continuation — is not determinable from the code alone without conformance evidence; verdict is Indeterminate.

### [122] nb-single-one-line

BNF:
```
nb-single-one-line ::=
  nb-single-char*
```

Spec prose: §7.3.2 (single-line variant of nb-single-text).

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:415-455` (`scan_single_quoted_line` on a single body slice).

Reasoning: A single call to `scan_single_quoted_line` produces the equivalent of `nb-single-char*` over one physical line; the closing `'` on the same line corresponds to the boundary after the BNF's repetition.

### [123] nb-ns-single-in-line

BNF:
```
nb-ns-single-in-line ::=
  (
    s-white*
    ns-single-char
  )*
```

Spec prose: §7.3.2 — interior of a continuation line in a multi-line single-quoted scalar.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:107` (leading whitespace trim), `quoted.rs:147-152` (non-closing continuation, trailing whitespace trim).

Reasoning: Continuation lines have leading whitespace stripped at line 107 (`trim_start_matches([' ', '\t'])`) and trailing whitespace stripped at line 152 (`trim_end_matches([' ', '\t'])`). Interior whitespace runs are preserved as part of the content. The `s-white* ns-single-char` repetition reduces to "trim the line's borders, keep the middle".

### [124] s-single-next-line(n)

BNF:
```
s-single-next-line(n) ::=
  s-flow-folded(n)
  (
    ns-single-char
    nb-ns-single-in-line
    (
        s-single-next-line(n)
      | s-white*
    )
  )?
```

Spec prose: §7.3.2 — recursive definition of continuation lines.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:79-153` (continuation loop), `quoted.rs:109-113` (blank line handling), `quoted.rs:117-122` (fold space insertion).

Reasoning: The continuation loop replaces the recursive BNF: each iteration consumes one line, and the `s-flow-folded(n)` semantics (blank lines → newlines; single break → space) are realized at lines 109-119. The optional non-empty tail in the BNF corresponds to a non-blank `trimmed` continuation; the iterative loop terminates when a `Closed` line is found.

### [125] nb-single-multi-line(n)

BNF:
```
nb-single-multi-line(n) ::=
  nb-ns-single-in-line
  (
      s-single-next-line(n)
    | s-white*
  )
```

Spec prose: §7.3.2 — first line of multi-line single-quoted text plus optional continuation.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/quoted.rs:59-77` (first-line scan and one-line return path), `quoted.rs:79-153` (multi-line dispatch).

Reasoning: The first call to `scan_single_quoted_line` covers `nb-ns-single-in-line`; if it returns `closed`, no continuation runs (the empty `s-white*` alternative). Otherwise the continuation loop satisfies `s-single-next-line(n)`. Same structure as [116] with single-quoted-specific scanning.

### [126] ns-plain-first(c)

BNF:
```
ns-plain-first(c) ::=
    (
        ns-char
      - c-indicator
    )
  | (
      (
          c-mapping-key       # '?'
        | c-mapping-value     # ':'
        | c-sequence-entry    # '-'
      )
      [ lookahead = ns-plain-safe(c) ]
    )
```

Spec prose: §7.3.3 — "Plain scalars must not begin with most indicators... However, the `?`, `:` and `-` indicators may be used as the first character if followed by a non-space `safe` character, as this causes no ambiguity."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/plain.rs:287-302` (`ns_plain_first_block`), `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1536-1562` (flow-context plain-first dispatch).

Reasoning: `ns_plain_first_block` rejects all `c-indicator` characters except `?`, `:`, `-` (lines 290-298), which are admitted only when the lookahead character satisfies `ns_plain_safe_block`. The non-indicator branch (line 301) accepts any `is_ns_char(ch)`. The flow handler at `flow.rs:1536-1562` mirrors the same structure but uses a flow-context safety predicate that excludes `,[]{}` after the lookahead. The two BNF alternatives are mapped one-to-one. Note: `ns_plain_safe_block` for the block-context lookahead (`plain.rs:307-309`) does NOT exclude flow indicators — but in block context the spec says `ns-plain-safe(BLOCK-KEY) = ns-plain-safe-out = ns-char`, so flow indicators ARE permitted in block context. The lookahead in both flow and block contexts matches the spec parameterization.

### [127] ns-plain-safe(c)

BNF:
```
ns-plain-safe(FLOW-OUT)  ::= ns-plain-safe-out
ns-plain-safe(FLOW-IN)   ::= ns-plain-safe-in
ns-plain-safe(BLOCK-KEY) ::= ns-plain-safe-out
ns-plain-safe(FLOW-KEY)  ::= ns-plain-safe-in
```

Spec prose: §7.3.3 — context-driven dispatch to the in/out variants.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/plain.rs:340-416` (`scan_plain_line_block` is the FLOW-OUT/BLOCK-KEY scanner), `plain.rs:431-502` (`scan_plain_line_flow` is the FLOW-IN/FLOW-KEY scanner). Dispatch by context happens at the call site: `event_iter/flow.rs:1480` (flow context) and `event_iter/step.rs` block-context plain scanning.

Reasoning: Two distinct scan functions encode the two variants. Block-context callers use `scan_plain_line_block`; flow-context callers use `scan_plain_line_flow`. The parameterized BNF reduces to exactly two implementations selected by call site, matching the spec's two-way grouping.

### [128] ns-plain-safe-out

BNF:
```
ns-plain-safe-out ::=
  ns-char
```

Spec prose: §7.3.3 — outside flow, any non-whitespace character is safe.

Verdict: Lenient

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/plain.rs:307-309` (`ns_plain_safe_block`), `/workspace/rlsp-yaml-parser/src/chars.rs:67-76` (`is_ns_char`).

Reasoning: `ns_plain_safe_block` is defined as `is_ns_char(ch)` — a direct reduction to the §5 [34] predicate. This composes correctly with the BNF. The leniency arises because `is_ns_char` (per the §5 audit) admits literal non-printable characters that `c-printable` would have rejected (e.g. control characters in the ranges 0x00-0x08, 0x0B-0x0C, 0x0E-0x1F, 0x7F that are not `'\t'`, `'\n'`, `'\r'`). However, the `scan_plain_line_block` implementation at `plain.rs:381` explicitly rejects bytes in `0x00..=0x1F | 0x7F`, so the leniency at the predicate level is masked at the scanner. Per the audit reconciliation principle, the leniency lives at [34] (`ns-char`); since this production's own implementation does NOT enforce a stricter rule, the leniency is inherited and propagates here in the abstract predicate even though the scanner is stricter.

### [129] ns-plain-safe-in

BNF:
```
ns-plain-safe-in ::=
  ns-char - c-flow-indicator
```

Spec prose: §7.3.3 — inside flow context, the five flow indicators `,`, `[`, `]`, `{`, `}` additionally terminate plain scalars to avoid ambiguity with collection structure.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/plain.rs:431-502` (`scan_plain_line_flow`), `plain.rs:467` (terminates on `b',' | b'[' | b']' | b'{' | b'}'`).

Reasoning: The flow-context scanner terminates explicitly on each flow indicator at line 467, in addition to all other terminators that block context handles. The subtraction `ns-char - c-flow-indicator` is realized by the explicit byte match. The implementation matches the spec exactly.

### [130] ns-plain-char(c)

BNF:
```
ns-plain-char(c) ::=
    (
        ns-plain-safe(c)
      - c-mapping-value    # ':'
      - c-comment          # '#'
    )
  | (
      [ lookbehind = ns-char ]
      c-comment          # '#'
    )
  | (
      c-mapping-value    # ':'
      [ lookahead = ns-plain-safe(c) ]
    )
```

Spec prose: §7.3.3 — "Plain scalars must never contain the `: ` and ` #` character combinations."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/plain.rs:320-330` (`ns_plain_char_block`), `plain.rs:340-416` (block scanner with prev_was_ws tracking), `plain.rs:431-502` (flow scanner with same logic).

Reasoning: The function `ns_plain_char_block` directly encodes the three BNF alternatives: `#` allowed only when `!prev_was_ws` (the lookbehind alternative); `:` allowed only when `next.is_some_and(ns_plain_safe_block)` (the lookahead alternative); otherwise the safe-minus-`:`-minus-`#` set. The scanner threads `prev_was_ws` through each iteration (`plain.rs:345`) so the lookbehind is correctly applied. The flow-context scanner additionally makes `:` terminate when followed by space/tab/flow-indicator/EOL (`plain.rs:489-490`), consistent with `ns-plain-safe-in` lookahead. The three alternatives map one-to-one.

### [131] ns-plain(n,c)

BNF:
```
ns-plain(n,FLOW-OUT)  ::= ns-plain-multi-line(n,FLOW-OUT)
ns-plain(n,FLOW-IN)   ::= ns-plain-multi-line(n,FLOW-IN)
ns-plain(n,BLOCK-KEY) ::= ns-plain-one-line(BLOCK-KEY)
ns-plain(n,FLOW-KEY)  ::= ns-plain-one-line(FLOW-KEY)
```

Spec prose: §7.3.3 — "Plain scalars are further restricted to a single line when contained inside an implicit key."

Verdict: Indeterminate

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/plain.rs:31-143` (`try_consume_plain_scalar` always allows continuations), `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1128-1134` (multi-line implicit key error in flow sequence), `flow.rs:1450-1528` (multi-line plain continuation in flow).

Reasoning: As with `nb-double-text` and `nb-single-text`, the plain-scalar entry point does not take an implicit-key parameter. In block context, `try_consume_plain_scalar` always invokes `collect_plain_continuations`. The implicit-key single-line restriction in `BLOCK-KEY` is not enforced directly — block mappings rely on `tick_mapping_phase_after_scalar` to detect a `:` separator, and a multi-line plain key followed by `:` would behave inconsistently with the spec. The flow-context multi-line key check at `flow.rs:1128` partially covers `FLOW-KEY`. Without conformance-suite cross-check, the verdict between Strict-conformant and Lenient cannot be reached on code reading; verdict is Indeterminate.

### [132] nb-ns-plain-in-line(c)

BNF:
```
nb-ns-plain-in-line(c) ::=
  (
    s-white*
    ns-plain-char(c)
  )*
```

Spec prose: §7.3.3 — interior of one line of plain content.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/plain.rs:340-416` (`scan_plain_line_block`), `plain.rs:374-378` (whitespace within a line), `plain.rs:431-502` (`scan_plain_line_flow`).

Reasoning: The block and flow scanners both walk a line byte-by-byte. Interior whitespace (`b' '` / `b'\t'`) is consumed but does not advance `committed_end` until the next non-whitespace `ns-plain-char` is seen — so trailing whitespace is excluded but interior whitespace is preserved. This realizes `(s-white* ns-plain-char(c))*` by tracking the last non-whitespace position.

### [133] ns-plain-one-line(c)

BNF:
```
ns-plain-one-line(c) ::=
  ns-plain-first(c)
  nb-ns-plain-in-line(c)
```

Spec prose: §7.3.3 — first-line plain scalar (no continuation).

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/plain.rs:242-273` (`peek_plain_scalar_first_line` checks `ns_plain_first_block` at line 254-257 then calls `scan_plain_line_block`).

Reasoning: The first-line composition is exactly `ns-plain-first(c)` (the `ns_plain_first_block` test) followed by `nb-ns-plain-in-line(c)` (the `scan_plain_line_block` call that includes the first character via the loop). The two BNF symbols are concatenated in this two-step inspection.

### [134] s-ns-plain-next-line(n,c)

BNF:
```
s-ns-plain-next-line(n,c) ::=
  s-flow-folded(n)
  ns-plain-char(c)
  nb-ns-plain-in-line(c)
```

Spec prose: §7.3.3 — continuation line: fold then content.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/plain.rs:149-230` (`collect_plain_continuations`), `plain.rs:161-168` (blank lines accumulate), `plain.rs:210-216` (fold to space or newlines), `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1450-1528` (flow-context multi-line continuation).

Reasoning: The continuation collector applies `s-flow-folded(n)` semantics — blank lines push pending newlines (`plain.rs:161-168`), and a non-blank line emits either pending newlines or a single fold space (`plain.rs:210-215`). The continuation content is then scanned via `scan_plain_line_block`, which begins with one `ns-plain-char` and continues with `nb-ns-plain-in-line`. The BNF triple is realized in order.

### [135] ns-plain-multi-line(n,c)

BNF:
```
ns-plain-multi-line(n,c) ::=
  ns-plain-one-line(c)
  s-ns-plain-next-line(n,c)*
```

Spec prose: §7.3.3 — first line plus zero or more continuations.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/lexer/plain.rs:31-143` (entry: first line + continuation loop), `plain.rs:124-126` (call to `collect_plain_continuations`).

Reasoning: `try_consume_plain_scalar` first calls `peek_plain_scalar_first_line` to consume the equivalent of `ns-plain-one-line(c)`, then `collect_plain_continuations` runs the `s-ns-plain-next-line(n,c)*` loop. The BNF concatenation maps exactly to the two-stage process.

### [136] in-flow(n,c)

BNF:
```
in-flow(n,FLOW-OUT)  ::= ns-s-flow-seq-entries(n,FLOW-IN)
in-flow(n,FLOW-IN)   ::= ns-s-flow-seq-entries(n,FLOW-IN)
in-flow(n,BLOCK-KEY) ::= ns-s-flow-seq-entries(n,FLOW-KEY)
in-flow(n,FLOW-KEY)  ::= ns-s-flow-seq-entries(n,FLOW-KEY)
```

Spec prose: §7.4 — context-shifting wrapper used by `c-flow-sequence`. Outer FLOW-OUT/FLOW-IN propagate as FLOW-IN; outer BLOCK-KEY/FLOW-KEY propagate as FLOW-KEY (the implicit-key restriction is preserved through nested flow content).

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:49-1748` (entire flow handler is the in-flow body), `flow.rs:159-249` (initialization: depth checks, indent checks, key tracking).

Reasoning: The flow handler is unified — there is no per-context branch for `in-flow`. The 1024-character implicit-key check (`flow.rs:1136-1161`) and the multi-line implicit key check (`flow.rs:1128`) preserve the BLOCK-KEY/FLOW-KEY propagation: when the flow collection itself is a key, those checks transitively constrain inner content. Implementation realizes the BNF context propagation indirectly through these key-position guards. The propagation rule is correctly observed.

### [137] c-flow-sequence(n,c)

BNF:
```
c-flow-sequence(n,c) ::=
  c-sequence-start    # '['
  s-separate(n,c)?
  in-flow(n,c)?
  c-sequence-end      # ']'
```

Spec prose: §7.4.1 — "Flow sequence content is denoted by surrounding `[` and `]` characters."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:394-470` (open `[`), `flow.rs:475-535` (close `]`), `flow.rs:489-500` (sequence end emission).

Reasoning: The opener `[` triggers `Event::SequenceStart` and pushes a `Sequence` frame onto the explicit flow stack (lines 425-446). Whitespace after the `[` is consumed at the top of the next loop iteration (`flow.rs:312-339`), realizing `s-separate(n,c)?`. The body is processed by the main loop until `]` is encountered at line 475, which pops the `Sequence` frame and emits `Event::SequenceEnd` (line 499). The four BNF symbols are realized exactly.

### [138] ns-s-flow-seq-entries(n,c)

BNF:
```
ns-s-flow-seq-entries(n,c) ::=
  ns-flow-seq-entry(n,c)
  s-separate(n,c)?
  (
    c-collect-entry     # ','
    s-separate(n,c)?
    ns-s-flow-seq-entries(n,c)?
  )?
```

Spec prose: §7.4.1 — "Sequence entries are separated by a `,` character. The final `,` may be omitted."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:662-831` (comma handling), `flow.rs:673-680` (leading-comma rejection), `flow.rs:692-701` (double-comma rejection).

Reasoning: The recursive BNF is realized iteratively in the main loop. Entries are emitted (sub-productions `ns-flow-seq-entry`); `s-separate(n,c)?` is realized by the whitespace skip at loop start; `c-collect-entry` is the comma handler at line 662; the recursion is the loop continuation. The "final `,` may be omitted" is realized by allowing the next iteration to encounter `]` directly. Leading-comma (`flow.rs:673`) and consecutive-comma (`flow.rs:692`) protections exceed the BNF's bare structure but match the spec's intent that "flow collection entries can never be completely empty" (§7.4).

### [139] ns-flow-seq-entry(n,c)

BNF:
```
ns-flow-seq-entry(n,c) ::=
  ns-flow-pair(n,c) | ns-flow-node(n,c)
```

Spec prose: §7.4.1 — "Any flow node may be used as a flow sequence entry. In addition, YAML provides a compact notation for the case where a flow sequence entry is a mapping with a single key/value pair."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1088-1251` (`:` handler dispatches to `ns-flow-pair` via `in_implicit_map` insertion), `flow.rs:1216-1246` (sequence single-pair MappingStart insertion), main loop (lines 392-1703) for `ns-flow-node`.

Reasoning: A sequence entry is processed by the main loop dispatching on the first non-whitespace character. If the entry is a regular flow node (`[`, `{`, `'`, `"`, `*`, plain), the corresponding handler emits the node directly. If a `:` arrives after a key has been emitted, the sequence entry is retroactively wrapped in a `MappingStart`/`MappingEnd` pair (lines 1216-1238 insert MappingStart at `key_start_idx`, `flow.rs:497-498` emits MappingEnd on `]`). The two BNF alternatives map to these two paths.

### [140] c-flow-mapping(n,c)

BNF:
```
c-flow-mapping(n,c) ::=
  c-mapping-start       # '{'
  s-separate(n,c)?
  ns-s-flow-map-entries(n,in-flow(c))?
  c-mapping-end         # '}'
```

Spec prose: §7.4.2 — "Flow mappings are denoted by surrounding `{` and `}` characters."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:447-468` (open `{`, `MappingStart`), `flow.rs:475-535` (close `}` with empty-key/empty-value handling at lines 511-518), `flow.rs:519` (`MappingEnd` emission).

Reasoning: The `{` opener emits `Event::MappingStart` and pushes a `Mapping` frame (lines 447-467). The body is processed by the main loop until `}` (line 475), which emits `MappingEnd`. Special handling at lines 511-518 covers `{?}` (null key + null value) and Value-phase `}` (null value). The `in-flow(c)` context propagation is implicit because the main loop applies the same flow-indicator exclusions throughout. The four BNF symbols and the implicit-key context propagation are realized.

### [141] ns-s-flow-map-entries(n,c)

BNF:
```
ns-s-flow-map-entries(n,c) ::=
  ns-flow-map-entry(n,c)
  s-separate(n,c)?
  (
    c-collect-entry     # ','
    s-separate(n,c)?
    ns-s-flow-map-entries(n,c)?
  )?
```

Spec prose: §7.4.2 — mapping entries separated by commas, trailing comma optional.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:662-831` (comma handler with phase reset), `flow.rs:709-775` (empty-value emission on comma in Value phase).

Reasoning: Same iterative reduction of the recursive BNF as for `ns-s-flow-seq-entries`. The comma handler at line 662 resets phase to `Key` (line 818) and clears `has_value`, completing the entry. Empty-value handling at line 709-741 ensures Value-phase entries closed by `,` get a synthetic null value. The mapping-specific phase machinery is the iterative form of the recursion.

### [142] ns-flow-map-entry(n,c)

BNF:
```
ns-flow-map-entry(n,c) ::=
    (
      c-mapping-key    # '?' (not followed by non-ws char)
      s-separate(n,c)
      ns-flow-map-explicit-entry(n,c)
    )
  | ns-flow-map-implicit-entry(n,c)
```

Spec prose: §7.4.2 — "If the optional `?` mapping key indicator is specified, the rest of the entry may be completely empty."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1057-1083` (`?` handler), `flow.rs:1063-1078` (`explicit_key_pending` and `key_is_explicit` flags), main loop after `?` consumption.

Reasoning: A `?` followed by whitespace/EOL (line 1059) sets `explicit_key_pending = true` (line 1074) and `key_is_explicit = true` (line 1078). The main loop then processes the explicit entry following the BNF's `ns-flow-map-explicit-entry`. A `?` not followed by whitespace falls through to plain-scalar handling (line 1082), so the implicit alternative is taken. The two BNF alternatives are dispatched by the lookahead test at line 1059.

### [143] ns-flow-map-explicit-entry(n,c)

BNF:
```
ns-flow-map-explicit-entry(n,c) ::=
    ns-flow-map-implicit-entry(n,c)
  | (
      e-node    # ""
      e-node    # ""
    )
```

Spec prose: §7.4.2 — explicit entry that may be a fully-implicit pair, or a completely empty pair.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:511-513` (null key + null value emission when `}` reached with `explicit_key_pending`).

Reasoning: When `}` is reached with the `Mapping` frame still in `Key` phase and `explicit_key_pending = true`, the handler emits two empty scalars (lines 512-513), matching the BNF's `e-node e-node` alternative. Otherwise the parsing continues with normal implicit-entry processing — covering the first BNF alternative `ns-flow-map-implicit-entry`.

### [144] ns-flow-map-implicit-entry(n,c)

BNF:
```
ns-flow-map-implicit-entry(n,c) ::=
    ns-flow-map-yaml-key-entry(n,c)
  | c-ns-flow-map-empty-key-entry(n,c)
  | c-ns-flow-map-json-key-entry(n,c)
```

Spec prose: §7.4.2 — three implicit-entry forms based on whether the key is YAML-like, empty, or JSON-like.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1088-1251` (`:` handler dispatches by phase + has_value), `flow.rs:1183-1198` (empty-key emission when `:` arrives in Key phase with `has_value=false`).

Reasoning: The three BNF alternatives are distinguished by what precedes the `:`. (1) A YAML-key entry has had a plain or YAML scalar emitted; the `:` advances Key→Value (line 1199-1200). (2) An empty-key entry has `has_value=false` when `:` arrives; the parser emits a null-key scalar (lines 1183-1198). (3) A JSON-key entry had a quoted scalar or flow collection emitted; after the close of the JSON-like value, the `:` advances Key→Value the same way as a YAML key, but `c-ns-flow-map-adjacent-value` allows value-without-separator (handled at lines 1100-1108). Composition is correct.

### [145] ns-flow-map-yaml-key-entry(n,c)

BNF:
```
ns-flow-map-yaml-key-entry(n,c) ::=
  ns-flow-yaml-node(n,c)
  (
      (
        s-separate(n,c)?
        c-ns-flow-map-separate-value(n,c)
      )
    | e-node    # ""
  )
```

Spec prose: §7.4.2 — YAML-like key followed either by `:value` or by an implicit empty value (when only the key is present, e.g. before `,` or `}`).

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:709-741` (empty-value emission on comma in Value phase), `flow.rs:514-518` (empty-value emission on `}` in Value phase).

Reasoning: When a YAML key has been emitted (the frame is in Value phase) and `,` or `}` arrives without a separator + value, the parser emits the implicit `e-node` (lines 728-741 for comma, line 517 for `}`). When `:` is present, the separate-value sub-production handles the value. The two alternatives in the BNF are distinguished at the comma/closer handlers.

### [146] c-ns-flow-map-empty-key-entry(n,c)

BNF:
```
c-ns-flow-map-empty-key-entry(n,c) ::=
  e-node    # ""
  c-ns-flow-map-separate-value(n,c)
```

Spec prose: §7.4.2 — empty key with explicit `: value`.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1182-1198` (empty-key emission when `:` arrives in Key phase with `has_value=false`).

Reasoning: The `:` handler at line 1182-1198 detects "Key phase, no key emitted yet" and emits an empty scalar key with any pending tag/anchor attached, then advances to Value phase. The subsequent value is handled by `c-ns-flow-map-separate-value` semantics. The two BNF symbols (empty key, separator-value) are realized in the proper order.

### [147] c-ns-flow-map-separate-value(n,c)

BNF:
```
c-ns-flow-map-separate-value(n,c) ::=
  c-mapping-value    # ':'
  [ lookahead ≠ ns-plain-safe(c) ]
  (
      (
        s-separate(n,c)
        ns-flow-node(n,c)
      )
    | e-node    # ""
  )
```

Spec prose: §7.4.2 — "Normally, YAML insists the `:` mapping value indicator be separated from the value by white space."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1098-1119` (separator detection), `flow.rs:1099` (lookahead check `is_standard_sep` for whitespace/comma/bracket/EOL), `flow.rs:1100-1108` (adjacent-JSON exception), `flow.rs:1109-1117` (Value-phase plain-scalar-after-colon allowance).

Reasoning: The `is_standard_sep` test at line 1099 enforces the BNF's `lookahead ≠ ns-plain-safe(c)` — the colon is a separator only when the next character is whitespace, a flow indicator, or end-of-line. Two narrowly-scoped exceptions to "lookahead ≠ ns-plain-safe" exist: (a) `is_adjacent_json_sep` allows non-whitespace after `:` only when this colon is actually `c-ns-flow-map-adjacent-value` for a JSON-like key in a flow sequence; (b) `is_mapping_value_phase` allows the second `:` in `{x: :x}` so the value can start with `:`. Both exceptions correspond to legal spec patterns ([148] adjacent value and the value-starts-with-colon plain scalar).

### [148] c-ns-flow-map-json-key-entry(n,c)

BNF:
```
c-ns-flow-map-json-key-entry(n,c) ::=
  c-flow-json-node(n,c)
  (
      (
        s-separate(n,c)?
        c-ns-flow-map-adjacent-value(n,c)
      )
    | e-node    # ""
  )
```

Spec prose: §7.4.2 — "if a key inside a flow mapping is JSON-like, YAML allows the following value to be specified adjacent to the `:`."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1100-1108` (`is_adjacent_json_sep` for adjacent JSON-key value separator in a flow sequence), `flow.rs:514-518` (empty-value branch).

Reasoning: A JSON-like key (quoted scalar or flow collection) emits a value event that sets `has_value=true`. When `:` arrives without whitespace, the `is_adjacent_json_sep` branch (line 1100) recognizes the adjacent value when in a flow sequence context. The "`s-separate(n,c)?`" optionality is realized by the standard-separator test, and the empty-value branch is the same as in [145].

### [149] c-ns-flow-map-adjacent-value(n,c)

BNF:
```
c-ns-flow-map-adjacent-value(n,c) ::=
  c-mapping-value          # ':'
  (
      (
        s-separate(n,c)?
        ns-flow-node(n,c)
      )
    | e-node    # ""
  )
```

Spec prose: §7.4.2 — adjacent value form: `:` may be immediately followed by the value with no whitespace.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1100-1119` (adjacent-value detection with relaxed lookahead).

Reasoning: The adjacent-value form differs from `c-ns-flow-map-separate-value` ([147]) only in that it does NOT require lookahead-not-plain-safe. The `is_adjacent_json_sep` and `is_mapping_value_phase` branches at lines 1100-1117 collectively allow the `:` separator without requiring whitespace separation, exactly when the spec permits it. The optional `s-separate(n,c)?` and the alternation with `e-node` mirror the spec.

### [150] ns-flow-pair(n,c)

BNF:
```
ns-flow-pair(n,c) ::=
    (
      c-mapping-key     # '?' (not followed by non-ws char)
      s-separate(n,c)
      ns-flow-map-explicit-entry(n,c)
    )
  | ns-flow-pair-entry(n,c)
```

Spec prose: §7.4.1 — "A more compact notation is usable inside flow sequences, if the mapping contains a single key/value pair."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1057-1083` (`?` in flow sequence sets `explicit_key_in_seq = true`), `flow.rs:1216-1246` (single-pair MappingStart insertion on `:`).

Reasoning: Inside a flow sequence, a `?` followed by whitespace sets `explicit_key_in_seq = true` (line 1064) and suppresses the DK4H single-line implicit-key check (line 1128). The `:` handler then inserts `MappingStart` retroactively at `key_start_idx` (line 1228), realizing the single-pair mapping wrap. The implicit-pair branch (without `?`) is the default path through the sequence main loop. The two BNF alternatives are dispatched by the explicit-key flag.

### [151] ns-flow-pair-entry(n,c)

BNF:
```
ns-flow-pair-entry(n,c) ::=
    ns-flow-pair-yaml-key-entry(n,c)
  | c-ns-flow-map-empty-key-entry(n,c)
  | c-ns-flow-pair-json-key-entry(n,c)
```

Spec prose: §7.4.1 — three implicit-pair forms parallel to the implicit-mapping-entry forms.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1216-1246` (`:` handler in sequence: key wrap with three sub-cases inline), `flow.rs:1224-1226` (empty-key fallback when `has_value=false`).

Reasoning: The three alternatives are all dispatched by the `:` handler in a sequence frame (line 1210-1247). A YAML key has been emitted as a plain scalar; an empty key triggers the `!*has_value` branch (line 1224); a JSON key has been emitted as a quoted scalar or flow collection. The composition matches the BNF's three-way alternation by distinguishing the source of the preceding entry event.

### [152] ns-flow-pair-yaml-key-entry(n,c)

BNF:
```
ns-flow-pair-yaml-key-entry(n,c) ::=
  ns-s-implicit-yaml-key(FLOW-KEY)
  c-ns-flow-map-separate-value(n,c)
```

Spec prose: §7.4.1 — YAML-like implicit-key pair inside a flow sequence.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1136-1161` (1024-char limit check on implicit key), `flow.rs:1128-1134` (multi-line implicit key check in sequence).

Reasoning: The implicit YAML key in a flow sequence is bounded by the spec's 1024-Unicode-character cap (§7.4.3), which is enforced at lines 1136-1161 — `key_start_byte` is recorded when the key scalar starts (line 1620) and the colon-byte position bounds the length count. The single-line restriction is enforced at line 1128. After the `:`, normal `c-ns-flow-map-separate-value` handling applies.

### [153] c-ns-flow-pair-json-key-entry(n,c)

BNF:
```
c-ns-flow-pair-json-key-entry(n,c) ::=
  c-s-implicit-json-key(FLOW-KEY)
  c-ns-flow-map-adjacent-value(n,c)
```

Spec prose: §7.4.1 — JSON-like implicit-key pair inside a flow sequence.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:884-922` (quoted-scalar key recording), `flow.rs:898-899` (`key_start_byte` recorded when quoted scalar in key position), `flow.rs:1100-1108` (adjacent-value separator).

Reasoning: A JSON-like (quoted or flow-collection) key in a sequence sets `key_start_byte = cur_abs_pos.byte_offset` (line 898). The 1024-char cap applies the same way as for YAML keys. After the JSON-like value emission, the `:` handler routes to the adjacent-value path via `is_adjacent_json_sep`. Composition matches the BNF.

### [154] ns-s-implicit-yaml-key(c)

BNF:
```
ns-s-implicit-yaml-key(c) ::=
  ns-flow-yaml-node(0,c)
  s-separate-in-line?
  /* At most 1024 characters altogether */
```

Spec prose: §7.4.3 — "the `:` indicator must appear at most 1024 Unicode characters beyond the start of the key. In addition, the key is restricted to a single line."

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1136-1161` (1024-char limit), `flow.rs:1128-1134` (single-line restriction), `flow.rs:1619-1621` (record `key_start_byte` for plain scalars in key position).

Reasoning: The 1024-character cap is enforced explicitly at line 1153 by counting `self.input[key_start_byte..colon_byte].chars().count()`. The single-line restriction is enforced at line 1128: `cur_base_pos.line != last_token_line` rejects a `:` whose preceding key spanned more than one line. The `s-separate-in-line?` (whitespace before `:`) is consumed by the loop's whitespace-skip prelude. Both spec constraints are enforced.

### [155] c-s-implicit-json-key(c)

BNF:
```
c-s-implicit-json-key(c) ::=
  c-flow-json-node(0,c)
  s-separate-in-line?
  /* At most 1024 characters altogether */
```

Spec prose: §7.4.3 — same 1024-char cap and single-line restriction as YAML implicit key, applied to JSON-like keys.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:885-899` (quoted-scalar key-start recording for length cap), `flow.rs:1136-1161` (length cap check), `flow.rs:1128-1134` (single-line check).

Reasoning: The same 1024-character cap and single-line constraints applied to YAML keys also apply to JSON-like keys. `key_start_byte` is set whenever a quoted scalar starts in a key position (line 898); subsequent flow-collection JSON keys are handled by the `[`/`{` opener which records the position via the parent frame's `key_start_idx` (lines 414-422). The single-line check uses `last_token_line` which is updated when each token completes. Both constraints are enforced.

### [156] ns-flow-yaml-content(n,c)

BNF:
```
ns-flow-yaml-content(n,c) ::=
  ns-plain(n,c)
```

Spec prose: §7.5 — plain scalars are the only flow style without explicit start/end indicators; they are the YAML-content sub-form.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1534-1675` (plain scalar in flow context).

Reasoning: The flow handler dispatches plain-scalar starts via `is_plain_first` test (lines 1536-1562) and emits the result of `scan_plain_line_flow`. The BNF reduces directly to `ns-plain(n,c)`, and the parser uses the same `scan_plain_line_flow` for both `ns-flow-yaml-content` (the standalone case) and the YAML key form. The reduction is exact.

### [157] c-flow-json-content(n,c)

BNF:
```
c-flow-json-content(n,c) ::=
    c-flow-sequence(n,c)
  | c-flow-mapping(n,c)
  | c-single-quoted(n,c)
  | c-double-quoted(n,c)
```

Spec prose: §7.5 — JSON-like flow styles are the four with explicit start/end indicators.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:394-470` (sequence/mapping start), `flow.rs:877-1052` (single/double quoted dispatch).

Reasoning: The four JSON-like alternatives are dispatched by the first character: `[` and `{` open a nested collection; `'` and `"` invoke the single- or double-quoted lexer methods. Each alternative has its own handler and resulting events. The four-way alternation is realized exactly.

### [158] ns-flow-content(n,c)

BNF:
```
ns-flow-content(n,c) ::=
    ns-flow-yaml-content(n,c)
  | c-flow-json-content(n,c)
```

Spec prose: §7.5 — full flow content alternation.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:392-1703` (main loop dispatch covers all five forms), `flow.rs:394` ([flow-sequence/mapping]), `flow.rs:877` ([quoted]), `flow.rs:1564` ([plain]).

Reasoning: The combined dispatch at the top of each loop iteration considers indicator characters first (`[`, `{`, `'`, `"`) and falls through to plain-scalar handling otherwise. The five alternatives across the two BNF sub-rules are exhaustively dispatched.

### [159] ns-flow-yaml-node(n,c)

BNF:
```
ns-flow-yaml-node(n,c) ::=
    c-ns-alias-node
  | ns-flow-yaml-content(n,c)
  | (
      c-ns-properties(n,c)
      (
          (
            s-separate(n,c)
            ns-flow-yaml-content(n,c)
          )
        | e-scalar
      )
    )
```

Spec prose: §7.5 — YAML-flow node with optional properties; alias-or-plain or properties-then-content.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1355-1423` (alias), `flow.rs:1258-1350` (tag), `flow.rs:1317-1349` (anchor), `flow.rs:1534-1675` (plain content, with pending tag/anchor consumption at lines 1626-1633), `flow.rs:723-741` (e-scalar fallback when properties pending and entry ends at `,`).

Reasoning: The three BNF alternatives are realized as: (1) alias — `*name` handler at line 1355; (2) plain content — main loop plain branch at line 1564; (3) properties then content or e-scalar — tag/anchor handlers at lines 1258 and 1317 set `pending_flow_tag` / `pending_flow_anchor`, and when a content event is emitted (lines 1626-1633) the pending properties are attached. If the entry ends without content, the `,`-handler emits an e-scalar with the pending properties (lines 723-741). The composition is correct.

### [160] c-flow-json-node(n,c)

BNF:
```
c-flow-json-node(n,c) ::=
  (
    c-ns-properties(n,c)
    s-separate(n,c)
  )?
  c-flow-json-content(n,c)
```

Spec prose: §7.5 — JSON-flow node has mandatory separator after properties (since the JSON content has its own start indicator).

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:1258-1350` (tag/anchor with whitespace-skip after), `flow.rs:1302-1308` (whitespace skip after tag), `flow.rs:1340-1346` (whitespace skip after anchor name).

Reasoning: After a tag or anchor is parsed, the parser skips whitespace (lines 1302-1308 for tags, 1340-1346 for anchors), which realizes `s-separate(n,c)`. The pending-property is then consumed by the next JSON content event (e.g. `[`, `{`, `'`, `"` opener attaches the property via `make_meta`). The mandatory separator is implemented as a whitespace-consume rather than a check, but the next character must be a JSON-content opener for the property to attach correctly — equivalent to the BNF's mandatory `s-separate`.

### [161] ns-flow-node(n,c)

BNF:
```
ns-flow-node(n,c) ::=
    c-ns-alias-node
  | ns-flow-content(n,c)
  | (
      c-ns-properties(n,c)
      (
        (
          s-separate(n,c)
          ns-flow-content(n,c)
        )
        | e-scalar
      )
    )
```

Spec prose: §7.5 — most general flow-node form, encompassing both YAML and JSON content.

Verdict: Strict-conformant

Evidence: `/workspace/rlsp-yaml-parser/src/event_iter/flow.rs:392-1703` (full main loop covers all three alternatives), `flow.rs:1355-1423` ([alias]), `flow.rs:1258-1350` ([properties]), `flow.rs:723-741` ([e-scalar fallback]).

Reasoning: The three BNF alternatives correspond to the three top-level dispatches in the main loop: alias (`*name`), content (any of the five flow content forms), and properties-then-content-or-e-scalar (tag/anchor handlers + pending property consumption). The composition reproduces the BNF's three-way alternation with the e-scalar fallback realized at the comma/closer handlers (lines 723-741, 511-518).
