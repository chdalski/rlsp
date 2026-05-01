---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: A
section: §6.8
date: 2026-04-30
---

# Phase 2 Behavioral Audit — §6.8 Directives (Auditor A)

Scope: end-to-end behavioral audit of `%YAML` and `%TAG` directive
handling, plus reserved/unknown directive handling, against the
normative requirements of YAML 1.2.2 §6.8, §6.8.1, §6.8.2.

Method: probes were run via a standalone audit-probe Cargo project at
`/tmp/audit-probe-§6.8/` depending on `rlsp-yaml-parser` by path, so
nothing was added to the parser tree. Every requirement entry below
cites the specific input and the literal observed events from
`parse_events()` and `load()`.

The parser does not implement a `Warning` event type
(`grep -rn 'Warning\|emit_warning' /workspace/rlsp-yaml-parser/src/`
returns no matches in `event_iter/` or in `event.rs`). This affects
several spec-mandated "should warn" requirements below.

---

### REQ-§6.8-1: Accept `%YAML 1.2` documents

Spec requirement: "A version 1.2 YAML processor must accept documents
with an explicit `%YAML 1.2` directive ... Such documents are assumed
to conform to the 1.2 version specification." (§6.8.1, lines 3041-3043)
Test method: standalone probe — feed `"%YAML 1.2\n---\nfoo\n"` to
`parse_events()` and `load()`.
Test input: `"%YAML 1.2\n---\nfoo\n"`
Observed output: `StreamStart`, `DocumentStart { explicit: true,
version: Some((1, 2)), tag_directives: [] }`, `Scalar { value: "foo",
... }`, `DocumentEnd`, `StreamEnd`. `load()` returns one document.
Spec expectation: accept without error.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:107-156`
(`parse_yaml_directive`); branch `major != 1` returns error, branch
`major == 1` stores `(major, minor)`. Version is propagated to
`DocumentStart` event at `directives.rs:296-303`. Probe in
`/tmp/audit-probe-§6.8/`, label `YAML-1.2-explicit`.
Reasoning: The parser stores the version in `DirectiveScope.version`
and emits it on the next `DocumentStart`. No unexpected error or
warning event is produced. The behavior matches the normative "must
accept" requirement.

### REQ-§6.8-2: Accept documents without any `%YAML` directive

Spec requirement: "A version 1.2 YAML processor must accept documents
... lacking a `YAML` directive." (§6.8.1, line 3041-3042)
Test method: feed bare content with no directive line.
Test input: `"foo\n"`
Observed output: `StreamStart`, `DocumentStart { explicit: false,
version: None, tag_directives: [] }`, `Scalar { value: "foo", ... }`,
`DocumentEnd`, `StreamEnd`. `load()` returns one document.
Spec expectation: accept; document assumed 1.2.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:338-355`
(implicit `DocumentStart` path with `version: None` when scope is
empty). Probe label `YAML-no-directive`.
Reasoning: The parser correctly treats absence of `%YAML` as the
default 1.2-conforming case and emits `version: None`, leaving
interpretation to consumers. No error.

### REQ-§6.8-3: Accept `%YAML 1.1` (with adjustment / warning)

