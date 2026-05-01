---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: B
section: §6.8
date: 2026-04-30
---

# Phase 2 Behavioral Audit — §6.8 Directives (Auditor B)

## Method

All test inputs were exercised through the public
`parse_events()` API via a standalone audit-probe Cargo
project at `/tmp/audit-probe-6.8-b/` with a path dependency
on `rlsp-yaml-parser`. No probe code was added to the
parser tree. Every requirement entry below records the
exact bytes fed to `parse_events()` and the verbatim event
sequence (or first error) the parser emitted.

The conformance doc claims §6.8 is "Conformant" across the
board ([83]–[88]). The probes below test that claim
behaviorally; where the parser exhibits stricter or more
lenient behavior than the spec mandates, this audit
records the discrepancy on the requirement where the rule
is enforced (per the symmetric reconciliation principle).

## REQ-§6.8-1 — `%YAML 1.2` accepted

- **Spec requirement (§6.8.1):** "A version 1.2 YAML
  processor must accept documents with an explicit
  `%YAML 1.2` directive[.]"
- **Test method:** Standalone probe; fed
  `"%YAML 1.2\n---\nfoo\n"` to `parse_events()`.
- **Test input:** `%YAML 1.2\n---\nfoo\n`
- **Observed output:** `StreamStart`,
  `DocumentStart { explicit: true, version: Some((1, 2)),
  tag_directives: [] }`, `Scalar("foo")`, `DocumentEnd`,
  `StreamEnd`.
- **Spec expectation:** Document accepted; version 1.2
  recorded.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:107-156`
  (`parse_yaml_directive`); `directives.rs:296-305`
  (version propagated to `DocumentStart`).
- **Reasoning:** Directive parsed; version surfaced into
  `DocumentStart.version` exactly as the spec requires.

## REQ-§6.8-2 — Document with no `%YAML` directive accepted

- **Spec requirement (§6.8.1):** "[…] as well as documents
  lacking a `YAML` directive."
- **Test method:** Standalone probe; fed `"foo\n"` to
  `parse_events()`.
- **Test input:** `foo\n`
- **Observed output:** `StreamStart`,
  `DocumentStart { explicit: false, version: None,
  tag_directives: [] }`, `Scalar("foo")`, `DocumentEnd`,
  `StreamEnd`.
- **Spec expectation:** Document accepted with default 1.2
  semantics; `version` field reflects absence.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:344-355`
  (implicit `DocumentStart` with `version: None` when no
  directive parsed).
- **Reasoning:** Bare content is parsed without complaint
  and `version: None` correctly distinguishes
  directive-absent from directive-present.

## REQ-§6.8-3 — Higher major version (`%YAML 2.0`) rejected with error

- **Spec requirement (§6.8.1):** "Documents with a `%YAML`
  directive specifying a higher major version (e.g.
  `%YAML 2.0`) must be rejected with an appropriate error
  message."
- **Test method:** Standalone probe; fed
  `"%YAML 2.0\n---\nfoo\n"` to `parse_events()`.
- **Test input:** `%YAML 2.0\n---\nfoo\n`
- **Observed output:** `StreamStart`, then
  `Err { message: "unsupported YAML version 2.0: only 1.x
  is supported" }`.
- **Spec expectation:** Reject with appropriate error.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:146-151`
  (rejects `major != 1`).
- **Reasoning:** Error includes the offending version and
  states the supported range — meets "appropriate error
  message" requirement.

## REQ-§6.8-4 — Higher minor version (`%YAML 1.3`) processed (with warning)

- **Spec requirement (§6.8.1):** "Documents with a `%YAML`
  directive specifying a higher minor version (e.g.
  `%YAML 1.3`) should be processed with an appropriate
  warning."
- **Test method:** Standalone probe; fed
  `"%YAML 1.3\n---\nfoo\n"` to `parse_events()`.
- **Test input:** `%YAML 1.3\n---\nfoo\n`
- **Observed output:** `StreamStart`,
  `DocumentStart { explicit: true, version: Some((1, 3)),
  tag_directives: [] }`, `Scalar("foo")`, `DocumentEnd`,
  `StreamEnd`. No warning event emitted (`Event::Comment`
  or analogous absent).
- **Spec expectation:** "should" — non-mandatory. Document
  processed; warning ideal.
- **Verdict:** Strict-conformant.
- **Reasoning:** The spec uses "should" (RFC 2119 — not
  mandatory). The parser processes the document, propagates
  `Some((1, 3))` to the consumer, and emits no error. The
  parser has no warning channel in its event vocabulary,
  but per RFC 2119 "should" the omission is acceptable.
  Consumers can detect minor-version mismatch by inspecting
  `DocumentStart.version`.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:146-153`
  (no major/minor check beyond `major != 1`); event
  vocabulary at `event.rs:128-140`
  (`DocumentStart.version` exposes raw `(u8, u8)` to
  consumer).

