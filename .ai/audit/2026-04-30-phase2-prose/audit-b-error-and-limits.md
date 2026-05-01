---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: B
section: error-and-limits
date: 2026-04-30
---

# Phase 2 Behavioral Audit — Error semantics and resource limits (Auditor B)

## Method

All probes were exercised through the public `parse_events()`
and `LoaderBuilder::build().load()` APIs via a standalone
audit-probe Cargo project at
`/tmp/audit-probe-error-and-limits-b/` (path-dependency on
`rlsp-yaml-parser`). The probe project was deleted after
observation; no probe code was added to the parser tree
(`git status --porcelain` for `rlsp-yaml-parser/` is empty).

Each requirement records the exact input fed to the parser
and the verbatim error (or successful event sequence) that
came back. For limit categories I enumerated every
`pub const MAX_*` in `src/limits.rs`, every option exposed
on `LoaderOptions` / `LoaderBuilder` in `src/loader.rs`,
and the implicit `UNRESOLVED_VALUE_MAX_CHARS` truncation
constant in `loader.rs:940`. Phase 1 prior-art entries
([59]/[60]/[61], [86], [87]) were re-tested for error
position accuracy. The §6.8 [110] hardcoded
`MAX_DIRECTIVES_PER_DOC` finding from Phase 2 was used as
input to REQ-error-and-limits-9 below.

## Summary of categories

| Limit category | Constant / Option | Layer | Default |
|---|---|---|---|
| Anchor name length | `MAX_ANCHOR_NAME_BYTES` | parser | 1024 bytes |
| Verbatim/shorthand tag length | `MAX_TAG_LEN` | parser | 4096 bytes |
| Comment length | `MAX_COMMENT_LEN` | parser | 4096 bytes |
| Directives per document | `MAX_DIRECTIVES_PER_DOC` | parser | 64 |
| `%TAG` handle length | `MAX_TAG_HANDLE_BYTES` | parser | 256 bytes |
| Resolved tag length | `MAX_RESOLVED_TAG_LEN` (= `MAX_TAG_LEN`) | parser | 4096 bytes |
| Block/flow nesting depth | `MAX_COLLECTION_DEPTH` (parser) and `LoaderOptions::max_nesting_depth` (loader) | parser + loader | 512 |
| Anchor count per doc | `LoaderOptions::max_anchors` | loader | 10 000 |
| Alias-expansion node count | `LoaderOptions::max_expanded_nodes` | loader | 1 000 000 |
| Unresolved-scalar truncation | `loader::UNRESOLVED_VALUE_MAX_CHARS` | loader | 128 chars |

The loader also exposes `LoaderOptions::mode`
(`Lossless` / `Resolved`) and `LoaderOptions::schema`
(`Failsafe` / `Json` / `Core`), but those are not resource
limits and are out of scope for §error-and-limits.

## REQ-error-and-limits-1 — Lex-error position accuracy: unterminated single-quoted scalar

- **Spec requirement (§7.4):** Quoted scalars must be
  closed; an EOF inside a single-quoted scalar is an error.
- **Test method:** Probe fed
  `"key: 'unterminated\n"` (19 bytes) to `parse_events()`
  and read the error's `pos` field.
- **Test input:** `key: 'unterminated\n`
- **Observed output:** `Err { pos: Pos { byte_offset: 19,
  line: 2, column: 0 }, message: "unterminated
  single-quoted scalar" }`. Byte 19 is one past the input
  end (i.e. EOF reached after the trailing newline).
- **Spec expectation:** Reject; position should be
  meaningful (an LSP could drive a diagnostic from it).
- **Verdict:** Lenient — position points at EOF rather
  than the opening `'` (byte 5) or the unclosed-line end.
- **Evidence:**
  `rlsp-yaml-parser/src/lexer/quoted.rs:79-86`
  (`pos: self.current_pos` captures the current parser
  position, which has already advanced past the line
  break).
