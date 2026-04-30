---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: A
section: §9
date: 2026-04-30
---

### [202] l-document-prefix

BNF:
```
[#] l-document-prefix ::=
  c-byte-order-mark?
  l-comment*
```
Spec prose: "A document may be preceded by a _prefix_ specifying the [character encoding] and optional [comment] lines. Note that all [documents] in a stream must use the same [character encoding]. However it is valid to re-specify the [encoding] using a [byte order mark] for each [document] in the stream. The existence of the optional prefix does not necessarily indicate the existence of an actual [document]." (§9.1.1)
Verdict: Strict-conformant
Evidence: `lines.rs:115-117` (initial BOM stripped on first line); `lines.rs:282-305` (`signal_document_boundary` strips a leading BOM at every inter-document preamble); `lexer.rs:131-146` (`skip_blank_lines_between_docs` calls `signal_document_boundary` before returning so a BOM that begins a follow-on document prefix is consumed); `event_iter/directives.rs:33-64` (`consume_preamble_between_docs` skips blank lines, comment lines, and directive lines before deciding whether a document follows).
Reasoning: The spec allows an optional BOM followed by zero or more comments at the start of any document prefix, including between documents. The first-line strip in `lines.rs:115-117` covers the stream-leading BOM; `signal_document_boundary` at `lines.rs:292-302` covers per-document BOM re-specification. Comment lines preceding a document are consumed by `consume_preamble_between_docs` and emitted as `Event::Comment`, satisfying the `l-comment*` repetition. A prefix without a following document is also tolerated: the same routine is re-entered, and if no `---`/`...` or content line follows, `step_between_docs` (`directives.rs:272-286`) emits `StreamEnd` without forcing a document. The own production correctly composes `c-byte-order-mark?` and `l-comment*` without adding new constraints or relaxing existing ones.

### [203] c-directives-end

