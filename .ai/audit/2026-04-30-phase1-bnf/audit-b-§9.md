---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase1.md
phase: 1
side: B
section: §9
date: 2026-04-30
---

### [202] l-document-prefix

BNF: `l-document-prefix ::= c-byte-order-mark? l-comment*`
Spec prose: §9.1.1: "A document may be preceded by a prefix specifying the character encoding and optional comment lines. Note that all documents in a stream must use the same character encoding. However it is valid to re-specify the encoding using a byte order mark for each document in the stream."
Verdict: Strict-conformant
Evidence: `src/lexer.rs:131-146` (`skip_blank_lines_between_docs` calls `signal_document_boundary` after each blank-line skip); `src/lines.rs:292-305` (`signal_document_boundary` strips a leading BOM from the primed next line); `src/event_iter/directives.rs:33-64` (`consume_preamble_between_docs` consumes the `l-comment*` portion via `is_comment_line` / `try_consume_comment`); `src/event_iter/step.rs:72-82` (mid-document body BOM rejected as `invalid character U+FEFF`).
Reasoning: The optional `c-byte-order-mark?` is consumed at every document-prefix position by the boundary-signal hook. The BOM strip is gated by `next.content.starts_with('\u{FEFF}')` — a BOM at the start of a document prefix is silently consumed, while a BOM elsewhere (mid-document body) is fielded by the explicit reject in `step_in_document`. The `l-comment*` portion is handled by `consume_preamble_between_docs`, which loops over comment lines and blank lines until a non-comment, non-directive, non-blank line appears. The two halves compose the BNF cleanly. Conformance doc agrees.

### [203] c-directives-end

BNF: `c-directives-end ::= "---"`
Spec prose: §9.1.2: "The solution is the use of two special marker lines to control the processing of directives, one at the start of a document and one at the end. At the start of a document, lines beginning with a \"%\" character are assumed to be directives. The (possibly empty) list of directives is terminated by a directives end marker line."
Verdict: Strict-conformant
Evidence: `src/lexer.rs:193-197` (`is_directives_end` delegates to `is_marker(content, b'-')`); `src/lexer.rs:544-565` (`is_marker` accepts only `bytes[0..3] == [ch,ch,ch]` and `bytes[3] in {None, ' ', '\t'}`); `src/event_iter/directives.rs:287-308` (consumed in `step_between_docs` to emit `DocumentStart { explicit: true }`); `src/event_iter/step.rs:160-207` (consumed inside a document to close-and-restart).
Reasoning: The marker recogniser implements the literal `"---"` token plus the spec-required follow-on constraint that the next char (if any) is whitespace — without that constraint `---x` would also match, but the BNF and §9.1.2 prose treat such a line as content. Column-0 enforcement is implicit: `Line.content` includes leading spaces (`src/lines.rs:142`), so an indented `---` would have `content[0] == ' '` and the byte equality check at `is_marker` line 560 fails. Inside `step_in_document` the `peeked_indent == 0` guard at `step.rs:141`/`step.rs:160` is redundant defence-in-depth; the indent test in `directives.rs:287` is omitted because `step_between_docs` only fires after `consume_preamble_between_docs` has aligned the buffer to a non-prefix line. Conformance doc agrees.

### [204] c-document-end

BNF: `c-document-end ::= "..."    # (not followed by non-ws char)`
Spec prose: §9.1.2: "At the end of a document, a document end marker line is used to signal the parser to begin scanning for directives again. The existence of this optional document suffix does not necessarily indicate the existence of an actual following document."
Verdict: Strict-conformant
Evidence: `src/lexer.rs:204-208` (`is_document_end` delegates to `is_marker(content, b'.')`); `src/lexer.rs:544-565` (same `is_marker` shape: three dots + optional space/tab); `src/event_iter/step.rs:141-159` (in-document `...` closes all collections, emits `DocumentEnd { explicit: true }`); `src/lexer.rs:241-292` (`consume_marker_line(true)` flags non-comment inline content via `marker_inline_error`).
Reasoning: The shape `..."    # (not followed by non-ws char)"` is enforced by both the marker-recogniser (4th byte must be space/tab/none) and the marker-line consumer that sets `marker_inline_error = "invalid content after document-end marker '...'"` for any inline non-comment payload (`lexer.rs:279-284`). A trailing same-line comment is allowed (recorded via `trailing_comment` at `lexer.rs:275-278`), matching the spec prose treating the suffix as `c-document-end s-l-comments`. Conformance doc agrees.