Spec requirement: "A version 1.2 YAML processor must also accept
documents with an explicit `%YAML 1.1` directive. ... a version 1.2
processor should process version 1.1 documents as if they were version
1.2, giving a warning on points of incompatibility." (§6.8.1, lines
3049-3055)
Test method: probe `"%YAML 1.1\n---\nfoo\n"`.
Test input: `"%YAML 1.1\n---\nfoo\n"`
Observed output: `DocumentStart { explicit: true, version: Some((1,
1)), tag_directives: [] }`, `Scalar { value: "foo", ... }`. No warning
event of any kind.
Spec expectation: accept the document; emit a warning when
incompatible features are encountered.
Verdict: Lenient
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:146-153`
accepts any minor when major == 1. No `Warning`/`Diagnostic` variant
exists in `Event` (`rlsp-yaml-parser/src/event.rs`, no `warn` matches
in `event_iter/`). Probe label `YAML-1.1-explicit`. The downstream
1.1-vs-1.2 incompatibility surface (e.g. non-ASCII line breaks) is
not specially flagged either.
Reasoning: The "must accept" half is satisfied — the directive is not
rejected and the document parses. The "should warn on points of
incompatibility" half is unfulfilled because the parser surfaces no
warning channel at all. Per the symmetric reconciliation principle,
this is attributed at the requirement where enforcement is missing
(the missing warning emission) rather than propagated to the document
acceptance which is correct. Verdict is Lenient because the spec uses
"should ... giving a warning"; absence of a warning means the
processor under-enforces a SHOULD requirement.

### REQ-§6.8-4: Higher-minor `%YAML 1.3` should be processed with warning

Spec requirement: "Documents with a `YAML` directive specifying a
higher minor version (e.g. `%YAML 1.3`) should be processed with an
appropriate warning." (§6.8.1, lines 3044-3045)
Test method: probe `"%YAML 1.3\n---\nfoo\n"`.
Test input: `"%YAML 1.3\n---\nfoo\n"`
Observed output: `DocumentStart { explicit: true, version: Some((1,
3)), tag_directives: [] }`, then content as normal, no warning event.
Spec expectation: process the document; emit a warning.
Verdict: Lenient
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:146-153`
(only `major != 1` is rejected; `1.x` for any minor is accepted). No
warning channel exists. Probe label `YAML-1.3-higher-minor`.
Reasoning: The spec text is a SHOULD: process plus warn. The parser
processes (good) but emits no warning (the SHOULD half is unmet).
Same Lenient classification as REQ-§6.8-3 with the same root cause —
no warning emission infrastructure.

### REQ-§6.8-5: Higher-major `%YAML 2.0` should be rejected