## REQ-§6.8-5 — Lower minor version (`%YAML 1.1`) processed (with adjustment)

- **Spec requirement (§6.8.1):** "A version 1.2 YAML
  processor must also accept documents with an explicit
  `%YAML 1.1` directive. […] Documents with a `%YAML`
  directive specifying a lower minor version (e.g.
  `%YAML 1.1`) should be processed with an appropriate
  adjustment."
- **Test method:** Standalone probe; fed
  `"%YAML 1.1\n---\nfoo\n"` to `parse_events()`.
- **Test input:** `%YAML 1.1\n---\nfoo\n`
- **Observed output:** `StreamStart`,
  `DocumentStart { explicit: true, version: Some((1, 1)),
  tag_directives: [] }`, `Scalar("foo")`, `DocumentEnd`,
  `StreamEnd`.
- **Spec expectation:** Must accept; "should" adjust
  parsing for 1.1 incompatibilities (non-ASCII line breaks,
  etc.).
- **Verdict:** Strict-conformant on the "must accept" half;
  Lenient on the "appropriate adjustment" half.
- **Reasoning:** The "must" half (acceptance) is satisfied
  — the document parses without error and version
  surfaces. The "should" half (1.1-vs-1.2 adjustments such
  as the non-ASCII line-break treatment called out in the
  spec) is not enforced — the parser applies 1.2 lexical
  rules uniformly regardless of declared version. Per RFC
  2119 "should" this is permissible (not a mandatory
  failure), and consumers receive the version field and
  can apply their own adjustments. Logging this as Lenient
  on §6.8.1 because the parser intentionally does not
  mode-switch on version.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:107-156`
  — version is stored verbatim and the parsing pipeline
  consults no version-conditional branches downstream.
  Cross-checked: no `directive_scope.version` reads exist
  outside `tag_directives` collection and event emission.

## REQ-§6.8-6 — Major version 0 rejected

- **Spec requirement (§6.8.1):** Spec defines no behavior
  for major version 0 — implicitly an unsupported version.
  Spec only mandates rejection of "higher" major.
- **Test method:** Standalone probe; fed
  `"%YAML 0.5\n---\nfoo\n"` to `parse_events()`.
- **Test input:** `%YAML 0.5\n---\nfoo\n`
- **Observed output:** `StreamStart`, then
  `Err { message: "unsupported YAML version 0.5: only 1.x
  is supported" }`.
- **Spec expectation:** No mandate — spec only requires
  rejection of higher major. Lower-major (e.g. 0.x) is
  unspecified.
- **Verdict:** Stricter-than-spec.
- **Reasoning:** Spec does not explicitly mandate
  rejection of major < 1 — only major > 1. Phase 1 noted
  this same observation (`[86]` Stricter-than-spec). The
  parser's check `major != 1` rejects both directions,
  which is more conservative than the spec demands.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:146`
  (`if major != 1`).

## REQ-§6.8-7 — Major version 256+ rejected at lex time (overflow)

- **Spec requirement (§6.8.1):** Higher major must be
  rejected. The version grammar is `ns-dec-digit+ '.'
  ns-dec-digit+` — arbitrary digit counts.
- **Test method:** Standalone probe; fed
  `"%YAML 256.0\n---\nfoo\n"`,
  `"%YAML 1.300\n---\nfoo\n"`,
  `"%YAML 1.256\n---\nfoo\n"` to `parse_events()`.
- **Test inputs:** As above.
- **Observed output:**
  - `%YAML 256.0`: `Err { message: "malformed %YAML
    major version: \"256\"" }`.
  - `%YAML 1.300`: `Err { message: "malformed %YAML
    minor version: \"300\"" }`.
  - `%YAML 1.256`: `Err { message: "malformed %YAML
    minor version: \"256\"" }`.
- **Spec expectation:** Reject as "higher major" or
  process minor with warning. Both must produce coherent
  error or warning, not silent acceptance.
- **Verdict:** Stricter-than-spec on the cause-classification
  axis (overflow rejected as "malformed" rather than
  "unsupported"); spec-compliant on the
  reject/process-with-warning axis.
- **Reasoning:** Phase 1 noted this same observation
  (`[87]` Stricter-than-spec — `parse::<u8>` bounds
  digits to [0, 255]). Behaviorally: 256+ in major
  rejects (which the spec wants for higher-major); 256+
  in minor rejects (which the spec says "should" warn,
  not error). The minor-version case is the meaningful
  divergence — `%YAML 1.300` is grammatically valid per
  `ns-yaml-version` but the parser rejects with
  "malformed" instead of accepting with implicit
  warning. Consumers cannot recover from this error.
  Per RFC 2119 "should" the spec permits adjustment
  behavior, but rejection-on-overflow weakens the
  must-accept guarantee that future-compatible 1.x
  consumers might rely on.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:136-143`
  — `parse::<u8>()` rejects 256+ as `ParseIntError`;
  the `if major != 1` check is downstream of parse.

## REQ-§6.8-8 — Duplicate `%YAML` directive in same document rejected

- **Spec requirement (§6.8.1):** "It is an error to specify
  more than one `%YAML` directive for the same document,
  even if both occurrences give the same version number."
- **Test method:** Standalone probe; fed two duplicate
  forms.
- **Test inputs:**
  - `%YAML 1.2\n%YAML 1.2\n---\nfoo\n` (same version)
  - `%YAML 1.2\n%YAML 1.1\n---\nfoo\n` (different versions)
- **Observed output (both):** `StreamStart`, then
  `Err { message: "duplicate %YAML directive in the same
  document", pos: line 2 col 0 }`.
- **Spec expectation:** Error in both cases.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:108-113`
  (`if self.directive_scope.version.is_some()` rejects).
- **Reasoning:** "Even if both occurrences give the same
  version number" — the spec explicitly forbids the same
  case the probe exercised. The parser rejects both,
  satisfying the strict reading.

## REQ-§6.8-9 — Per-document `%YAML` scope (duplicates do not span document boundaries)

- **Spec requirement (§6.8.1):** Directives apply to the
  *single* document they precede; "the same document" in
  the duplicate-rejection rule means a single document.
- **Test method:** Standalone probe; fed two complete
  documents each with their own `%YAML 1.2`.
- **Test input:** `%YAML 1.2\n---\nfoo\n...\n%YAML 1.2\n---\nbar\n`
- **Observed output:** Two successful `DocumentStart`
  events, each with `version: Some((1, 2))`. No duplicate
  error.
- **Spec expectation:** Per-document scope.
- **Verdict:** Strict-conformant.
- **Evidence:** `directives.rs:295-305` and the
  `DirectiveScope` reset between documents — Phase 1
  audit reflects this; the empty `tag_handles` map and
  `version: None` in document 2's `DocumentStart` (when
  no directive precedes it) confirms the scope is reset.
  Verified by R3.F probe.

## REQ-§6.8-10 — `%TAG` primary handle (`!`) registers a prefix

- **Spec requirement (§6.8.2.1):** Primary handle is `!`;
  "By default, the prefix associated with this handle is
  `!`. […] An explicit `TAG` directive may override [it]."
- **Test method:** Standalone probe; fed `%TAG !
  tag:example.com:` then a body using `!type x`.
- **Test input:** `%TAG ! tag:example.com:\n---\n!type x\n`
- **Observed output:** `DocumentStart` carries
  `tag_directives: [("!", "tag:example.com:")]`. Scalar
  parses with no error.
- **Spec expectation:** Primary handle binding stored.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:218-228`
  (insert into `tag_handles`); `directive_scope.rs:134-151`
  (primary `!suffix` resolution applies registered
  prefix).

## REQ-§6.8-11 — `%TAG` secondary handle (`!!`) defaults and overrides

- **Spec requirement (§6.8.2.1):** "By default, the prefix
  associated with this handle is `tag:yaml.org,2002:`. […]
  An explicit `TAG` directive may override this default."
- **Test method:** Two probes — default (no `%TAG !!`),
  and override.
- **Test inputs:**
  - `---\n!!str x\n` (default)
  - `%TAG !! tag:example.com:\n---\n!!str x\n` (override)
- **Observed output:**
  - Default: `DocumentStart.tag_directives: []` (correctly
    empty — defaults are not stored). Body parses; resolved
    `!!str` → `tag:yaml.org,2002:str` per
    `directive_scope.rs:93-108`.
  - Override: `DocumentStart.tag_directives: [("!!",
    "tag:example.com:")]`. Body resolves `!!str` against
    the overridden prefix.
- **Spec expectation:** Both default and override behave
  per spec.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directive_scope.rs:93-108`
  — `map_or("tag:yaml.org,2002:", String::as_str)` provides
  default; override path uses the registered handle.

## REQ-§6.8-12 — `%TAG` named handle (`!handle!`) requires explicit declaration

- **Spec requirement (§6.8.2.1):** "A handle name must not
  be used in a tag shorthand unless an explicit `TAG`
  directive has associated some prefix with it."
- **Test method:** Two probes.
- **Test inputs:**
  - `%TAG !e! tag:example.com:\n---\n!e!type x\n` (declared)
  - `---\n!unknown!type x\n` (undeclared)
- **Observed output:**
  - Declared: clean parse; tag resolves.
  - Undeclared: `Err { message: "undefined tag handle:
    !unknown!" }` at the scalar position (line 2 col 0).
- **Spec expectation:** Undeclared use is an error;
  declared use resolves.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directive_scope.rs:111-132`
  (`return Err` for undeclared named handles).

## REQ-§6.8-13 — `%TAG` per-document scope (no carry across `...`)

- **Spec requirement (§6.8):** Directives are
  per-document; the same handle reused in the next
  document must be re-declared.
- **Test method:** Two-document probe.
- **Test input:** `%TAG !e! tag:a:\n---\n!e!t a\n...\n---\n!e!t b\n`
- **Observed output:** Doc 1 parses cleanly; Doc 2 emits
  `DocumentStart.tag_directives: []` (scope reset), then
  `Err { message: "undefined tag handle: !e!" }` on the
  scalar.
- **Spec expectation:** Doc 1 directives do not bind in
  Doc 2.
- **Verdict:** Strict-conformant.
- **Evidence:** `DirectiveScope` is reset between
  documents (the `directive_count: 0` and empty
  `tag_handles` in Doc 2's `DocumentStart` confirm the
  reset is effective).

## REQ-§6.8-14 — Duplicate `%TAG` handle in same document rejected

- **Spec requirement (§6.8.2):** "It is an error to
  specify more than one `%TAG` directive for the same
  handle in the same document, even if both occurrences
  give the same prefix."
- **Test method:** Three probes covering primary,
  secondary, and named handles.
- **Test inputs:**
  - `%TAG ! tag:a:\n%TAG ! tag:b:\n---\nfoo\n` (primary,
    different prefix)
  - `%TAG !! tag:a:\n%TAG !! tag:b:\n---\nfoo\n`
    (secondary, different prefix)
  - `%TAG !e! tag:a:\n%TAG !e! tag:a:\n---\nfoo\n` (named,
    same prefix)
- **Observed output (all three):**
  `Err { message: "duplicate %TAG directive for handle
  \"<H>\"" }` at line 2 col 0.
- **Spec expectation:** Error in all cases including
  "even if both occurrences give the same prefix."
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:217-223`
  (`if tag_handles.contains_key` rejects).

## REQ-§6.8-15 — Reserved/unknown directive ignored (with warning)

- **Spec requirement (§6.8):** "A YAML processor should
  ignore unknown directives with an appropriate warning."
- **Test method:** Standalone probe; fed several unknown
  directive forms.
- **Test inputs:**
  - `%FOO bar\n---\nfoo\n`
  - `%YAMLISH 1.2\n---\nfoo\n`
  - `%FOO\n---\nfoo\n` (no params)
  - `%RES1 a\n%RES2 b c\n---\nfoo\n` (multiple)
- **Observed output (all):** `DocumentStart` with
  `version: None` and `tag_directives: []`; document body
  parses normally; no event reflects the unknown
  directives. No warning of any form.
- **Spec expectation:** "should" — non-mandatory; ignore
  + warn.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:98-103`
  — unknown directive names increment `directive_count`
  and return `Ok(())` without consulting the params or
  emitting any event.
- **Reasoning:** "Should ignore unknown directives with
  an appropriate warning" — RFC 2119 "should" is
  non-mandatory. The parser ignores; the warning
  omission is permissible. Phase 1 documented this same
  reading and this audit confirms the behavior matches
  end-to-end. Note conformance doc agrees ([83]).
  However, see REQ-§6.8-16 for a behavioral side-effect.

## REQ-§6.8-16 — Reserved directive participates in `MAX_DIRECTIVES_PER_DOC` limit

- **Spec requirement (§6.8):** The spec does not specify a
  per-document directive count limit; the limit is an
  implementation defense against DoS. The spec also has no
  prescription on whether reserved directives count
  against any limit.
- **Test method:** Standalone probe; fed 65 reserved
  directives followed by `---`.
- **Test input:** 65 lines of `%R<N> v\n` then `---\nfoo\n`.
- **Observed output:** `Err { message: "directive count
  exceeds maximum of 64 per document", pos: line 65 col 0 }`.
- **Spec expectation:** No spec mandate; implementation
  may impose a limit. The relevant spec axis is "ignore
  unknown directives" — counting them does not contradict
  ignoring them.
- **Verdict:** Strict-conformant on spec axis (ignoring is
  preserved); Stricter-than-spec on
  side-effect-of-counting axis (reserved directives are
  not silently absorbed; they consume budget).
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:75-83`
  (`MAX_DIRECTIVES_PER_DOC` check applies before
  dispatch); `directives.rs:100` (counter increment for
  reserved directive).
- **Reasoning:** The behavior is consistent with the
  spec's "should ignore" because rejection at the limit
  is a hard implementation guard, not directive
  evaluation. A user could in principle author a
  document with >64 reserved directives that the spec
  permits but this parser rejects. The conservatism is
  defensible (DoS guard) but worth noting.

## REQ-§6.8-17 — Lowercase directive names treated as reserved (case-sensitive `YAML` and `TAG`)

- **Spec requirement (§6.8):** The two defined directives
  are `YAML` and `TAG` (uppercase, in the spec
  productions). Other names are reserved.
- **Test method:** Two probes feeding `%yaml` and `%tag`.
- **Test inputs:**
  - `%yaml 1.2\n---\nfoo\n`
  - `%tag !e! tag:a:\n---\n!e!t x\n`
- **Observed output:**
  - `%yaml`: clean parse; `DocumentStart.version: None`
    (treated as reserved/ignored).
  - `%tag`: directive ignored; `DocumentStart.tag_directives:
    []`; the body's `!e!t` then errors with `undefined tag
    handle: !e!`.
- **Spec expectation:** Case-sensitive. Lowercase forms are
  reserved.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:95-103`
  (`match name { "YAML" | "TAG" | _ => ... }` — exact
  string match is case-sensitive).
- **Reasoning:** The spec's directive name productions are
  literal strings `"YAML"` and `"TAG"`. The parser's
  `match` is byte-exact; lowercase variants land in the
  reserved branch. Consistent with the spec.

## REQ-§6.8-18 — Directive name token allows arbitrary bytes (incl. NUL) up to first whitespace

- **Spec requirement (§6.8 / [84] ns-directive-name):**
  `ns-directive-name ::= ns-char+`. `ns-char` excludes
  whitespace, BOM, and (per §5.1) the NUL byte and other
  c0 control codes.
- **Test method:** Standalone probe; fed
  `%YAML\x00 1.2\n---\nfoo\n` and `%FOO\x00 bad\n---\nfoo\n`.
- **Test inputs:** As above (NUL embedded in name token).
- **Observed output:**
  - `%YAML\x00 1.2`: `DocumentStart.version: None` —
    parser interpreted the directive name as `YAML\x00`
    (does not match `"YAML"`), routed to the reserved
    branch, ignored silently. Body parses cleanly.
  - `%FOO\x00 bad`: Same — name `FOO\x00`, reserved branch,
    ignored.
- **Spec expectation:** `ns-char+` excludes NUL; the
  directive name should be ill-formed and produce an error
  (or at minimum a warning).
- **Verdict:** Lenient. (Phase 1 [84] flagged the same.)
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:88-92`
  — `find([' ', '\t'])` extracts the name without
  validating individual characters against `ns-char`. NUL,
  control bytes, BOM, etc. are accepted into the name
  token.
- **Reasoning:** The parser treats the name as an opaque
  byte slice from `%` to the first space/tab. The spec's
  `ns-char+` constraint is not enforced. Real-world impact
  is small (real authors would not embed NUL in names),
  but a strict reading of the BNF marks this Lenient.
  Conformance doc claims [84] "Conformant" — this audit
  disagrees because behavioral testing reveals the
  validation gap.

## REQ-§6.8-19 — Directive parameter token allows arbitrary bytes (incl. NUL) up to whitespace

- **Spec requirement (§6.8 / [85] ns-directive-parameter):**
  `ns-directive-parameter ::= ns-char+`. NUL and other
  control bytes excluded.
- **Test method:** Standalone probe; fed
  `%FOO param\x00\n---\nfoo\n` and `%FOO ab\x01cd\n---\nfoo\n`.
- **Test inputs:** As above.
- **Observed output:**
  - `%FOO param\x00`: clean parse; reserved directive
    silently absorbs; body parses. NUL is tolerated.
  - `%FOO ab\x01cd`: same — control byte tolerated.
  - `%FOO café\n` (non-ASCII multibyte): also tolerated
    silently.
- **Spec expectation:** Parameters with NUL/control bytes
  should be flagged as invalid `ns-char+`.
- **Verdict:** Lenient. (Phase 1 [85] flagged the same.)
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:93,
  100-102` — for reserved directives, the parameter blob is
  not inspected at all. For `%TAG` and `%YAML`,
  parameter validation is downstream and shape-specific
  (e.g. `parse::<u8>()` for version), but byte-level
  `ns-char+` is not enforced at the directive layer.
- **Reasoning:** Reserved directives are the most lenient
  case — they accept any bytes between the name and the
  newline. Conformance doc claims [85] "Conformant" —
  this audit disagrees.

## REQ-§6.8-20 — `s-separate-in-line` (space or tab) separator accepted before parameters

- **Spec requirement (§6.8):** Directive name and
  parameters are separated by `s-separate-in-line` =
  `s-white+` where `s-white = SPACE | TAB`.
- **Test method:** Standalone probe; tabs in place of
  spaces.
- **Test inputs:**
  - `%YAML\t1.2\n---\nfoo\n`
  - `%TAG\t!\t!\n---\n!t x\n`
  - `%YAML\t1.2\t#c\n---\nfoo\n` (tab + trailing
    comment)
- **Observed output:** All clean parses;
  `DocumentStart.version: Some((1, 2))` and tag handle
  registered correctly. Trailing tab+comment tolerated.
- **Spec expectation:** Tabs accepted as separators.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:88-93,
  165-170` (`find([' ', '\t'])` and
  `trim_start_matches([' ', '\t'])`).

## REQ-§6.8-21 — Trailing comment after `%YAML` accepted

- **Spec requirement (§6.8 / [82] l-directive):**
  `l-directive ::= c-directive (...) s-l-comments`. A
  comment may follow on the directive line.
- **Test method:** Standalone probe; fed
  `%YAML 1.2 # version\n---\nfoo\n`.
- **Test input:** `%YAML 1.2 # version\n---\nfoo\n`
- **Observed output:** Clean parse;
  `DocumentStart.version: Some((1, 2))`.
- **Spec expectation:** Trailing comment accepted.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:122-134`
  — explicit check `if !trailing.is_empty() &&
  !trailing.starts_with('#')` allows comment-or-empty.

## REQ-§6.8-22 — Trailing junk after `%YAML 1.2` (non-comment) rejected

- **Spec requirement (§6.8.1):** `ns-yaml-directive ::=
  "YAML" s-separate-in-line ns-yaml-version`. Anything
  beyond is `s-l-comments` only.
- **Test method:** Standalone probe; fed `%YAML 1.2 foo`.
- **Test input:** `%YAML 1.2 foo\n---\nfoo\n`
- **Observed output:** `Err { message: "malformed %YAML
  directive: unexpected trailing content \"foo\"" }`.
- **Spec expectation:** Reject (or at minimum, fail to
  bind any version).
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:127-134`.

## REQ-§6.8-23 — `%TAG` comment-after-prefix not separated by space treated as part of prefix

- **Spec requirement (§6.8.2):** `ns-tag-directive ::=
  "TAG" s-separate-in-line c-tag-handle s-separate-in-line
  ns-tag-prefix`, with `s-l-comments` optional after.
  `ns-tag-prefix` is `c-ns-local-tag-prefix |
  ns-global-tag-prefix`; both terminate at whitespace.
- **Test method:** Standalone probe; fed
  `%TAG ! ! # primary\n---\n!t x\n`.
- **Test input:** `%TAG ! ! # primary\n---\n!t x\n`
- **Observed output:** `DocumentStart.tag_directives:
  [("!", "! # primary")]` — the entire trailing string
  is captured into the prefix. The body `!t x` resolves
  using prefix `! # primary` (concatenation of local
  prefix + suffix `t`), yielding the literal local tag
  expansion.
- **Spec expectation:** The `ns-tag-prefix` ends at the
  next `s-separate-in-line`; the `# primary` portion
  belongs to `s-l-comments`, not the prefix.
- **Verdict:** Lenient.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:165-170`
  — `find([' ', '\t'])` extracts handle, then everything
  after (until newline) is taken as prefix verbatim. No
  comment-aware splitting.
- **Reasoning:** This is the same pattern probed in R6.F
  via `%TAG ! !`. The prefix `! # primary` contains
  spaces, which `ns-tag-prefix` (a single non-whitespace
  token of `c-tag` + `ns-uri-char*`) explicitly forbids.
  The control-char check on prefix
  (`directives.rs:208-215`) does not flag spaces. This
  produces a corrupt-looking but technically lossless
  store. Most users will not author this, but it is a
  behavioral leniency from the spec's prescription.
  Conformance doc [88]/[93] claims "Conformant" —
  disagree.

## REQ-§6.8-24 — `%TAG` malformed handle `!foo` (missing trailing `!`) rejected

- **Spec requirement (§6.8.2 / [89]–[92]):** Tag handle
  must be `!`, `!!`, or `!<word-chars>!`.
- **Test method:** Standalone probe; fed `%TAG !foo
  tag:a:`.
- **Test input:** `%TAG !foo tag:a:\n---\nfoo\n`
- **Observed output:** `Err { message: "malformed %TAG
  handle: \"!foo\" is not a valid tag handle" }`.
- **Spec expectation:** Reject (handle is malformed).
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:281-295`.

## REQ-§6.8-25 — `%TAG` named handle with underscore rejected (`ns-word-char` is `[a-zA-Z0-9-]`)

- **Spec requirement (§6.8.2 / [92] c-named-tag-handle):**
  `ns-word-char ::= ns-dec-digit | ns-ascii-letter | '-'`
  (no underscore).
- **Test method:** Standalone probe; fed `%TAG !my_ns!
  tag:a:`.
- **Test input:** `%TAG !my_ns! tag:a:\n---\nfoo\n`
- **Observed output:** `Err { message: "malformed %TAG
  handle: \"!my_ns!\" is not a valid tag handle" }`.
- **Spec expectation:** Reject.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:285-294`.

## REQ-§6.8-26 — `%TAG` named handle with hyphen accepted

- **Spec requirement (§6.8.2 / [92]):** `-` is a valid
  `ns-word-char`.
- **Test method:** Standalone probe; fed `%TAG !my-ns!
  tag:a:` then `!my-ns!t x`.
- **Test input:** `%TAG !my-ns! tag:a:\n---\n!my-ns!t x\n`
- **Observed output:** Clean parse;
  `DocumentStart.tag_directives: [("!my-ns!", "tag:a:")]`;
  scalar parses.
- **Spec expectation:** Accept; resolve.
- **Verdict:** Strict-conformant.
- **Evidence:** `properties.rs:289`.

## REQ-§6.8-27 — `%TAG` missing prefix rejected

- **Spec requirement (§6.8.2):** Prefix is required; the
  production demands `c-tag-handle s-separate-in-line
  ns-tag-prefix`.
- **Test method:** Standalone probe; multiple forms.
- **Test inputs:**
  - `%TAG !\n---\nfoo\n` (handle only)
  - `%TAG\n---\nfoo\n` (nothing after name)
  - `%TAG    \n---\nfoo\n` (whitespace only)
- **Observed output:**
  - `%TAG !`: `Err { message: "malformed %TAG directive:
    expected 'handle prefix', got \"!\"" }`.
  - `%TAG`/`%TAG    `: `Err { message: "malformed %TAG
    directive: expected 'handle prefix', got \"\"" }`.
- **Spec expectation:** Reject.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:165-176`.

## REQ-§6.8-28 — `%YAML` directive without `---` marker rejected

- **Spec requirement (§9.1.2):** Directives must be
  followed by a `---` marker.
- **Test method:** Standalone probe; two forms.
- **Test inputs:**
  - `%YAML 1.2\n` (EOF)
  - `%YAML 1.2\nfoo\n` (content but no marker)
- **Observed output:** Both yield `Err { message:
  "directives must be followed by a '---' document-start
  marker" }`.
- **Spec expectation:** Reject.
- **Verdict:** Strict-conformant on §9.1.2 (this is the
  enforcement point for that rule).
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directives.rs:271-282,
  309-321, 327-337`.

## Summary

Tally of verdicts for §6.8:

| Verdict | Count | REQs |
|---|---|---|
| Strict-conformant | 22 | 1, 2, 3, 4, 5 (must-half), 8, 9, 10, 11, 12, 13, 14, 15, 17, 20, 21, 22, 24, 25, 26, 27, 28 |
| Stricter-than-spec | 3 | 6, 7, 16 |
| Lenient | 4 | 5 (should-half), 18, 19, 23 |
| Indeterminate | 0 | — |

REQ-§6.8-5 is split-classified because the requirement
contains both a "must accept" half (Strict-conformant)
and a "should adjust" half (Lenient).

Phase 1 findings re-confirmed behaviorally:
- `[83]` "should ignore" → confirmed silent-ignore matches
  intent (REQ-15); RFC 2119 "should" reading holds.
- `[84]`/`[85]` parameter validation → confirmed Lenient
  (REQ-18, REQ-19); NUL and control bytes flow through
  the name and parameter tokens unchecked.
- `[86]` `major == 0` rejection → confirmed
  Stricter-than-spec (REQ-6).
- `[87]` digit overflow at u8 boundary → confirmed
  Stricter-than-spec (REQ-7); minor-version overflow
  case is the more meaningful divergence because the
  spec says "should adjust" not "must reject."

Conformance-doc disagreements (audit's findings vs doc's
"Conformant" classification):
- [83] doc=Conformant; audit=Strict-conformant (agrees).
- [84] doc=Conformant; audit=**Lenient** for ns-char+
  enforcement.
- [85] doc=Conformant; audit=**Lenient** for ns-char+
  enforcement.
- [86] doc=Conformant; audit=**Stricter-than-spec** for
  `major != 1` rejecting major=0.
- [87] doc=Conformant; audit=**Stricter-than-spec** for
  u8 overflow at digit count > 3.
- [88]/[93] doc=Conformant; audit=**Lenient** on
  `ns-tag-prefix` boundary (REQ-23) — comment-after-prefix
  without proper separator absorbs into prefix.

No probe code remained in the parser tree at completion.
Probes lived in `/tmp/audit-probe-6.8-b/` (a separate
Cargo project outside the workspace).
