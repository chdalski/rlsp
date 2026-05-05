# BNF Conformance — §9 Document Stream Productions

Source: `.ai/audit/2026-04-30-phase1-bnf/reconciliation-§9.md` (10 entries)

**Verdict tally:** Strict-conformant: 10, Stricter-than-spec: 0, Not-applicable: 0

§9 is the smallest chapter audited (10 productions) and the only chapter with zero inter-auditor disagreements. Document and stream productions have well-defined boundaries and the parser's stream/document handling is consistent across both auditor interpretations.

---

### [202] l-document-prefix

BNF: `l-document-prefix ::= c-byte-order-mark? l-comment*`

- **Verdict:** Strict-conformant
- **Spec (§9.1.1):** "A document may be preceded by a prefix specifying the character encoding and optional comment lines. Note that all documents in a stream must use the same character encoding. However it is valid to re-specify the encoding using a byte order mark for each document in the stream."
- **Implementation:** `signal_document_boundary()` in `lines.rs` — strips a leading BOM from the primed next line at document-prefix positions, implementing the optional `c-byte-order-mark?`; `skip_blank_lines_between_docs()` in `lexer.rs` calls `signal_document_boundary()`; `consume_preamble_between_docs()` in `event_iter/directives.rs` processes the `l-comment*` portion of the prefix
- **Tests:** `tests/encoding.rs` (`parse_events_accepts_bom_immediately_after_document_end_marker`; `parse_events_accepts_bom_after_doc_end_then_blank_lines`; `parse_events_accepts_bom_after_doc_end_then_comment`; `parse_events_accepts_multiple_docs_each_with_bom`; `load_multidoc_with_bom_between_docs_produces_correct_ast`); `src/lines.rs` unit tests (`bom_stripped_after_document_boundary_signal`; `signal_document_boundary_strips_bom_from_primed_next_line`)

### [203] c-directives-end

BNF: `c-directives-end ::= "---"`

- **Verdict:** Strict-conformant
- **Spec (§9.1.2):** "The solution is the use of two special marker lines to control the processing of directives, one at the start of a document and one at the end. At the start of a document, lines beginning with a '%' character are assumed to be directives. The (possibly empty) list of directives is terminated by a directives end marker line."
- **Implementation:** `step_between_docs()` in `event_iter/directives.rs` — `lexer.is_directives_end()` detects `---` at column 0; `lexer.consume_marker_line(false)` consumes the line and captures any inline scalar
- **Tests:** `tests/yaml-test-suite/src/FTA2.yaml` (single block sequence with anchor and explicit document start); `tests/yaml-test-suite/src/2LFX.yaml` (directive + `---` marker); `tests/smoke/directives.rs` (`explicit_document_start_span_covers_dashes`)

### [204] c-document-end

BNF: `c-document-end ::= "..."    # (not followed by non-ws char)`

- **Verdict:** Strict-conformant
- **Spec (§9.1.2):** "At the end of a document, a document end marker line is used to signal the parser to begin scanning for directives again. The existence of this optional document suffix does not necessarily indicate the existence of an actual following document."
- **Implementation:** `step_in_document()` in `event_iter/step.rs` — `lexer.is_document_end()` detects `...` at column 0; `consume_marker_line(true)` enforces that `...` is not followed by non-whitespace content (sets `marker_inline_error` for inline content after `...`)
- **Tests:** `tests/yaml-test-suite/src/3HFZ.yaml` (error: bad footer); `tests/smoke/directives.rs` (`yaml_directive_followed_by_document_end_returns_error`)

### [205] l-document-suffix

BNF: `l-document-suffix ::= c-document-end s-l-comments`

- **Verdict:** Strict-conformant
- **Spec (§9.1.2):** "At the end of a document, a document end marker line is used to signal the parser to begin scanning for directives again. The existence of this optional document suffix does not necessarily indicate the existence of an actual following document."
- **Implementation:** `step_in_document()` in `event_iter/step.rs` — `...` marker detected and consumed via `consume_marker_line(true)`; trailing comments after `...` drained by `drain_trailing_comment()` before emitting `DocumentEnd { explicit: true }`
- **Tests:** `tests/yaml-test-suite/src/2LFX.yaml` (document with suffix); `tests/smoke/directives.rs` (multi-doc stream tests)

### [206] c-forbidden

BNF: `c-forbidden ::= <start-of-line> ( c-directives-end | c-document-end ) ( b-char | s-white | <end-of-input> )`

- **Verdict:** Strict-conformant
- **Spec (§9.1.2):** "Obviously, the actual content lines are therefore forbidden to begin with either of these markers."
- **Implementation:** `step_in_document()` in `event_iter/step.rs` — both marker checks require `peeked_indent == 0`; `is_directives_end()` and `is_document_end()` in `lexer.rs` check for column-0 `---`/`...` not followed by non-whitespace content; a line with indent > 0 cannot be a marker
- **Tests:** `tests/yaml-test-suite/src/N782.yaml` (invalid document markers in flow style — `---` inside flow collection is not at start-of-line); `tests/smoke/directives.rs`