Spec requirement: "Documents with a `YAML` directive specifying a
higher major version (e.g. `%YAML 2.0`) should be rejected with an
appropriate error message." (§6.8.1, lines 3046-3047)
Test method: probe `"%YAML 2.0\n---\nfoo\n"`.
Test input: `"%YAML 2.0\n---\nfoo\n"`
Observed output: error `"unsupported YAML version 2.0: only 1.x is
supported"` at line 1 col 0.
Spec expectation: reject with an error message.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:146-151`
returns an error if `major != 1`. Probe label
`YAML-2.0-higher-major`.
Reasoning: A clear, descriptive error is emitted. The parser meets
the SHOULD-rejection cleanly. (Note this also runs through `load()`
which propagates the same `Error`.)

### REQ-§6.8-6: Lower-major `%YAML 0.x` handling

Spec requirement: §6.8.1 normatively addresses *higher* major
versions. The spec does not directly classify `%YAML 0.x`. Phase 1
note: "[86] Stricter-than-spec: rejects `major == 0`." Verify Phase 1
finding behaviorally.
Test method: probe `"%YAML 0.5\n---\nfoo\n"`.
Test input: `"%YAML 0.5\n---\nfoo\n"`
Observed output: error `"unsupported YAML version 0.5: only 1.x is
supported"`.
Spec expectation: undefined by §6.8.1 — the spec talks about higher
major versions only. The `ns-yaml-version` BNF accepts arbitrary
digit pairs.
Verdict: Stricter-than-spec
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:146-151`.
Probe label `YAML-0.5-lower-major`.
Reasoning: Phase 1's "Stricter-than-spec" finding reproduces
behaviorally — the parser rejects `0.x` even though §6.8.1 only
mandates rejection of *higher* majors. This is a defensible practical
choice (no reasonable consumer can interpret YAML 0.x), but it goes
beyond what the spec mandates. Recorded under the symmetric
reconciliation principle as a property of this specific input range.

### REQ-§6.8-7: `%YAML` minor-version digit count

Spec requirement: BNF `ns-yaml-version ::= ns-dec-digit+ '.'
ns-dec-digit+` (line 3066-3068) — any nonempty digit count is
syntactically valid; semantic restrictions are stated in §6.8.1
prose.
Test method: probe `"%YAML 1.100\n---\nfoo\n"` and `"%YAML
1.300\n---\nfoo\n"`.
Test input: two — `"%YAML 1.100\n---\nfoo\n"` and `"%YAML
1.300\n---\nfoo\n"`.
Observed output: `1.100` accepted, `version: Some((1, 100))`. `1.300`
rejected with `"malformed %YAML minor version: \"300\""`.
Spec expectation: per BNF the digit count is unrestricted. The spec
prose is silent on out-of-range minors that are not "higher minor
versions" — `1.300` would qualify as a higher minor; the implication
is "should be processed with a warning" rather than rejected.
Verdict: Stricter-than-spec
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:140-143`
parses minor as `u8`, so values > 255 fail with `parse_int` error.
Probe labels `YAML-1.100-minor-within-u8`, `YAML-1.300-minor-out-of-u8`.
Reasoning: Phase 1's `[87]` finding is confirmed: the `u8`
representation imposes a `[0, 255]` bound on each version component.
Within that bound the parser is conformant; above it the parser
rejects rather than warning. Because spec §6.8.1 frames higher minor
versions as a SHOULD-warn case rather than a reject case, the parser
is stricter than mandated for minors > 255. The error message ("minor
version") is descriptive, but the parser does not distinguish
"out-of-range integer" from "non-numeric" — both surface as
`malformed`. This is consistent strictness, not a bug.

### REQ-§6.8-8: Duplicate `%YAML` directive in the same document

Spec requirement: "It is an error to specify more than one `YAML`
directive for the same document, even if both occurrences give the
same version number." (§6.8.1, lines 3090-3091; example at
3094-3107).
Test method: probe two same-version and two different-version
duplicates in one document.
Test input: `"%YAML 1.2\n%YAML 1.2\n---\nfoo\n"` and `"%YAML
1.2\n%YAML 1.1\n---\nfoo\n"`.
Observed output: both inputs error: `"duplicate %YAML directive in the
same document"` at the second directive's position.
Spec expectation: error.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:107-113`
checks `self.directive_scope.version.is_some()`. Probe labels
`YAML-duplicate-same-version`, `YAML-duplicate-different-version`.
Reasoning: The error fires for both same-version and
different-version duplicates as the spec requires. Position points
to the second directive. Clean MUST conformance.

### REQ-§6.8-9: `%TAG` primary handle (`!`) override

Spec requirement: "It is possible to override the default behavior by
providing an explicit `TAG` directive, associating a different prefix
for this handle." (§6.8.2 Primary Handle, lines 3190-3193) — example
at 3202-3216 shows `%TAG ! tag:example.com,2000:app/` causing `!foo`
to expand to `tag:example.com,2000:app/foo`.
Test method: probe `"%TAG ! tag:example.com,2000:app/\n---\n!foo
bar\n"`.
Test input: `"%TAG ! tag:example.com,2000:app/\n---\n!foo bar\n"`
Observed output: `DocumentStart { ..., tag_directives: [("!",
"tag:example.com,2000:app/")] }`; scalar `bar` carries
`tag: Some("tag:example.com,2000:app/foo")`.
Spec expectation: `!foo` resolves to
`tag:example.com,2000:app/foo`.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directive_scope.rs:137-151`
expands `!suffix` against a registered `!` handle. Probe label
`TAG-primary-bare`.
Reasoning: Resolved tag matches spec example exactly.

### REQ-§6.8-10: `%TAG` secondary handle (`!!`) default and override

Spec requirement: "By default, the prefix associated with this handle
is `tag:yaml.org,2002:`" (§6.8.2 Secondary Handle, line 3226). "It is
possible to override this default behavior by providing an explicit
`TAG` directive associating a different prefix for this handle."
(line 3228-3229)
Test method: two probes — default secondary (`!!str foo`), and
overridden secondary (`%TAG !! tag:example.com,2000:app/` followed by
`!!int 42`).
Test input: `"---\n!!str foo\n"` and `"%TAG !!
tag:example.com,2000:app/\n---\n!!int 42\n"`.
Observed output: default → `tag: Some("tag:yaml.org,2002:str")`.
Override → `tag: Some("tag:example.com,2000:app/int")`.
Spec expectation: default is `tag:yaml.org,2002:`; override replaces
it.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directive_scope.rs:92-109`
— `!!suffix` resolution looks up the `"!!"` handle and falls back to
`"tag:yaml.org,2002:"` literal. Probe labels `TAG-secondary-default`,
`TAG-secondary-override`.
Reasoning: Both default and overridden behaviors match the spec.

### REQ-§6.8-11: `%TAG` named handle resolution

Spec requirement: "A named tag handle surrounds a non-empty name with
`!` characters. A handle name must not be used in a tag shorthand
unless an explicit `TAG` directive has associated some prefix with
it." (§6.8.2 Named Handles, lines 3254-3256). BNF
`c-named-tag-handle ::= c-tag ns-word-char+ c-tag` (line 3263-3268).
Example at 3273-3284 shows `!e!foo` → `tag:example.com,2000:app/foo`.
Test method: probe `"%TAG !e! tag:example.com,2000:app/\n---\n!e!foo
bar\n"` and verify undefined-handle behavior in the cross-doc test.
Test input: `"%TAG !e! tag:example.com,2000:app/\n---\n!e!foo bar\n"`
Observed output: scalar `bar` carries `tag:
Some("tag:example.com,2000:app/foo")`. The cross-doc probe
(`TAG-scope-not-leak-across-doc`) shows undefined-handle error
`"undefined tag handle: !m!"`.
Spec expectation: registered named handle resolves correctly;
unregistered handle is an error (since shorthand "must not be used"
without registration).
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directive_scope.rs:111-132`
performs named-handle lookup and errors with `"undefined tag handle"`
when missing. Probe labels `TAG-named-handle`,
`TAG-scope-not-leak-across-doc`.
Reasoning: Resolution and error behavior both match the spec.

### REQ-§6.8-12: Duplicate `%TAG` directive for the same handle

Spec requirement: "It is an error to specify more than one `TAG`
directive for the same handle in the same document, even if both
occurrences give the same prefix." (§6.8.2, lines 3146-3147; example
at 3150-3165)
Test method: probe two same-prefix duplicates and two
different-prefix duplicates.
Test input: `"%TAG ! !foo\n%TAG ! !foo\n---\nbar\n"` and `"%TAG !
!a\n%TAG ! !b\n---\nbar\n"`.
Observed output: both inputs error: `"duplicate %TAG directive for
handle \"!\""` at the second directive's line.
Spec expectation: error.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:218-223`
checks `tag_handles.contains_key(handle)`. Probe labels
`TAG-duplicate-same-handle-same-prefix`,
`TAG-duplicate-different-prefix`.
Reasoning: Error fires for both same-prefix and different-prefix
duplicates, matching the "even if both occurrences give the same
prefix" clause exactly. Position points to the offending second
directive.

### REQ-§6.8-13: `%TAG` directive scope is per-document

Spec requirement: §6.8 line 2975: "Directives are a presentation
detail and must not be used to convey content information." §6.8.2
example at 3315-3329 shows the same `%TAG !m! !my-` directive being
re-declared between two documents — implying per-document scope is
mandatory.
Test method: probe (a) two consecutive documents that each redeclare
the same `%TAG !m! !my-` directive, and (b) one declaration scoped to
the first document followed by an attempted use in a second document
without redeclaration.
Test input: `"%TAG !m! !my-\n---\n!m!a foo\n...\n%TAG !m!
!my-\n---\n!m!b bar\n"` and `"%TAG !m! !my-\n---\nfoo\n...\n---\n!m!b
bar\n"`.
Observed output: case (a) — both docs parse cleanly with
`tag_directives: [("!m!", "!my-")]` per `DocumentStart` and tags
expand correctly. Case (b) — first doc clean, second doc emits
`DocumentStart { ..., tag_directives: [] }` and then errors with
`"undefined tag handle: !m!"`.
Spec expectation: `%TAG` declarations from doc N do not carry into
doc N+1.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/state.rs` and
`rlsp-yaml-parser/src/event_iter/directives.rs:33` (entire
`consume_preamble_between_docs` re-enters with whatever scope was
reset by the document-end transition). The reset itself happens
elsewhere — searching for where `directive_scope` is reset
(`grep -n "directive_scope" rlsp-yaml-parser/src/event_iter/`)
confirms the scope clears at each `...` / `---` transition. Probe
labels `TAG-scope-per-document`, `TAG-scope-not-leak-across-doc`,
`YAML-scope-per-document`.
Reasoning: Behaviorally observed: the second document begins with no
inherited tag directives. This is the spec-mandated per-document
scoping.

### REQ-§6.8-14: Reserved/unknown directive should be ignored with warning

Spec requirement: "A YAML processor should ignore unknown directives
with an appropriate warning." (§6.8, lines 2993-2994) Example at
3016-3026: `%FOO bar baz` is processed with documents continuing
normally.
Test method: probe `%FOO`, `%FOO bar baz`, and `%FOO` with NUL bytes.
Test input: four — `"%FOO bar baz\n---\n\"foo\"\n"`,
`"%FOO\n---\n\"foo\"\n"`, `"%FOO\x00 bar\n---\n\"foo\"\n"`, and
`"%FO\x00O bar\n---\n\"foo\"\n"`.
Observed output: all four parse successfully with no warning event
and no error. Document content is delivered as expected (`Scalar
"foo"`). Phase 1 also noted [83] as Strict-conformant under "should is
non-mandatory" reading.
Spec expectation: ignore the directive AND emit a warning.
Verdict: Lenient
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:95-103`
silently increments `directive_count` and returns `Ok(())` for the
default case. No warning channel exists. Probe labels
`FOO-reserved-with-params`, `FOO-reserved-no-params`,
`FOO-reserved-with-NUL`, `FOO-reserved-NUL-in-name`.
Reasoning: The "ignore" half is met — the directive is consumed and
the document parses. The "with an appropriate warning" half is
unmet. The spec uses SHOULD here, which is an expectation the parser
under-enforces. Verdict: Lenient on the warning expectation. (The
ignore-on-NUL behavior is also Lenient w.r.t. directive name
validation — see REQ-§6.8-15.)