- **Reasoning:** The position is well-formed and consistent
  with `Error`'s contract (it is *a* byte offset in the
  input range). It is not, however, "the actual offending
  byte" — the offender is the opening quote at byte 5.
  The spec is silent on which byte should be reported, so
  this does not violate any normative requirement;
  classifying as Lenient documents the divergence from
  the requirement's exact wording ("must point to the
  actual offending byte").

## REQ-error-and-limits-2 — Lex-error position: unterminated flow collection

- **Spec requirement (§7.4):** Flow collections terminate
  at `]` / `}`; missing close → error.
- **Test method:** Probe fed `"[\n"` (2 bytes) to
  `parse_events()`.
- **Test input:** `[\n`
- **Observed output:** `Err { pos: Pos { byte_offset: 2,
  line: 2, column: 0 }, message: "unterminated flow
  collection: unexpected end of input" }`.
- **Spec expectation:** Reject.
- **Verdict:** Lenient — same shape as REQ-1: the position
  is at EOF (byte 2), not at the offending `[` (byte 0).
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/flow.rs` (the flow
  iterator surfaces `current_pos` when it discovers EOF
  before finding the matching close).
- **Reasoning:** Behaviour matches REQ-1: structured error
  is produced, position is well-formed, but it points at
  end-of-stream rather than the opener. Same caveat: the
  spec does not mandate which byte; this is a usability
  gap, not a normative violation.

## REQ-error-and-limits-3 — Parser-state error position: tab-as-indentation

- **Spec requirement (§6.1):** `s-indent(n) ::= s-space×n`
  — a tab character cannot serve as block-collection
  indentation.
- **Test method:** Probe fed `"\t bad: indentation\n"` to
  `parse_events()`.
- **Test input:** `\t bad: indentation\n`
- **Observed output:** `Err { pos: Pos { byte_offset: 0,
  line: 1, column: 0 }, message: "tabs are not allowed as
  indentation (YAML 1.2 §6.1)" }`. Byte 0 = `0x09` (the
  tab character itself).
- **Spec expectation:** Reject; position should be the
  offending tab.
- **Verdict:** Strict-conformant — position points at the
  exact offending byte (the tab).
- **Evidence:** Lexer surfaces the tab position via
  `current_pos` when entering the indent-parse path.
- **Reasoning:** Error message cites the spec section and
  the position lands on byte 0, the offending tab. This
  is the requirement's "actual offending byte" criterion
  satisfied exactly.

## REQ-error-and-limits-4 — Numeric-escape rejection position ([59]/[60]/[61])

- **Spec requirement (Phase 1 prior finding):** §5 [59]
  `ns-esc-8-bit`, [60] `ns-esc-16-bit`, [61]
  `ns-esc-32-bit` — the spec allows any hex-escaped
  codepoint; the implementation rejects when the decoded
  character is non-`c-printable` (security-hardened).
- **Test method:** Probe fed
  `"key: \"\\x01\"\n"` (decoded `\x01` is non-printable).
- **Test input:** `key: "\x01"\n`
- **Observed output:** `Err { pos: Pos { byte_offset: 6,
  line: 1, column: 6 }, message: "escape produces
  non-printable character U+0001" }`. Byte 6 is the
  backslash that introduces the offending escape.
- **Spec expectation:** Spec accepts; this implementation
  rejects (Stricter-than-spec, security-hardened).
- **Verdict:** Strict-conformant on position — the
  position points at the escape's leading `\`, the
  offending byte for the rejected escape.
- **Evidence:**
  `rlsp-yaml-parser/src/lexer/quoted.rs:594-618`
  (rejection site); position is captured at the start of
  the escape sequence before decoding.
- **Reasoning:** The strictness itself is on the
  Stricter-than-spec entry [59]/[60]/[61]; this REQ
  measures only position accuracy, which is correct.

## REQ-error-and-limits-5 — Phase 1 [86] / [87] error positions (`%YAML` rejection)

- **Spec requirement (Phase 1 prior finding):** [86] /
  [87] — implementation rejects major≠1 and any
  major/minor that doesn't fit `u8` (Stricter-than-spec
  for the digit-count cap).