BNF:
```
[#] c-directives-end ::= "---"
```
Spec prose: "At the start of a [document], lines beginning with a '%' character are assumed to be [directives]. The (possibly empty) list of [directives] is terminated by a _directives end marker_ line. Lines following this marker can safely use '%' as the first character." (§9.1.2)
Verdict: Strict-conformant
Evidence: `lexer.rs:193-197` (`is_directives_end` calls `is_marker(line.content, b'-')`); `lexer.rs:544-565` (`is_marker` requires bytes 0..3 to all equal `'-'` and the optional 4th byte to be space or tab); `event_iter/step.rs:141,160` and `event_iter/directives.rs:287` (callers gate the marker on column 0 via `peeked_indent == 0` or via the line buffer's column-0 invariant).
Reasoning: The bare BNF says only `"---"`. The parser's `is_marker` additionally requires the 4th byte to be `b-char`/`s-white`/end-of-input, and callers require column 0. Both constraints come from `c-forbidden` [206], which combines with `c-directives-end` at every use site (`c-forbidden ::= <start-of-line> ( c-directives-end | c-document-end ) ( b-char | s-white | <end-of-input> )`). Per the reconciliation principle, attribute these constraints to [206] rather than to [203]: the parser's `is_directives_end` is the composition of [203] with [206], not [203] in isolation. The composition is correct — `is_marker` rejects `---x` because that does not satisfy `c-forbidden`'s follower rule, which is the spec-mandated behaviour anywhere `c-directives-end` is used. The own production composes correctly; strictness sits in [206].

### [204] c-document-end

BNF:
```
[#] c-document-end ::=
  "..."    # (not followed by non-ws char)
```
Spec prose: "At the end of a [document], a _document end marker_ line is used to signal the [parser] to begin scanning for [directives] again. The existence of this optional _document suffix_ does not necessarily indicate the existence of an actual following [document]." (§9.1.2)
Verdict: Strict-conformant
Evidence: `lexer.rs:204-208` (`is_document_end` calls `is_marker(line.content, b'.')`); `lexer.rs:544-565` (`is_marker` requires bytes 0..3 to be `'.'` and the optional 4th byte to be space or tab — exactly the spec's "not followed by non-ws char"); `event_iter/step.rs:141-159` (consume `...` from inside a document and emit `DocumentEnd { explicit: true }`); `event_iter/directives.rs:309-325` (consume orphan `...` between documents); `lexer.rs:241-292` (`consume_marker_line` with `reject_all_inline=true` for `...`, errors any non-comment inline content via `marker_inline_error`).
Reasoning: The spec BNF carries the explicit comment "not followed by non-ws char". The parser implements exactly this via `is_marker` accepting only None/space/tab as the 4th byte. Column 0 is enforced by callers (the `LineBuffer` invariant for marker scans, or `peeked_indent == 0` in `step.rs:141`); column 0 is the universal precondition for both markers and is required by `c-forbidden` [206]. Inline content after `...` is rejected (`lexer.rs:279-284`), which matches the spec — `c-document-end` is followed by a line break in `l-document-suffix`. The own code matches the BNF and its inline comment; no extra constraints, no laxity.

### [205] l-document-suffix

BNF:
```
[#] l-document-suffix ::=
  c-document-end
  s-l-comments
```
Spec prose: "The existence of this optional _document suffix_ does not necessarily indicate the existence of an actual following [document]. Obviously, the actual [content] lines are therefore forbidden to begin with either of these markers." (§9.1.2)
Verdict: Strict-conformant
Evidence: `event_iter/step.rs:141-159` (in-document path: emits `DocumentEnd { explicit: true }`, transitions to `BetweenDocs`, drains any same-line trailing comment via `drain_trailing_comment`); `event_iter/directives.rs:309-325` (between-docs path: orphan `...` is consumed and the loop re-enters `consume_preamble_between_docs` which absorbs subsequent blank/comment lines as `s-l-comments`); `lexer.rs:241-292` (`consume_marker_line(true)` extracts a trailing `# comment` on the marker line itself into `trailing_comment` and rejects any non-comment inline content); `event_iter/directives.rs:33-64` (post-marker comment lines are folded into the next document's prefix).
Reasoning: `s-l-comments` (production [79]) is "0+ blank lines and end-line comments after a separator." The implementation: (1) accepts an optional inline `# …` directly on the `...` line by storing it in `trailing_comment` and emitting it; (2) re-enters the between-docs preamble loop (`directives.rs:33-64`) which skips blank lines and emits comment lines as events. The composition `c-document-end` + `s-l-comments` is therefore implemented faithfully. Inline non-comment content after `...` is rejected by `lexer.rs:279-284`, which is correct because `l-document-suffix` requires the marker to be followed only by `s-l-comments` (whitespace/comment to end-of-line). The production correctly composes its sub-productions.

### [206] c-forbidden

BNF:
```
[#] c-forbidden ::=
  <start-of-line>
  (
      c-directives-end
    | c-document-end
  )
  (
      b-char
    | s-white
    | <end-of-input>
  )
```
Spec prose: "Obviously, the actual [content] lines are therefore forbidden to begin with either of these markers." (§9.1.2)
Verdict: Strict-conformant
Evidence: `lexer.rs:540-565` (`is_marker` predicate enforces start-of-line, three `'-'`/`'.'` bytes, and a trailing `b-char`/`s-white`/end-of-input as the 4th byte's None/space/tab match); `lexer.rs:567-573` (`is_doc_marker_line` wraps both); `lexer/plain.rs:171-173` (plain-scalar continuation aborts on `is_marker` so a content line cannot begin with the markers); `lexer/quoted.rs:88-96,266-274` (single- and double-quoted multi-line scalars error on `is_doc_marker_line`); `lexer/block.rs:147-163` (literal/folded block scalars terminate on a marker line at column 0).
Reasoning: `c-forbidden` is an exclusion rule applied to `c-l+literal`, `c-l+folded`, plain scalars, and quoted scalars. The parser checks `is_marker` (which encodes start-of-line via the line-buffer column-0 invariant, the three-byte marker, and the b-char/s-white/EOI follower) at every continuation point: plain scalars break (`plain.rs:171-173`); quoted scalars error with a clear message (`quoted.rs:88-96` and `quoted.rs:266-274`); block scalars break out of body collection (`block.rs:154-163`). This is exactly the union of forbidden positions the spec implies. Each predicate site uses the same `is_marker`, so the constraints are identical to the BNF: three identical bytes plus a whitespace/EOL/EOI separator. No extra constraint is added; no required exclusion is dropped.

### [207] l-bare-document

BNF:
```
[#] l-bare-document ::=
  s-l+block-node(-1,BLOCK-IN)
  /* Excluding c-forbidden content */
```
Spec prose: "A _bare document_ does not begin with any [directives] or [marker] lines. Such documents are very 'clean' as they contain nothing other than the [content]. In this case, the first non-comment line may not start with a '%' first character. Document [nodes] are [indented] as if they have a parent [indented] at -1 [spaces]. Since a [node] must be more [indented] than its parent [node], this allows the document's [node] to be [indented] at zero or more [spaces]." (§9.1.3)
Verdict: Strict-conformant
Evidence: `event_iter/directives.rs:338-355` (in `BetweenDocs`, when the next line is content (not `---`, not `...`, not blank, not comment, not directive) the parser emits `DocumentStart { explicit: false, … }` and transitions to `InDocument`); `event_iter/step.rs:997-998` (root-level `plain_parent_indent = 0`, `block_parent_indent = usize::MAX` — the root has no parent, equivalent to the spec's "indented at -1"); `event_iter/step.rs:894-913` (root collection closure uses `min_indent_before == Some(0)` so a column-0 root collection is recognised); `event_iter/directives.rs:330-336` (rejects directives without a `---` to start a bare document — bare documents may not have preceding directives, per spec prose).
Reasoning: `s-l+block-node(-1, BLOCK-IN)` permits the document's root node to begin at any column ≥ 0. The parser models the −1 sentinel via `block_parent_indent = usize::MAX` (treated as "no enclosing block, any column allowed") and `plain_parent_indent = 0` for the root context (`step.rs:997-998`). Root collection bookkeeping (`step.rs:894-913`) recognises a root collection that began at column 0, marking `root_node_emitted = true` after it closes — this enforces the single-root-node contract that the production implies. The "Excluding c-forbidden content" clause is enforced by every scalar continuation site discussed under [206]. Bare documents starting with `%` are rejected by the directive-without-`---` guard (`directives.rs:330-336`) plus the in-document directive guard (`step.rs:223-244`), satisfying the spec's "first non-comment line may not start with a '%'". The parent production composes [206] correctly.

### [208] l-explicit-document

BNF:
```
[#] l-explicit-document ::=
  c-directives-end
  (
      l-bare-document
    | (
        e-node    # ""
        s-l-comments
      )
  )
```
Spec prose: "An _explicit document_ begins with an explicit [directives end marker] line but no [directives]. Since the existence of the [document] is indicated by this [marker], the [document] itself may be [completely empty]." (§9.1.4)
Verdict: Strict-conformant
Evidence: `event_iter/directives.rs:287-308` (between-docs: on `is_directives_end`, consume the marker, emit `DocumentStart { explicit: true, … }`, switch to `InDocument`); `event_iter/step.rs:160-207` (in-document: a second `---` ends the previous document and starts a new one with `explicit: true`); `loader.rs:419-427` (loader emits an empty scalar root when the next event after `DocumentStart` is `DocumentEnd` or `StreamEnd`, satisfying the `e-node` alternative); `lexer.rs:241-292` (`consume_marker_line(false)` allows inline content after `---` — handed off to the in-document state to parse as `l-bare-document`).
Reasoning: The production composes `c-directives-end` with either `l-bare-document` (a node body follows, possibly inline on the same line via `consume_marker_line` re-prepending the inline content) or `e-node + s-l-comments` (empty document — the marker line is followed only by comments/blanks until the next boundary). The loader's `is_document_end` peek (`loader.rs:881-886`) decides between these two alternatives at AST construction time and emits an empty scalar for the empty case. Trailing inline `# comment` on the `---` line is captured into `trailing_comment` and emitted, satisfying `s-l-comments`. The composition matches the BNF's two-alternative shape; no constraints added or relaxed.

### [209] l-directive-document

BNF:
```
[#] l-directive-document ::=
  l-directive+
  l-explicit-document
```
Spec prose: "A _directives document_ begins with some [directives] followed by an explicit [directives end marker] line." (§9.1.5)
Verdict: Strict-conformant
Evidence: `event_iter/directives.rs:33-64` (`consume_preamble_between_docs` collects all directive lines into `directive_scope`); `event_iter/directives.rs:107-156` (`parse_yaml_directive`); `event_iter/directives.rs:159-230` (`parse_tag_directive`); `event_iter/directives.rs:272-286,309-325,330-336` (every path that ends `BetweenDocs` without consuming a `---` and with `directive_count > 0` errors with "directives must be followed by a '---' document-start marker"); `event_iter/directives.rs:287-308` (when `---` follows directives, the accumulated `version` and `tag_directives` are attached to the `DocumentStart` event); `event_iter/step.rs:148-156` (`directive_scope` is reset at every document boundary so directives do not leak).
Reasoning: The "+" in `l-directive+` (one or more) is enforced implicitly: any non-directive that produces a directive scope cleanly transitions through `consume_preamble_between_docs`. The strict requirement that directives be followed by `---` is enforced at three exits: EOF (`directives.rs:272-286`), orphan `...` (`directives.rs:309-325`), and any non-marker content line (`directives.rs:330-336`). All three error if `directive_count > 0`. After the `---` is consumed, the document is parsed as `l-explicit-document` (sub-production [208]). The parent production correctly composes its sub-productions with the spec-mandated `---` requirement.

### [210] l-any-document

BNF:
```
[#] l-any-document ::=
    l-directive-document
  | l-explicit-document
  | l-bare-document
```
Spec prose: "A YAML _stream_ consists of zero or more [documents]. Subsequent [documents] require some sort of separation [marker] line." (§9.2)
Verdict: Strict-conformant
Evidence: `event_iter/directives.rs:259-356` (`step_between_docs` dispatches to one of the three alternatives based on what follows): `directive_scope.directive_count > 0` plus `is_directives_end` ⇒ `l-directive-document`; `is_directives_end` with no directives ⇒ `l-explicit-document` (`directives.rs:287-308`); content line with no directives and no `---` ⇒ `l-bare-document` (`directives.rs:338-355`); `directives.rs:309-325` consumes orphan `...` between documents without emitting an `l-any-document`.
Reasoning: The dispatcher correctly chooses among the three alternatives. The order matters: directives must be followed by `---` (so a "directive document" is the only valid form when directives are present, enforced at the three exits cited under [209]); `---` without preceding directives is `l-explicit-document`; otherwise the next non-blank/non-comment/non-marker line opens a bare document. The parent composes its sub-productions; the alternation matches the BNF exactly.

### [211] l-yaml-stream

BNF:
```
[#] l-yaml-stream ::=
  l-document-prefix*
  l-any-document?
  (
      (
        l-document-suffix+
        l-document-prefix*
        l-any-document?
      )
    | c-byte-order-mark
    | l-comment
    | l-explicit-document
  )*
```
Spec prose: "A YAML _stream_ consists of zero or more [documents]. Subsequent [documents] require some sort of separation [marker] line. If a [document] is not terminated by a [document end marker] line, then the following [document] must begin with a [directives end marker] line. … A sequence of bytes is a _well-formed stream_ if, taken as a whole, it complies with the above `l-yaml-stream` production." (§9.2)
Verdict: Strict-conformant
Evidence: `event_iter/base.rs:505-532` (top-level iterator alternates `BetweenDocs` and `InDocument`, emitting `StreamStart` once at the beginning and `StreamEnd` once at the end); `event_iter/base.rs:516-520` (`BeforeStream` emits `StreamStart` then transitions to `BetweenDocs`); `event_iter/directives.rs:272-286` (EOF in `BetweenDocs` emits `StreamEnd`); `event_iter/step.rs:86-115` (EOF in `InDocument` closes all open collections, emits implicit `DocumentEnd { explicit: false }` and `StreamEnd`); `event_iter/step.rs:160-207` (a `---` in the middle of a document closes the current document with `DocumentEnd { explicit: false }` and starts a new one with `DocumentStart { explicit: true }`, satisfying "if not terminated by `...` the next document must begin with `---`"); `event_iter/directives.rs:309-325` (consume `l-document-suffix+` between documents); `event_iter/directives.rs:33-64` (consume inter-document `l-document-prefix*`, including BOMs via `signal_document_boundary` from `lexer.rs:131-146`); `loader.rs:383-456` (loader produces `Vec<Document<Span>>`, one per `DocumentStart`/`DocumentEnd` pair, supporting the "zero or more documents" cardinality including the empty case at `loader.rs:387-395,397-453`).
Reasoning: The stream production is the top-level alternation that the iterator state machine implements. The leading `l-document-prefix*` is the initial `BetweenDocs` entry that absorbs BOMs and comments before the first document. The `l-any-document?` optional first document is dispatched per [210]. The repeating tail covers (a) `l-document-suffix+ l-document-prefix* l-any-document?` — implemented by `directives.rs:309-325` consuming `...` markers and re-entering `consume_preamble_between_docs`; (b) standalone `c-byte-order-mark` between documents — `signal_document_boundary` strips it from the next line; (c) standalone `l-comment` — emitted by `consume_preamble_between_docs`; (d) standalone `l-explicit-document` — handled by `directives.rs:287-308` when `---` appears without preceding `...`. The "subsequent documents require a separator marker" constraint is enforced by `step.rs:160-207` (a bare document cannot follow another bare document without a `---` because the in-document state never re-enters `BetweenDocs` except via `---`/`...`/EOF). The parent composes its sub-productions; `StreamStart`/`StreamEnd` bookend correctly; multi-document streams round-trip through the loader at `loader.rs:383-456`.