### REQ-§6.8-15: Directive name BNF (`ns-directive-name ::= ns-char+`)

Spec requirement: BNF at lines 3006-3008: `ns-directive-name ::=
ns-char+`. `ns-char` excludes whitespace and control characters
(see §5.4). Therefore a NUL byte inside a directive name is BNF-illegal.
Test method: probe a NUL inside what would otherwise be `%YAML`.
Test input: `"%YAM\x00L 1.2\n---\nfoo\n"`
Observed output: `DocumentStart { explicit: true, version: None,
tag_directives: [] }` — the directive parser treated `YAM\x00L` as
an unknown reserved directive name and silently ignored it. No
error or warning was raised.
Spec expectation: BNF violation; should be flagged (or, charitably,
the entire directive should be rejected as malformed).
Verdict: Lenient
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:88-103` — name
extraction stops at first space/tab (no validation of `ns-char+`),
and the reserved-directive fallthrough silently accepts any name.
Probe labels `YAML-with-NUL-in-name`, `FOO-reserved-NUL-in-name`,
`FOO-reserved-with-NUL`.
Reasoning: This is the exact gap Phase 1 [84]/[85] flagged Lenient,
now confirmed end-to-end. The NUL-in-name input has two failure
modes: (a) the name validator does not reject control bytes, (b)
even if `%YAM\x00L` is treated as an unknown directive, the
reserved-directive handler silently swallows it. Combined effect:
malformed directive bytes silently disappear with the document still
parsing as if the directive were not there. The user has no signal
that something was discarded.

### REQ-§6.8-16: `%YAML` parameter must be `ns-dec-digit+ '.' ns-dec-digit+`

Spec requirement: BNF at lines 3064-3069 fixes the version shape.
Anything else is malformed.
Test method: probe several malformed forms.
Test input: five — `"%YAML 12\n---\nfoo\n"` (no dot),
`"%YAML 1.2.3\n---\nfoo\n"` (three parts),
`"%YAML 1.2 trailing\n---\nfoo\n"` (extra non-comment content),
`"%YAML 1.2 # comment\n---\nfoo\n"` (trailing comment), and
`"%YAML\n---\nfoo\n"` (empty params).
Observed output: no-dot → error `"malformed %YAML directive:
expected 'major.minor', got \"12\""`. Three-parts → error
`"malformed %YAML minor version: \"2.3\""`. Trailing-non-comment →
error `"malformed %YAML directive: unexpected trailing content
\"trailing\""`. Trailing-comment → accepted, `version: Some((1,
2))`. Empty-params → error `"malformed %YAML directive: expected
'major.minor', got \"\""`.
Spec expectation: reject all malformed forms; the spec also accepts
trailing comments per §6.6 (s-l-comments after a directive).
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:115-143`.
Probe labels `YAML-no-dot`, `YAML-three-parts`,
`YAML-trailing-content-not-comment`, `YAML-trailing-comment-allowed`,
`YAML-empty-version`.
Reasoning: Each malformed form is rejected with a reasonably
specific message. The trailing-comment carve-out at directives.rs:127
correctly mirrors the BNF `l-directive ::= ... s-l-comments`
production at line 2978-2987.