### [207] l-bare-document

BNF: `l-bare-document ::= s-l+block-node(-1,BLOCK-IN)  /* Excluding c-forbidden content */`

- **Verdict:** Strict-conformant
- **Spec (§9.1.3):** "A bare document does not begin with any directives or marker lines. Such documents are very 'clean' as they contain nothing other than the content. In this case, the first non-comment line may not start with a '%' first character."
- **Implementation:** `step_between_docs()` in `event_iter/directives.rs` — when no directives were accumulated and the next token is not `---` or `...`, a `DocumentStart { explicit: false }` event is emitted and state transitions to `InDocument`; `step_in_document()` then parses the block node content
- **Tests:** `tests/yaml-test-suite/src/9KBC.yaml` (bare document, error); `tests/smoke/directives.rs` (`bare_document_sets_explicit_false`)

### [208] l-explicit-document

BNF: `l-explicit-document ::= c-directives-end ( l-bare-document | ( e-node s-l-comments ) )`

- **Verdict:** Strict-conformant
- **Spec (§9.1.4):** "An explicit document begins with an explicit directives end marker line but no directives. Since the existence of the document is indicated by this marker, the document itself may be completely empty."
- **Implementation:** `step_between_docs()` in `event_iter/directives.rs` — when `lexer.is_directives_end()`, emits `DocumentStart { explicit: true, version, tag_directives }` and transitions to `InDocument`; the empty-document case (`e-node`) is handled when `step_in_document()` immediately hits EOF or `...`
- **Tests:** `tests/yaml-test-suite/src/FTA2.yaml` (explicit document start); `tests/smoke/directives.rs` (`explicit_document_marker_sets_explicit_true`); `src/loader.rs` unit tests (UT-D2, UT-D6)

### [209] l-directive-document

BNF: `l-directive-document ::= l-directive+ l-explicit-document`

- **Verdict:** Strict-conformant
- **Spec (§9.1.5):** "A directives document begins with some directives followed by an explicit directives end marker line."
- **Implementation:** `consume_preamble_between_docs()` in `event_iter/directives.rs` — accumulates `%YAML`/`%TAG` directives into `self.directive_scope`; if directives were accumulated but no `---` follows (EOF, `...`, or bare content), returns an error ("directives must be followed by a '---' document-start marker"); when `---` follows, `DocumentStart` includes the accumulated `version` and `tag_directives`
- **Tests:** `tests/yaml-test-suite/src/B63P.yaml` (directive without document — error); `tests/yaml-test-suite/src/2LFX.yaml` (directive + document); `tests/smoke/directives.rs` (directive scope per-document)

### [210] l-any-document

BNF: `l-any-document ::= l-directive-document | l-explicit-document | l-bare-document`

- **Verdict:** Strict-conformant
- **Spec (§9.2):** "A YAML stream consists of zero or more documents. Subsequent documents require some sort of separation marker line. If a document is not terminated by a document end marker line, then the following document must begin with a directives end marker line."
- **Implementation:** `step_between_docs()` in `event_iter/directives.rs` — implements the three-way dispatch: if directives were seen, require `---` (directive document); if `---` is next without directives, emit explicit document; otherwise emit bare document start. The ordering of checks implements the priority `l-directive-document` > `l-explicit-document` > `l-bare-document`
- **Tests:** `tests/smoke/directives.rs` (multi-doc stream, bare and explicit document variants)

### [211] l-yaml-stream

BNF: `l-yaml-stream ::= l-document-prefix* l-any-document? ( ( l-document-suffix+ l-document-prefix* l-any-document? ) | c-byte-order-mark | l-comment | l-explicit-document )*`

- **Verdict:** Strict-conformant
- **Spec (§9.2):** "A YAML stream consists of zero or more documents. Subsequent documents require some sort of separation marker line. If a document is not terminated by a document end marker line, then the following document must begin with a directives end marker line. […] A sequence of bytes is a well-formed stream if, taken as a whole, it complies with the above l-yaml-stream production."
- **Implementation:** state machine in `event_iter/base.rs` — `BeforeStream` → emits `StreamStart`; `BetweenDocs` → `step_between_docs()`; `InDocument` → `step_in_document()`; `Done` → emits nothing. The `c-byte-order-mark` alternative in the outer loop of `l-yaml-stream` is handled via `signal_document_boundary()` in `lines.rs`, called from `skip_blank_lines_between_docs()` in `lexer.rs` — a BOM at a document-prefix position is stripped before the stream parser sees it
- **Tests:** `tests/smoke/stream.rs` (full stream lifecycle: empty, whitespace, multi-doc, comment-only); `tests/conformance/stream.rs` (yaml-test-suite parameterised suite); `tests/encoding.rs` (`parse_events_accepts_bom_immediately_after_document_end_marker`; `parse_events_accepts_multiple_docs_each_with_bom`; `parse_events_bom_after_directives_end_marker_is_error`)