### [205] l-document-suffix

BNF: `l-document-suffix ::= c-document-end s-l-comments`
Spec prose: §9.1.2: "At the end of a document, a document end marker line is used to signal the parser to begin scanning for directives again. The existence of this optional document suffix does not necessarily indicate the existence of an actual following document."
Verdict: Strict-conformant
Evidence: `src/event_iter/step.rs:141-159` (in-document `...` path: closes collections, emits `DocumentEnd { explicit: true }` with span from the marker, drains trailing comment); `src/event_iter/directives.rs:309-326` (between-docs `...` orphan handling); `src/lexer.rs:131-146` (`skip_blank_lines_between_docs` consumes subsequent blank lines that comprise the rest of `s-l-comments`); `src/event_iter/directives.rs:33-64` (`consume_preamble_between_docs` consumes any standalone-comment portion of `s-l-comments` after the suffix).
Reasoning: The suffix decomposes into the `...` token (production [204]) and the comment/blank-line tail (`s-l-comments`). After the marker is consumed, `drain_trailing_comment` at `step.rs:157` records the same-line comment, then control returns to `step_between_docs`, which calls `consume_preamble_between_docs` to drain remaining blank/comment lines. Because both halves are correctly composed from sub-productions that are themselves Strict-conformant in this audit and elsewhere, the parent suffix is Strict-conformant by the reconciliation principle. Conformance doc agrees.

### [206] c-forbidden

BNF: `c-forbidden ::= <start-of-line> ( c-directives-end | c-document-end ) ( b-char | s-white | <end-of-input> )`
Spec prose: §9.1.2: "Obviously, the actual content lines are therefore forbidden to begin with either of these markers."
Verdict: Strict-conformant
Evidence: `src/lexer.rs:544-565` (`is_marker` enforces `<start-of-line>` via byte-0..2 equality on `Line.content`, which includes leading spaces, and follow-on `( s-white | <end-of-input> )` via the `bytes.get(3) == None | Some(' ' | '\t')` match); `src/lexer.rs:567-573` (`is_doc_marker_line`); `src/lexer/quoted.rs:88-96, 266-271` (multi-line single/double-quoted scalars terminate with an error if a marker line appears mid-scalar); `src/lexer/plain.rs:669` (`forbidden_continuation_stops_at_marker` named case verifies plain scalars stop at a `---` continuation); `src/event_iter/step.rs:141, 160` (`peeked_indent == 0` guard prevents indented `---`/`...` from being treated as markers).
Reasoning: The composite predicate has three parts. (1) `<start-of-line>` is enforced because line construction in `src/lines.rs:128-153` makes `Line.content` start at byte 0 of each physical line; an indented marker has a leading space which fails the strict `b0 == ch` byte test. (2) `c-directives-end | c-document-end` is the union of `is_marker(_, b'-')` and `is_marker(_, b'.')`. (3) The follow-on `( b-char | s-white | <end-of-input> )` is enforced via the 4th-byte match. The `b-char` case is implicit: a marker line whose content is exactly `---` or `...` (3 bytes, no trailing) satisfies "marker followed by line break," and a marker followed by `---x` fails at the 4th-byte check. Quoted-scalar and plain-scalar consumers explicitly check `is_doc_marker_line` to terminate, so c-forbidden is honoured even in scalar continuation contexts. Conformance doc agrees.

### [207] l-bare-document