- **Test method:** Probe fed several `%YAML` forms:
  - `%YAML 0.1\n---\nfoo\n` (major=0)
  - `%YAML 2.0\n---\nfoo\n` (major≥2)
  - `%YAML 1.300\n---\nfoo\n` (minor > u8::MAX)
  - `%YAML 256.1\n---\nfoo\n` (major > u8::MAX)
  - `%YAML 1.0\n---\nfoo\n` (control: minor=0)
- **Test inputs and outputs:**
  - `%YAML 0.1` → `Err { pos: byte 0, "unsupported YAML
    version 0.1: only 1.x is supported" }` (byte 0 = `%`).
  - `%YAML 2.0` → same shape, same position.
  - `%YAML 1.300` → `Err { pos: byte 0, "malformed %YAML
    minor version: \"300\"" }`.
  - `%YAML 256.1` → `Err { pos: byte 0, "malformed %YAML
    major version: \"256\"" }`.
  - `%YAML 1.0` → **accepted** (no error). Phase 1 prior
    finding noted "major-0" rejection; minor-0 is
    accepted.
- **Spec expectation:** Position should point at the
  offending byte (the digit run that fails).
- **Verdict:** Lenient on position — pos always points at
  byte 0 (the `%`), not at the offending major/minor
  substring (which starts at byte 6 / byte 8).
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:107-156`
  — every error in `parse_yaml_directive` is constructed
  with `pos: dir_pos` (the line-start `%`), regardless of
  which sub-substring failed.
- **Reasoning:** All paths are structured errors (no
  panics); the position is consistent and points at the
  start of the offending directive line. It does not
  point at the actual offending byte (the digit run), so
  per the requirement's exact wording this is Lenient. The
  Phase 1 [86]/[87] strictness is independent — that
  attribution belongs to the Phase 1 entry, not here.

## REQ-error-and-limits-6 — Anchor name length-limit error

- **Spec requirement (none — security limit):** Spec
  places no upper bound on anchor names; the parser caps
  at `MAX_ANCHOR_NAME_BYTES = 1024` to prevent DoS.
- **Test method:** Probe constructed `&{a × 1025} v\n`
  and read the error.
- **Test input:** `&{1025 × 'a'} v\n`
- **Observed output:** `Err { pos: Pos { byte_offset: 0,
  line: 1, column: 0 }, message: "anchor name exceeds
  maximum length of 1024 bytes" }`.
- **Spec expectation:** Limit enforced; structured error.
- **Verdict:** Strict-conformant — limit fires, structured
  error returned, message cites the byte cap. Position
  points at the start of the anchor token.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:38-45`.
- **Reasoning:** Position points at the offending token's
  start, message identifies the cap. No panic. Fully
  meets the limit-violation criterion.

## REQ-error-and-limits-7 — Verbatim-tag length-limit error

- **Spec requirement (none — security limit):** Spec
  places no upper bound on tag URIs; parser caps at
  `MAX_TAG_LEN = 4096` bytes.
- **Test method:** Probe fed
  `!<{4097 × 'a'}> hello\n`.
- **Observed output:** `Err { pos: byte 0, "verbatim tag
  URI exceeds maximum length of 4096 bytes" }`.
- **Spec expectation:** Limit enforced; structured error.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:155-160`.
- **Reasoning:** Same shape as REQ-6 — token-start
  position, structured error, no panic.

## REQ-error-and-limits-8 — Comment length-limit error

- **Spec requirement (none — security limit):** Spec
  imposes no upper bound on comment length; parser caps
  at `MAX_COMMENT_LEN = 4096` bytes.
- **Test method:** Probe fed `# {4097 × 'x'}\n`.
- **Observed output:** `Err { pos: byte 0, "comment
  exceeds maximum allowed length (4096 bytes)" }`.
- **Spec expectation:** Limit enforced; structured error.
- **Verdict:** Strict-conformant.
- **Evidence:** Limit constant referenced in
  `event_iter/directives.rs:40,246` and elsewhere; lexer
  raises a structured error when the byte count is
  exceeded.
- **Reasoning:** Structured `Error`, no panic, message
  cites the cap.

## REQ-error-and-limits-9 — Directives-per-document limit

- **Spec requirement (none — security limit):**
  Phase 2 §6.8 finding: parser caps at
  `MAX_DIRECTIVES_PER_DOC = 64` (Stricter-than-spec —
  spec imposes no cap).
- **Test method:** Probe synthesised 65 distinct `%TAG`
  directives followed by `---\nfoo\n` and read the
  error.
- **Observed output:** `Err { pos: Pos { byte_offset:
  2284, line: 65, column: 0 }, message: "directive count
  exceeds maximum of 64 per document" }`. Byte 2284 is
  the `%` of the 65th directive — the offending token.
- **Spec expectation:** Limit enforced; structured error
  pointing at the offender.
- **Verdict:** Strict-conformant on position and error
  shape — the position lands on the exact offending
  directive's `%`. (The Stricter-than-spec attribution
  itself belongs to the §6.8 [110] Phase 2 finding, not
  to this REQ.)
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:75-83`
  — `pos: dir_pos` is the position of the current
  directive's `%`.
- **Reasoning:** Among the directive-related limit errors
  this is the most precise — the position reflects the
  *specific* directive that triggered the limit, not the
  start of the document.

## REQ-error-and-limits-10 — `%TAG` handle length-limit error

- **Spec requirement (none — security limit):** Parser
  caps at `MAX_TAG_HANDLE_BYTES = 256` bytes.
- **Test method:** Probe fed `%TAG !{257 × 'a'}!
  tag:foo:\n---\nv\n`.
- **Observed output:** `Err { pos: byte 0, "tag handle
  exceeds maximum length of 256 bytes" }`.
- **Spec expectation:** Limit enforced; structured error.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:190-196`.
- **Reasoning:** Position points at the directive's `%`
  (consistent with all directive errors), structured
  error returned, no panic.

## REQ-error-and-limits-11 — Resolved-tag length-limit error

- **Spec requirement (none — security limit):** Parser
  caps the post-expansion tag at
  `MAX_RESOLVED_TAG_LEN = MAX_TAG_LEN = 4096` bytes.
- **Test method:** Probe registered
  `%TAG !p! tag:example.com:\n` then used
  `!p!{4097 - len(prefix) × 'a'} hello`.
- **Observed output:** `Err { pos: Pos { byte_offset:
  30, line: 3, column: 0 }, message: "resolved tag
  exceeds maximum length of 4096 bytes" }`.
- **Spec expectation:** Limit enforced; structured error.
- **Verdict:** Lenient on position — the position is the
  start of line 3 (immediately after the `---\n` marker)
  rather than the offending `!p!` token. Structurally
  correct otherwise.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directive_scope.rs:100-149`
  — three call sites all use the requesting position
  passed in (which here is the document-content start,
  not the tag token start).
- **Reasoning:** Structured error, no panic, but the
  position does not reflect the offending byte — it
  reflects the line-start of the document content.

## REQ-error-and-limits-12 — Loader nesting-depth limit (`max_nesting_depth`)

- **Spec requirement (none — security limit):** Loader
  defaults to `max_nesting_depth = 512`; parser-level
  `MAX_COLLECTION_DEPTH = 512` is the same number.
- **Test method:** Probe fed `[× 513` to both APIs.
  - via `parse_events`: `Err { pos: byte 512, line 1,
    column 512, message: "collection nesting depth
    exceeds limit" }`.
  - via `LoaderBuilder::new().build().load(input)`:
    `Err(LoadError::Parse { pos: byte 512, line 1,
    column 512, message: "collection nesting depth
    exceeds limit" })`.
- **Spec expectation:** Limit enforced; structured error
  pointing at the offending byte (the 513th `[`).
- **Verdict:** Strict-conformant on parser-level — the
  position lands exactly on the 513th `[`. Loader simply
  surfaces the parser's error.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/flow.rs:397`
  (flow case),
  `rlsp-yaml-parser/src/event_iter/block/sequence.rs:176`
  (block sequence),
  `rlsp-yaml-parser/src/event_iter/block/mapping.rs:494`
  (block mapping). Loader-level limit at
  `loader.rs:514-519` and `loader.rs:625-630` returns
  `LoadError::NestingDepthLimitExceeded { limit }` when
  the loader-side counter is the limiter — confirmed
  separately in source review.
- **Reasoning:** Both APIs return structured errors with
  meaningful positions. The parser's limit fires first
  for default-config inputs because the two limits share
  a default value of 512.

## REQ-error-and-limits-13 — Loader anchor-count limit (`max_anchors`)

- **Spec requirement (none — security limit):**
  `LoaderOptions::max_anchors` defaults to 10 000.
- **Test method:** Probe set `max_anchors(2)` and loaded
  `[&a 1, &b 2, &c 3]\n`.
- **Observed output:**
  `Err(LoadError::AnchorCountLimitExceeded { limit: 2 })`.
- **Spec expectation:** Limit enforced; structured error.
- **Verdict:** Strict-conformant — but note that
  `LoadError::AnchorCountLimitExceeded` carries no `pos`
  field. The error is structured and reports the limit;
  it does not localise the offending anchor.
- **Evidence:**
  `rlsp-yaml-parser/src/loader.rs:82-87`
  (variant definition — no `pos` field);
  `loader.rs:753-779` (enforcement site).
- **Reasoning:** The "limit-violation produces a
  structured error" criterion is met. The "error
  position" criterion is not directly applicable because
  this variant has no position field by design.

## REQ-error-and-limits-14 — Loader alias-expansion node limit (`max_expanded_nodes`)

- **Spec requirement (none — security limit):**
  `LoaderOptions::max_expanded_nodes` defaults to
  1 000 000; only enforced in `LoadMode::Resolved`.
- **Test method:** Probe set
  `.resolved().max_expanded_nodes(10)` and loaded
  `\n- &a [1, 2, 3, 4, 5]\n- *a\n- *a\n- *a\n`.
- **Observed output:**
  `Err(LoadError::AliasExpansionLimitExceeded {
  limit: 10 })`.
- **Spec expectation:** Limit enforced; structured error.
- **Verdict:** Strict-conformant. Like REQ-13, this
  variant has no `pos` field.
- **Evidence:**
  `rlsp-yaml-parser/src/loader.rs:89-94` (variant);
  `loader.rs:765-771,810-815` (enforcement sites).
- **Reasoning:** Mode-gated correctly (Lossless mode does
  not enforce, which is documented as the safe default
  for LSP usage); structured error returned; no panic.

## REQ-error-and-limits-15 — Multi-byte correctness for byte-counted limits

- **Spec requirement (none — internal correctness):** The
  three byte-count caps (`MAX_ANCHOR_NAME_BYTES`,
  `MAX_TAG_LEN`, `MAX_COMMENT_LEN`) document themselves as
  "byte length", so the implementation must use
  `s.len()` (bytes), not `s.chars().count()`.
- **Test method:** Probe fed an anchor of 342 × `'锚'`
  (each char = 3 bytes; total 1026 bytes / 342 chars) and
  separately 341 × `'锚'` (1023 bytes / 341 chars).
- **Observed output:**
  - 342 × `'锚'` (1026 B) → `Err { "anchor name exceeds
    maximum length of 1024 bytes" }`.
  - 341 × `'锚'` (1023 B) → no error (anchor accepted).
- **Spec expectation:** Limit fires when byte length
  exceeds 1024, irrespective of char count.
- **Verdict:** Strict-conformant — limit is enforced at
  the documented byte boundary, not at the char boundary.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:38`
  (`if end > MAX_ANCHOR_NAME_BYTES` where `end` is a byte
  offset).
- **Reasoning:** A char-based limit would have rejected
  the 342-char (1026 B) input only when chars > 1024,
  i.e. never for this test. Observed behaviour
  matches a byte limit exactly.

## REQ-error-and-limits-16 — Multi-byte correctness for char-counted truncation

- **Spec requirement (none — internal correctness):**
  `loader::UNRESOLVED_VALUE_MAX_CHARS = 128` documents
  itself as "Unicode scalar values" (char count); the
  implementation must use `chars().count()`, not bytes.
- **Test method:** Probe forced
  `LoadError::UnresolvedScalar` under `Schema::Json` with
  multi-byte values of 128 and 129 `'锚'` characters.
- **Observed output:**
  - 128 × `'锚'` → `value` field is 128 chars / 384 bytes;
    no `...` suffix.
  - 129 × `'锚'` → `value` field is 128 chars + `...`;
    trimmed length is exactly 128 chars.
- **Spec expectation:** Truncation boundary is exactly 128
  Unicode scalar values, not 128 bytes.
- **Verdict:** Strict-conformant — `sanitize_scalar_for_error`
  uses `chars().enumerate()` and truncates by char index.
- **Evidence:**
  `rlsp-yaml-parser/src/loader.rs:948-970` —
  `for (i, ch) in raw.chars().enumerate() { if i >=
  UNRESOLVED_VALUE_MAX_CHARS { … } }`.
- **Reasoning:** Char-counted boundary observed exactly
  at the documented threshold; matches the rule that
  "limits expressed in characters use chars().count()
  not bytes" in the requirement set.

## REQ-error-and-limits-17 — Error recovery: stop-at-first

- **Spec requirement (none — implementation choice):** A
  YAML processor may either stop at the first error or
  continue and report multiple. The conformance doc and
  `tests/error_reporting.rs` document this parser as
  stop-at-first.
- **Test method:** Probe fed
  `key: 'unterminated\nfoo: bar\nbaz: qux\n` (one error
  on line 1, valid content on lines 2–3) and counted
  errors and emitted events.
- **Observed output:** 5 events total: `StreamStart`,
  `DocumentStart`, `MappingStart`,
  `Err("unterminated single-quoted scalar")` — iteration
  ends after the error. Exactly 1 error event; the error
  is the last item; no events after the error.
- **Spec expectation:** Documented behaviour matches
  observed behaviour.
- **Verdict:** Strict-conformant — observed behaviour
  matches the documented contract exactly.
- **Evidence:**
  `rlsp-yaml-parser/tests/error_reporting.rs:251-276`
  (`parse_events_stops_after_first_error`,
  `parse_events_emits_stream_start_before_error`).
- **Reasoning:** The parser does not produce a `Warning`
  event variant (Phase 2 architectural finding) and the
  `Event` enum has no continue-on-error variant; the
  stop-at-first contract is the only one consistent with
  the public API.

## REQ-error-and-limits-18 — No panics on adversarial inputs

- **Spec requirement (none — robustness):** All error
  paths must produce structured `Error` / `LoadError`
  results, never panic.
- **Test method:** Probe fed 27 short adversarial inputs
  (empty string, lone indicators, dangling anchors,
  malformed flow collections, mismatched indents,
  truncated `%YAML`, etc.) through `parse_events()` under
  `std::panic::catch_unwind`.
- **Observed output:** All 27 inputs returned without
  panic; 12 produced a structured `Err`, 15 produced
  successful event sequences. No panics observed.
- **Test method (loader):** Same probe also exercised
  the loader for nesting/anchors/expansion limits — all
  returned structured `LoadError` variants without panic.
- **Spec expectation:** No panics in any error path.
- **Verdict:** Strict-conformant. Source-level grep for
  `panic!`/`unwrap()`/`expect(` outside `#[cfg(test)]`
  in `src/lexer/`, `src/event_iter/`, `src/loader/` and
  `src/loader.rs` returns no production-code matches —
  test code is the only consumer. The
  `clippy::indexing_slicing` and `clippy::panic` lints
  are workspace-deny except in test modules; this is
  enforced by clippy in CI.
- **Evidence:** Probe results above; source-tree grep
  results recorded during methodology.
- **Reasoning:** Structured errors observed in every
  case; no panic propagation; the workspace lint config
  reinforces the absence of panics in production paths.

## REQ-error-and-limits-19 — `LoadError::UndefinedAlias` and `LoadError::CircularAlias` carry no position

- **Spec requirement (§7.1):** "It is an error for an
  alias node to use an anchor that does not previously
  occur in the document."
- **Test method:** Probe loaded `*missing\n` and
  `&a\nfoo: *a\n` in resolved mode.
- **Observed output:**
  - `*missing` →
    `Err(LoadError::UndefinedAlias { name: "missing" })`.
  - `&a\nfoo: *a` →
    `Err(LoadError::UndefinedAlias { name: "a" })`
    (the anchor `&a` is on the empty-scalar root; by the
    time `*a` is resolved, the anchor map has not yet
    registered the root, so the alias is reported as
    undefined — the loader does not detect this case as
    a cycle).
- **Spec expectation:** Reject; spec doesn't mandate
  position reporting.
- **Verdict:** Lenient — neither
  `LoadError::UndefinedAlias` nor
  `LoadError::CircularAlias` carries a `pos` field by
  design (`loader.rs:96-108` — the variants store only
  `name: String`). The error is structured and identifies
  the offender by name, but loses byte/line/column
  information.
- **Evidence:**
  `rlsp-yaml-parser/src/loader.rs:96-108` (variant
  definitions);
  `rlsp-yaml-parser/src/loader.rs:781-799,818-827`
  (construction sites — `name: name.to_owned()` only).
- **Reasoning:** No spec violation (spec is silent on
  position reporting), but a usability gap relative to
  the requirement's "error positions must point to the
  actual offending byte" criterion. Documenting as
  Lenient.

## REQ-error-and-limits-20 — `LoadError::UnresolvedScalar` position accuracy

- **Spec requirement (§10.2.2):** Under JSON schema, a
  plain scalar that doesn't match the four type patterns
  is an error.
- **Test method:** Probe fed
  `"  not_a_json_scalar_value\n"` to a JSON-schema
  loader and read the error position.
- **Observed output:** `Err(LoadError::UnresolvedScalar
  { value: "not_a_json_scalar_value", pos: Pos {
  byte_offset: 2, line: 1, column: 2 } })`. Byte 2 is
  `'n'` — the first byte of the unresolved scalar
  content (after the two leading spaces).
- **Spec expectation:** Reject; position should locate
  the offending scalar.
- **Verdict:** Strict-conformant — position points at
  the actual first byte of the offending scalar.
- **Evidence:**
  `rlsp-yaml-parser/src/loader.rs:1027-1032` —
  `pos: span_start_to_pos(loc.start, line_index)` uses
  the scalar's own span start.
- **Reasoning:** Among the loader-emitted errors this is
  the most precisely localised. The `value` field is
  also sanitized (control chars escaped, truncated to
  128 chars) per `sanitize_scalar_for_error` —
  defense-in-depth against log injection.

## REQ-error-and-limits-21 — All `MAX_*` constants have documented defaults

- **Spec requirement (none — internal contract):**
  Requirement set 5 — "each limit has a defined default
  documented in source."
- **Test method:** Read every `pub const MAX_*` from
  `src/limits.rs` and every `pub` field from
  `LoaderOptions` in `src/loader.rs`.
- **Observed output:** Each constant carries an extensive
  doc-comment explaining the limit, the security
  rationale, the default value, and the failure mode
  (returns `Error` / `LoadError`, not a panic).
  - `MAX_COLLECTION_DEPTH = 512` (`limits.rs:14`)
  - `MAX_ANCHOR_NAME_BYTES = 1024` (`limits.rs:28`)
  - `MAX_TAG_LEN = 4096` (`limits.rs:42`)
  - `MAX_COMMENT_LEN = 4096` (`limits.rs:54`)
  - `MAX_DIRECTIVES_PER_DOC = 64` (`limits.rs:64`)
  - `MAX_TAG_HANDLE_BYTES = 256` (`limits.rs:73`)
  - `MAX_RESOLVED_TAG_LEN = MAX_TAG_LEN = 4096`
    (`limits.rs:85`)
  - `LoaderOptions::max_nesting_depth = 512`
    (`loader.rs:188-198`)
  - `LoaderOptions::max_anchors = 10_000`
    (`loader.rs:188-198`)
  - `LoaderOptions::max_expanded_nodes = 1_000_000`
    (`loader.rs:188-198`)
  - `loader::UNRESOLVED_VALUE_MAX_CHARS = 128`
    (`loader.rs:940`).
- **Spec expectation:** Each limit has a defined,
  documented default.
- **Verdict:** Strict-conformant.
- **Evidence:** Source-file ranges above.
- **Reasoning:** Every limit constant is named, has a
  numeric default, and carries a doc comment that
  explains the security rationale and the failure mode.

## REQ-error-and-limits-22 — All limit categories accessible via the public API

- **Spec requirement (none — internal contract):**
  Requirement set 4 — "every limit category exposed via
  `LoaderOptions` / `LoaderBuilder` plus internal limit
  constants."
- **Test method:** Inspected `LoaderBuilder` fluent API
  and `pub use` re-exports in `lib.rs`.
- **Observed output:**
  - Loader-level limits (`max_nesting_depth`,
    `max_anchors`, `max_expanded_nodes`) are configurable
    via `LoaderBuilder` (`loader.rs:240-258`).
  - Parser-level constants (`MAX_*`) are *not*
    configurable at runtime — they are compile-time
    `pub const`s re-exported via `lib.rs:32-35` so
    callers can reference them but not override them.
  - `UNRESOLVED_VALUE_MAX_CHARS` is `const` (private)
    in `loader.rs:940` — not configurable.
- **Spec expectation:** Public API documents and surfaces
  the categories.
- **Verdict:** Lenient (mild) — the loader-side limits
  are configurable, but the seven parser-side `MAX_*`
  constants are compile-time-only. Configurability of
  every limit is not in the requirement set as-stated;
  the requirement is "enumerated", which is met. I
  classify Lenient because the parser-side limits cannot
  be raised by callers — a caller running into a
  legitimate-but-large input has no recourse short of
  forking. Where parser-level limits are tighter than
  the loader limits this gap is particularly relevant.
- **Evidence:**
  `rlsp-yaml-parser/src/lib.rs:32-35` (re-export of
  `MAX_*` constants);
  `rlsp-yaml-parser/src/loader.rs:212-277`
  (`LoaderBuilder` fluent API).
- **Reasoning:** All categories are documented and
  named. The lenience is the absence of runtime tuning
  for parser-side limits — a usability gap rather than a
  correctness one.

## Verdict Summary

| Verdict | Count | REQs |
|---|---|---|
| Strict-conformant | 16 | 3, 4, 6, 7, 8, 9, 10, 12, 13, 14, 15, 16, 17, 18, 20, 21 |
| Lenient | 6 | 1, 2, 5, 11, 19, 22 |
| Stricter-than-spec | 0 | (Phase 1's [59]/[60]/[61], [86]/[87] entries are attributed to those REQs, not propagated here per symmetric reconciliation) |
| Indeterminate | 0 | — |

### Notes for reconciliation

- The "Lenient" entries in this audit are all about
  **error-position usability**, not about whether errors
  are produced. Every limit and every error path returns
  a structured result without panicking; the gap is
  always "the byte offset isn't where the offending
  token starts."
- REQ-22's "Lenient" classification is a soft one — it
  documents that parser-side limits are compile-time
  only. Reconciliation may adjust this to
  Strict-conformant if the rubric only asks for
  enumeration (which is met) and not configurability.
- The `MAX_DIRECTIVES_PER_DOC = 64` cap is itself a
  Phase 2 §6.8 [110] Stricter-than-spec finding; this
  audit measures only the *behaviour* of the limit
  (REQ-9), not the strictness of having it.
- The Phase 1 finding that "major-0" is rejected is
  confirmed (REQ-5) — `%YAML 0.x` is rejected with the
  "unsupported YAML version" error. `%YAML 1.0` is
  *accepted* — minor=0 is not the same as major=0.