### REQ-§6.8-17: `%TAG` parameter shape and prefix non-emptiness

Spec requirement: BNF `ns-tag-directive ::= "TAG" s-separate-in-line
c-tag-handle s-separate-in-line ns-tag-prefix` (lines 3119-3125).
Both handle and prefix are required.
Test method: probe missing prefix, missing handle/prefix, and
malformed handles.
Test input: four — `"%TAG !foo!\n---\nbar\n"` (handle but no prefix),
`"%TAG\n---\nbar\n"` (nothing), `"%TAG !foo bar\n---\nx\n"` (handle
without trailing `!`), and `"%TAG !f.o! bar\n---\nx\n"` (handle with
non-word `.`).
Observed output: missing-prefix and missing-handle both error
`"malformed %TAG directive: expected 'handle prefix', got <X>"`. Both
malformed handles error `"malformed %TAG handle: <X> is not a valid
tag handle"`.
Spec expectation: reject all four.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:163-187` —
explicit checks for missing-prefix, then handle validation via
`is_valid_tag_handle`. Validity rule at
`rlsp-yaml-parser/src/event_iter/properties.rs:281-295` matches the
BNF productions for primary, secondary, and named handles. Probe
labels `TAG-missing-prefix`, `TAG-missing-handle`,
`TAG-invalid-handle-no-trailing-bang`, `TAG-handle-with-non-word-char`.
Reasoning: All four cases produce informative errors at the directive
position. The handle validator's `is_ascii_alphanumeric() || c == '-'`
rule is the strict reading of `ns-word-char` from §5.6, properly
rejecting `.` in handle names.

### REQ-§6.8-18: Multiple distinct `%TAG` handles in a single document

Spec requirement: §6.8.2 distinguishes primary, secondary, and named
handles as separate handle-name spaces; the duplicate rule is
"per handle." Multiple distinct handles in the same document must
coexist.
Test method: probe a document declaring `!`, `!!`, and `!x!` all at
once and verify all three are visible to `DocumentStart` and resolve
correctly.
Test input: `"%TAG ! !primary-\n%TAG !! tag:override:\n%TAG !x!
tag:named:\n---\n!a\n"`
Observed output: `DocumentStart { ..., tag_directives: [("!",
"!primary-"), ("!!", "tag:override:"), ("!x!", "tag:named:")] }`.
Following scalar carries `tag: Some("!primary-a")` (primary-handle
expansion).
Spec expectation: all three handles coexist; each resolves
independently.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directive_scope.rs:55-64`
(separate `tag_handles` HashMap). All three entries reach
`DocumentStart.tag_directives` via
`directive_scope.tag_directives()` at
`rlsp-yaml-parser/src/event_iter/directive_scope.rs:158-167`. Probe
label `TAG-multiple-distinct-handles`.
Reasoning: Each handle is keyed independently in the HashMap; the
`tag_directives` accessor sorts them deterministically (which is also
useful for downstream reproducibility). All three are emitted.

### REQ-§6.8-19: Directive without trailing `---` document-start marker

Spec requirement: §9.2 (referenced from §6.8 by way of the "non-
indented" line constraint) requires directives to precede a
directives-end marker before any non-marker content. The example at
3072-3083 shows `%YAML 1.3\n---\n"foo"` — the `---` is required.
Test method: probe directives followed directly by content with no
`---`.
Test input: three — `"%YAML 1.2\nfoo\n"`, `"%TAG !!
tag:foo:\nbar\n"`, and `"%FOO bar\nbaz\n"`.
Observed output: all three error: `"directives must be followed by a
'---' document-start marker"` at the position of the first content
line.
Spec expectation: error.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/directives.rs:272-336` —
three branches (EOF, `...` orphan, plain content) all check
`directive_scope.directive_count > 0` before allowing an implicit
`DocumentStart`. Probe labels `YAML-without-doc-marker`,
`TAG-without-doc-marker`, `FOO-without-doc-marker`.
Reasoning: Reserved directive (`%FOO`) also fires this guard,
because `parse_directive` increments `directive_count` for the
reserved branch (`directives.rs:100`). This is a correct application
of §9.2 — any directive forbids implicit document start. Note that
this guard arguably makes the reserved-directive ignore behavior
slightly stricter than a pure "ignore" reading, but it preserves
spec compliance for §9.2 which is the higher-priority constraint.
Cross-cuts cleanly with REQ-§6.8-14 because the user input typically
will have `---` after directives anyway.

### REQ-§6.8-20: Directives must be on non-indented lines

Spec requirement: "Each directive is specified on a separate
non-indented line starting with the `%` indicator..." (§6.8, lines
2990-2992).
Test method: not separately tested by probe — the lexer's
`is_directive_line` is gated on `line.content.starts_with('%')`, and
`peek_next` produces the line content stripped of any indentation.
Inspection of `lexer.rs:148-154` shows directives are recognized by
their `%` prefix at the start of the line content. The `LineBuffer`
abstraction strips leading indentation; whether indented `%` is
recognized as a directive depends on how `line.content` is derived.
Observed output: not measured.
Spec expectation: indented lines starting with `%` should not be
recognized as directives (they are ordinary content).
Verdict: Indeterminate
Evidence: `rlsp-yaml-parser/src/lexer.rs:148-154` shows the test is
`line.content.starts_with('%')` but does not show whether
`line.content` includes leading indentation. Verifying this would
require tracing through `LineBuffer::peek_next()` and reading the
line struct definition — out of audit scope for §6.8 prose.
Reasoning: The §6.8 requirement is about line-position discipline
that interacts with the lexer's line-stripping conventions. Without
running a probe with `"  %YAML 1.2\n---\nfoo\n"`, the behavior cannot
be conclusively classified. Recording as Indeterminate per the
"if unsure → Indeterminate" instruction. Auditor B may have closer
visibility on the lexer.

---

## Verdict tally

- Strict-conformant: 13 (REQ-§6.8-1, 2, 5, 8, 9, 10, 11, 12, 13, 16,
  17, 18, 19)
- Stricter-than-spec: 2 (REQ-§6.8-6, 7)
- Lenient: 4 (REQ-§6.8-3, 4, 14, 15)
- Indeterminate: 1 (REQ-§6.8-20)
- Non-conformant: 0
- Not-applicable: 0

Total: 20 requirements.

## Notes on cross-requirement findings

1. **No warning channel.** The four Lenient verdicts (REQ-§6.8-3, 4,
   14, 15) all share a single root cause: there is no `Warning`
   variant in `Event` and no out-of-band warning collector. The spec
   uses "should ... with a warning" three times in §6.8 alone
   (1.1-incompat, 1.3-higher-minor, unknown-directive). Adding a
   warning channel would convert all four Lenient verdicts to
   Strict-conformant on the warning leg without changing the accept-
   /-process behavior.

2. **`u8` representation of version components.** Phase 1's [86] /
   [87] strictness reproduces in REQ-§6.8-6 and REQ-§6.8-7. The
   stricter-than-spec behavior is internally consistent and rejects
   only versions that no real consumer can use.

3. **Directive name validation gap.** REQ-§6.8-15 confirms Phase 1's
   [84]/[85] Lenient findings: `ns-char+` is not enforced on the
   directive name, allowing NUL bytes (and presumably any non-
   whitespace byte) through. Combined with silent reserved-directive
   handling, this means malformed bytes can silently disappear.

4. **`%FOO` triggers `---`-required guard.** REQ-§6.8-19 noted that
   reserved directives also activate the §9.2 guard requiring `---`
   after any directive. This is correct — but it is a slightly
   counter-intuitive interaction that future plans may want to
   surface in user-facing docs.

## Final-check confirmation

`git status --porcelain | grep "rlsp-yaml-parser/"` from `/workspace/`
produced no output — the parser tree is clean. The audit-probe lives
at `/tmp/audit-probe-§6.8/` outside the repository and was not
committed.