BNF: `l-bare-document ::= s-l+block-node(-1,BLOCK-IN)  /* Excluding c-forbidden content */`
Spec prose: §9.1.3: "A bare document does not begin with any directives or marker lines. Such documents are very \"clean\" as they contain nothing other than the content. In this case, the first non-comment line may not start with a \"%\" first character."
Verdict: Strict-conformant
Evidence: `src/event_iter/directives.rs:338-355` (`step_between_docs` final-fall-through emits `DocumentStart { explicit: false }` when no `%`-directive was seen and the next line is not `---`/`...`); `src/event_iter/step.rs:25-1050` (`step_in_document` parses the block-node body); `src/event_iter/directives.rs:51-57, 272-282, 312-318, 330-336` (a `%`-directive without a `---` follow-on raises `directives must be followed by a '---' document-start marker` — i.e. a directive line cannot lead a bare document).
Reasoning: A bare document is the implicit-start path: `step_between_docs` reaches its final block (`directives.rs:338-355`) only when no directives accumulated and the next token is content (not a marker). The c-forbidden exclusion is ensured by the marker checks earlier in `step_between_docs` (`directives.rs:287, 309`) and the in-document marker checks (`step.rs:141, 160`) that fire before any content-parse path can consume a `---`/`...` line. The `%`-line restriction is enforced by `consume_preamble_between_docs` at `directives.rs:51-57`: a directive line is parsed and accumulated, and if no `---` follows, the parser errors at `directives.rs:330-336`. The block-node body is delegated to `step_in_document` whose conformance is established by §6/§7/§8 productions. The composition is correct, so this parent production is Strict-conformant. Conformance doc agrees.

### [208] l-explicit-document

BNF: `l-explicit-document ::= c-directives-end ( l-bare-document | ( e-node s-l-comments ) )`
Spec prose: §9.1.4: "An explicit document begins with an explicit directives end marker line but no directives. Since the existence of the document is indicated by this marker, the document itself may be completely empty."
Verdict: Strict-conformant
Evidence: `src/event_iter/directives.rs:287-308` (`step_between_docs` on `is_directives_end()` consumes the marker, emits `DocumentStart { explicit: true }`, transitions to `InDocument`); `src/event_iter/step.rs:86-115` (`step_in_document` at EOF emits empty `Scalar`/`DocumentEnd`/`StreamEnd` — handles the `e-node` empty-document case); `src/event_iter/step.rs:141-158` (in-document `...` closes the document via `DocumentEnd { explicit: true }`).
Reasoning: The explicit document is dispatched from `step_between_docs` when the next line is the directives-end marker and no directives were accumulated for the current document (the directive-without-marker check at `directives.rs:330-336` fires before bare/explicit dispatch when directives exist — but in the explicit-without-directives case, the `directive_count == 0` guard is implicit because the explicit branch comes first). The `l-bare-document` alternative is the body parsed by `step_in_document`. The `e-node s-l-comments` alternative is the empty-document case: at-EOF (`step.rs:86-115`) and at `...` (`step.rs:141-158`) the parser emits an empty plain scalar with `value: Cow::Borrowed("")`, satisfying the `e-node` part, then drains comments. The composition is correct. Conformance doc agrees.

### [209] l-directive-document

BNF: `l-directive-document ::= l-directive+ l-explicit-document`
Spec prose: §9.1.5: "A directives document begins with some directives followed by an explicit directives end marker line."
Verdict: Strict-conformant
Evidence: `src/event_iter/directives.rs:33-64` (`consume_preamble_between_docs` accumulates `%YAML`/`%TAG` directives into `self.directive_scope`); `src/event_iter/directives.rs:287-308` (when `is_directives_end()` fires, `DocumentStart { explicit: true, version, tag_directives }` carries the accumulated directives); `src/event_iter/directives.rs:272-336` (a directive without a following `---` errors with `directives must be followed by a '---' document-start marker` — covers EOF, `...`, and bare-content cases).
Reasoning: The `l-directive+` part is enforced by `consume_preamble_between_docs` looping over directive lines and incrementing `directive_count`. The mandatory `l-explicit-document` follow-on is enforced by three error sites: EOF (`directives.rs:272-282`), orphan `...` (`directives.rs:312-318`), and bare-content (`directives.rs:330-336`). All three reject when `directive_count > 0` and no `---` was found. The accumulated version and tag-handle map are shipped as event metadata when the `---` does fire. The directive-document specifically is the path `directives.rs:51-57` → `directives.rs:287-308`, composing correctly. Conformance doc agrees.

### [210] l-any-document

BNF: `l-any-document ::= l-directive-document | l-explicit-document | l-bare-document`
Spec prose: §9.2: "A YAML stream consists of zero or more documents. Subsequent documents require some sort of separation marker line. If a document is not terminated by a document end marker line, then the following document must begin with a directives end marker line."
Verdict: Strict-conformant
Evidence: `src/event_iter/directives.rs:259-356` (`step_between_docs` is the three-way alternation site); `directives.rs:33-64` (preamble drains into the `l-directive+` accumulator); `directives.rs:287-308` (explicit/directive-document branch when `is_directives_end()`); `directives.rs:330-336` (rejects directive-without-marker so the directive branch cannot leak into bare); `directives.rs:338-355` (bare-document fallthrough emits `DocumentStart { explicit: false }`).
Reasoning: The dispatch order in `step_between_docs` correctly mirrors the spec alternation. After `consume_preamble_between_docs` settles the preamble: if `is_directives_end()` and `directive_count > 0`, this is `l-directive-document`; if `is_directives_end()` and `directive_count == 0`, this is `l-explicit-document`; otherwise (no marker, directive_count must be 0 because the `directives.rs:330-336` guard short-circuits), this is `l-bare-document`. A directive followed by content (no marker) is rejected, so the bare path is correctly closed off when directives precede it — preventing an invalid stream from being interpreted as a bare document plus dangling directives. Conformance doc agrees.

### [211] l-yaml-stream

BNF: `l-yaml-stream ::= l-document-prefix* l-any-document? ( ( l-document-suffix+ l-document-prefix* l-any-document? ) | c-byte-order-mark | l-comment | l-explicit-document )*`
Spec prose: §9.2: "A YAML stream consists of zero or more documents. Subsequent documents require some sort of separation marker line. If a document is not terminated by a document end marker line, then the following document must begin with a directives end marker line. […] A sequence of bytes is a well-formed stream if, taken as a whole, it complies with the above l-yaml-stream production."
Verdict: Strict-conformant
Evidence: `src/event_iter/base.rs:505-532` (`Iterator::next` runs the `BeforeStream → BetweenDocs → InDocument → Done` state machine); `src/event_iter/base.rs:516-519` (`StreamStart` emitted on first call); `src/event_iter/directives.rs:259-356` (`step_between_docs` handles inter-document prefixes/suffixes/comments); `src/event_iter/step.rs:86-115` (EOF in-document emits `DocumentEnd { explicit: false } → StreamEnd`); `src/event_iter/directives.rs:283-286` (EOF in `BetweenDocs` emits `StreamEnd` only after the directive-without-marker guard); `src/lexer.rs:131-146` (BOM stripped at every prefix position via `signal_document_boundary`); `src/encoding.rs` (BOM at stream start handled at the byte-level decode).
Reasoning: The state machine composes the four sub-productions referenced in the BNF. (1) `l-document-prefix*` — any sequence of leading BOM, comments, and blanks at stream start or between documents is absorbed by `consume_preamble_between_docs` plus the boundary BOM strip. (2) `l-any-document?` — the optional first document is the result of either the explicit/directive branch (with `---`) or the bare branch; if the stream is empty, the EOF-in-`BetweenDocs` arm at `directives.rs:283-286` emits `StreamEnd` directly, satisfying the `?` quantifier. (3) The repeating `(...)*` outer alternation — a `...` suffix triggers `DocumentEnd { explicit: true }` then re-enters `BetweenDocs`, which loops back through the preamble-and-document logic; a bare `---` mid-document closes the prior doc and starts a new one (`step.rs:160-207`), matching the `l-explicit-document` outer alternative; a stand-alone BOM, comment, or blank between documents is absorbed by the same preamble path. (4) End-of-stream is uniformly emitted as `StreamEnd` via two sites (`step.rs:112, 285` in directives.rs). Each sub-production is Strict-conformant by independent audit, and the composition correctly enforces the well-formedness condition. Conformance doc agrees.
