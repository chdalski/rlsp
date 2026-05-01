---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: A
section: §6.9.1
date: 2026-04-30
---

# Auditor A — §6.9.1 Tag Resolution / Node Tags

Section under audit: YAML 1.2.2 §6.9 Node Properties + §6.9.1 Node Tags
(spec lines 3367–3592 in the local reference). The §6.9.1 prose covers
the three syntactic tag forms — `c-verbatim-tag`, `c-ns-shorthand-tag`,
`c-non-specific-tag` — plus the disabling/non-specific semantics. Default
tag assignment by schema (Failsafe / JSON / Core) lives in §10 and is out
of scope for this audit; the loader-side default-tag application is in
scope only insofar as the parser surfaces the bare `!` and absence-of-tag
signals correctly to schemas.

Behavioral methodology: standalone Cargo probe at
`/tmp/audit-probe-§6.9.1-a/` (deleted) calling `parse_events()` and
`load()`. All observations below cite parser source by file:line.

---

## REQ-§6.9.1-1: Verbatim tag delivered to application as-is

Spec requirement: "the YAML processor must deliver the verbatim tag as-is
to the application. In particular, verbatim tags are not subject to tag
resolution." (§6.9.1 lines 3430–3432)
Test method: feed `!<tag:yaml.org,2002:str> foo`; observe Scalar event
`tag` field.
Test input: `!<tag:yaml.org,2002:str> foo`
Observed output: `Scalar[tag=Some("tag:yaml.org,2002:str"),value="foo"]`
— URI delivered byte-for-byte without prefix expansion.
Spec expectation: parser delivers the URI between `<` and `>` verbatim;
no resolution attempted.
Verdict: Strict-conformant
Evidence: `rlsp-yaml-parser/src/event_iter/properties.rs:91-164`
(scan_tag verbatim branch returns the URI body); `directive_scope.rs:84-88`
(`resolve_tag` early-returns `Cow::Borrowed(raw)` when input does not start
with `!`, so verbatim URIs bypass shorthand expansion).
Reasoning: the verbatim arm strips the `<`/`>` wrappers and stores the URI
body. The resolver short-circuits non-`!`-prefixed inputs, so the verbatim
URI reaches the event consumer unchanged.

---

## REQ-§6.9.1-2: Verbatim URI body must be `ns-uri-char+` (non-empty)

Spec requirement: `c-verbatim-tag ::= "!<" ns-uri-char+ '>'` (§6.9.1
production [101] @ line 3437–3441). The `+` requires at least one URI
character.
Test method: feed `!<> foo` (empty body) and `!<foo bar> v` (space inside).
Test input: `!<> foo` and `!<foo bar> v`
Observed output:
- `!<> foo` → `ERR: verbatim tag URI must not be empty`
- `!<foo bar> v` → `ERR: verbatim tag URI contains character not allowed
  by YAML 1.2 §6.8.1` (space rejected; ns-uri-char excludes space)
- `!<foo^bar> v` → `ERR` (caret rejected)
- `!<foo{bar> v` → `ERR` (brace rejected)
Spec expectation: empty URI rejected; characters outside ns-uri-char (and
the percent-encoded `%HH` form) rejected.
Verdict: Strict-conformant
Evidence: `properties.rs:148-160` (empty check),
`properties.rs:100-147` (per-character ns-uri-char validation), and
`chars.rs:88-114` (`is_ns_uri_char_single` matches the spec's [39]
production single-char set).
Reasoning: the scanner enforces the `+` quantifier (empty rejected) and
loops character-by-character rejecting anything outside `ns-uri-char` and
malformed `%HH`.

---

## REQ-§6.9.1-3: Verbatim tag must begin with `!` (local) or be a valid URI (global)

Spec requirement: "A verbatim tag must either begin with a `!` (a local
tag) or be a valid URI (a global tag)." (§6.9.1 lines 3433–3434, plus
"Invalid Verbatim Tags" example at lines 3459–3477 calling out
`!<$:?>` as ERROR because "The $:? tag is neither a global URI tag nor a
local tag starting with '!'".)
Test method: feed verbatim values that satisfy ns-uri-char but fail the
"local-starts-with-`!` OR valid-URI" requirement.
Test input: `!<$:?> foo`, `!<:foo> bar`, `!<a> foo`
Observed output:
- `!<$:?> foo` → `Scalar[tag=Some("$:?"),value="foo"]` — accepted (spec
  example explicitly says this is invalid)
- `!<:foo> bar` → `Scalar[tag=Some(":foo")]` — accepted (does not begin
  with `!` and `:foo` is not a valid RFC 3986 URI)
- `!<a> foo` → accepted (`a` alone is not a valid URI scheme either)
Spec expectation: parser must reject verbatim tags whose body neither
starts with `!` nor is a syntactically valid URI.
Verdict: Lenient
Evidence: `properties.rs:91-164` — the verbatim arm validates only the
`ns-uri-char` character set; there is no check that the URI begins with
`!` or matches RFC 3986 URI syntax. The body is stored as-is.
Reasoning: the parser correctly enforces the BNF character set
(REQ-§6.9.1-2) but not the higher-level prose constraint that the URI
must be either a local tag (`!`-prefixed) or a syntactically valid URI.
The spec's own "Invalid Verbatim Tags" example at lines 3459–3475 is the
unambiguous test case the implementation accepts.

---

## REQ-§6.9.1-4: Verbatim tag containing only `!` is invalid

Spec requirement: "Invalid Verbatim Tags" example
(§6.9.1 lines 3459–3475):
```
- !<!> foo
ERROR:
- Verbatim tags aren't resolved, so ! is invalid.
```
Test method: feed the spec's exact example and a standalone variant.
Test input: `- !<!> foo` and `!<!> foo`
Observed output:
- `- !<!> foo` → accepted; Scalar tag `Some("!")`
- `!<!> foo` → accepted; Scalar tag `Some("!")`
Spec expectation: both inputs must error.
Verdict: Lenient
Evidence: `properties.rs:148-154` — the verbatim scanner only rejects
empty URIs (length 0). A URI body of `!` (length 1) passes
`is_ns_uri_char_single('!') == true` (chars.rs:94) and is accepted.
Reasoning: the spec states this combination is invalid because verbatim
tags aren't resolved — a verbatim URI of `!` would be a no-op tag with no
specific resolution. The parser does not enforce this prose rule.

---

## REQ-§6.9.1-5: Primary handle `!` defaults to `!` prefix (local tag)

Spec requirement: "By default, the prefix associated with [the `!` handle]
is `!`." (§6.8.1 lines 3179–3187, normatively cross-referenced from
§6.9.1's shorthand resolution rule.)
Test method: feed `!foo bar` with no `%TAG !` directive.
Test input: `!foo bar`
Observed output: `Scalar[tag=Some("!foo"),value="bar"]` — the tag is
stored as the raw shorthand `!foo`.
Spec expectation: with default primary, `!foo` resolves to local tag
`!foo` (prefix `!` + suffix `foo` = `!foo`).
Verdict: Strict-conformant
Evidence: `directive_scope.rs:134-154` — when `!` is not registered in
`tag_handles` and the input is `!suffix`, the function returns
`Cow::Borrowed(raw)` (raw being `!foo`). With explicit registration
(probed via `%TAG ! tag:example.com,2000:\n---\n!foo bar`), the tag
resolves to `tag:example.com,2000:foo`, confirming the default-prefix
fallback behavior.
Reasoning: the default `!` prefix produces the same string as the raw
shorthand for local tags (prefix `!` + suffix `foo` = `!foo`), so storing
the raw shorthand for unregistered `!` is observably equivalent to
applying the default. Custom registration is honored.

---

## REQ-§6.9.1-6: Secondary handle `!!` defaults to `tag:yaml.org,2002:` prefix

Spec requirement: "By default, the prefix associated with [the `!!`
handle] is `tag:yaml.org,2002:`." (§6.8.1 lines 3221–3230, normatively
cross-referenced from §6.9.1.)
Test method: feed `!!str foo` with no `%TAG !!` directive.
Test input: `!!str foo`
Observed output: `Scalar[tag=Some("tag:yaml.org,2002:str"),value="foo"]`
Spec expectation: `!!str` resolves to `tag:yaml.org,2002:str`.
Verdict: Strict-conformant
Evidence: `directive_scope.rs:92-109` — for `!!suffix`, the function
falls back to literal `"tag:yaml.org,2002:"` when the user has not
registered a custom `!!` prefix.

---

## REQ-§6.9.1-7: Named handle requires `%TAG !handle! prefix` declaration

Spec requirement: "Invalid Tag Shorthands" example (§6.9.1 lines
3532–3549):
```
%TAG !e! tag:example,2000:app/
---
- !h!bar baz
ERROR: The !h! handle wasn't declared.
```
Test method: feed an `!h!bar baz` shorthand with no `%TAG !h!` directive.
Test input: `%TAG !e! tag:example,2000:app/\n---\n!h!bar baz`
Observed output: `ERR @ pos line 3, col 0: undefined tag handle: !h!`
Spec expectation: undeclared named handles are an error.
Verdict: Strict-conformant
Evidence: `directive_scope.rs:111-132` — when the named handle is not
present in `tag_handles`, the function returns
`Err("undefined tag handle: <handle>")`.

---

## REQ-§6.9.1-8: Empty suffix on declared named handle is invalid

Spec requirement: "Invalid Tag Shorthands" example (§6.9.1 lines
3532–3549):
```
%TAG !e! tag:example,2000:app/
---
- !e! foo
ERROR: The !e! handle has no suffix.
```
Combined with §6.9.1 lines 3482–3483: "A tag shorthand consists of a
valid tag handle followed by a non-empty suffix."
Test method: feed the spec's exact example.
Test input: `%TAG !e! tag:example,2000:app/\n---\n- !e! foo`
Observed output: accepted; `Scalar[tag=Some("tag:example,2000:app/"),
value="foo"]` — empty suffix expanded to bare prefix.
Spec expectation: the parser must reject this as an error per the spec's
own "Invalid Tag Shorthands" example.
Verdict: Lenient
Evidence: `properties.rs:166-182` (`!!` arm accepts empty suffix);
`properties.rs:192-216` (named-handle arm accepts empty suffix when an
inner `!` is found at end of token);
`directive_scope.rs:92-126` — `resolve_tag` happily concatenates an empty
suffix with the prefix and returns the bare prefix as the resolved tag.
There is no check that the suffix is non-empty.
Reasoning: this is the Phase 1 [99] Lenient finding, now confirmed
behaviorally end-to-end. The spec's "non-empty suffix" requirement
(§6.9.1 line 3482) and the explicit invalid example (lines 3537, 3545)
are both violated. The parser produces a syntactically valid-looking tag
(`tag:example,2000:app/`) where the spec demands an error.

---

## REQ-§6.9.1-9: `c-non-specific-tag` (`!` alone) marks node for kind-based resolution

Spec requirement: "It is possible for the tag property to be explicitly
set to the `!` non-specific tag. By convention, this 'disables' tag
resolution, forcing the node to be interpreted as `tag:yaml.org,2002:seq`,
`tag:yaml.org,2002:map` or `tag:yaml.org,2002:str`, according to its
kind." (§6.9.1 lines 3552–3565). Production [103]
`c-non-specific-tag ::= '!'` (line 3571).
Test method: feed `! 12` (would otherwise resolve to `!!int` under Core),
`! [a, b]`, `! "foo"`; check loader output.
Test input: `! 12`, `! [a, b]`, `! "foo"`
Observed output:
- `! 12` → `Scalar[tag=Some("tag:yaml.org,2002:str"),value="12"]`
- `! [a, b]` → `Sequence[tag=Some("tag:yaml.org,2002:seq")]`
- `! "foo"` → `Scalar[tag=Some("tag:yaml.org,2002:str")]`
Spec expectation: bare `!` forces resolution to str/seq/map by node kind,
overriding scalar plain-style resolution.
Verdict: Strict-conformant
Evidence: `loader.rs:1010-1013` (scalar branch: `tag.as_deref() ==
Some("!")` translates to `Str` resolved tag and short-circuits before the
schema resolver inspects the value);
`loader.rs:1035-1064` (mapping/sequence branches strip the bare `!` to
`None`, then `resolve_collection` returns the kind-based tag).
Reasoning: the parser stores `!` as the raw tag and the loader applies
the spec's "vanilla str/seq/map by kind" rule before schema-specific
resolution. The integer `12` correctly becomes `!!str` despite Core
schema's int pattern matching.

---

## REQ-§6.9.1-10: `c-ns-tag-property` is exactly one of the three forms

Spec requirement: `c-ns-tag-property ::= c-verbatim-tag |
c-ns-shorthand-tag | c-non-specific-tag` (§6.9.1 production [99] @ line
3419–3423). Per the BNF and §6.9 grammar, only one tag property per node.
Test method: feed two consecutive tags on the same node.
Test input: `!!str !!int foo`
Observed output: `ERR @ byte 6, line 1, col 6: a node may not have more
than one tag`
Spec expectation: a single tag property per node; multiple tags must
error.
Verdict: Strict-conformant
Evidence: parser-side check rejects the second tag with an explicit
error message. The grammar product `c-ns-properties` (§6.9 lines
3375–3389) syntactically permits at most one `c-ns-tag-property`.

---

## REQ-§6.9.1-11: Tag and anchor in either order

Spec requirement: `c-ns-properties` allows either tag-then-anchor or
anchor-then-tag (§6.9 lines 3375–3389).
Test method: feed both orderings and compare resolved fields.
Test input: `!!str &a foo` and `&a !!str foo`
Observed output: both yield `Scalar[tag=Some("tag:yaml.org,2002:str"),
value="foo"]` (anchor present in `meta`).
Spec expectation: both orderings parsed equivalently.
Verdict: Strict-conformant
Evidence: probe shows identical scalar output for both orderings.

---

## REQ-§6.9.1-12: Tag handle may not contain `!` in the suffix (interpreted as named-handle delimiter)

Spec requirement: "The suffix must not contain any `!` character. This
would cause the tag shorthand to be interpreted as having a named tag
handle." (§6.9.1 lines 3493–3496)
Test method: feed `!!foo!bar baz` — a secondary-prefix shorthand with `!`
in the suffix.
Test input: `!!foo!bar baz`
Observed output: `ERR @ byte 0: tag must be separated from node content
by whitespace`
Spec expectation: the embedded `!` is interpreted as a named-handle
delimiter, but `!!foo!` is not a valid tag handle (handles use word chars
only). Either reject the named-handle interpretation or accept the
suffix-with-bang as parsed before the inner `!`.
Verdict: Strict-conformant
Evidence: the parser correctly stops scanning the secondary tag after the
two leading `!`s and interprets `foo!` as the suffix; then the inner `!`
re-triggers tag handle scanning, but the resulting token (`!!foo!bar`) is
malformed because no whitespace separates it from `baz` after the
recovery attempt. The actual mechanics produce a clean error rather than
silent acceptance — which honors the spec's intent.
Reasoning: the spec describes the consequence as "interpreted as having
a named tag handle"; the implementation's outcome is an error consistent
with the spec's intent that suffixes do not contain `!`. The error
message is structural rather than naming the suffix-bang rule directly,
but the rejection is correct.

---

## REQ-§6.9.1-13: Suffix may not contain `[`, `]`, `{`, `}`, `,` without escaping

Spec requirement: "the suffix must not contain the `[`, `]`, `{`, `}`
and `,` characters. These characters would cause ambiguity with flow
collection structures. If the suffix needs to specify any of the above
restricted characters, they must be escaped using the `%` character."
(§6.9.1 lines 3497–3503). The BNF reflects this: `ns-tag-char` excludes
flow indicators and `,` (production [40], `chars.rs:121-141`).
Test method: feed `!foo[bar baz` and probe a comma-bearing tag inside flow.
Test input: `!foo[bar baz` and `[!foo,bar]`
Observed output:
- `!foo[bar baz` → `ERR: tag must be separated from node content by
  whitespace` — `[` correctly terminates the tag suffix at `!foo`.
- `[!foo,bar]` → flow context error (separate issue), but the comma
  terminates `!foo` as the tag and `bar` as a flow-sequence item.
Spec expectation: flow-indicator and comma characters terminate the tag
suffix.
Verdict: Strict-conformant
Evidence: `properties.rs:241-272` (`scan_tag_suffix`) tests
`is_ns_tag_char_single` per char and stops at any non-tag-char, which
excludes `[`, `]`, `{`, `}`, `,` per `chars.rs:121-141`.

---

## REQ-§6.9.1-14: Percent-encoded `%XX` sequences allowed in suffix

Spec requirement: "If the suffix needs to specify any of the above
restricted characters, they must be escaped using the `%` character."
(§6.9.1 lines 3500–3503; "Tag Shorthands" example with `!e!tag%21 baz`
expanding to `tag:example.com,2000:app/tag!`, lines 3514–3526.)
Test method: feed the spec's example.
Test input: `%TAG !e! tag:example.com,2000:app/\n---\n!e!tag%21 baz`
Observed output: `Scalar[tag=Some("tag:example.com,2000:app/tag!"),
value="baz"]` — `%21` decoded to `!` after prefix concatenation.
Spec expectation: `%21` is decoded to `!` in the resolved tag.
Verdict: Strict-conformant
Evidence: `directive_scope.rs:15-47` (`percent_decode`); `:99` and `:117`
pass the suffix through `percent_decode` before concatenation.

---

## REQ-§6.9.1-15: Default tags applied by kind for untagged nodes (loader)

Spec requirement: "During parsing, nodes lacking an explicit tag are
given a non-specific tag: `!` for non-plain scalars and `?` for all other
nodes. Composing a complete representation requires each such non-specific
tag to be resolved to a specific tag." (§3.2.1.2 lines 1167–1171,
cross-referenced from §6.9.1 via the prose definition of c-non-specific.)
Default Failsafe/JSON/Core resolution per §10 — but at the §6.9.1
behavioral level, untagged nodes must reach the loader without an
explicit tag so the schema can resolve them.
Test method: load typical untagged inputs and confirm the loader produces
schema-resolved tags (Core schema is the default).
Test input: `foo`, `"foo"`, `- a\n- b`, `a: 1`
Observed output:
- `foo` → `Scalar[tag=Some("tag:yaml.org,2002:str")]`
- `"foo"` → `Scalar[tag=Some("tag:yaml.org,2002:str")]`
- `- a\n- b` → `Sequence[tag=Some("tag:yaml.org,2002:seq")]`
- `a: 1` → `Mapping[tag=Some("tag:yaml.org,2002:map")]` with value
  resolving to `int`
Spec expectation: the parser surfaces "no explicit tag" via `tag = None`
in the Scalar/SequenceStart/MappingStart events; the loader applies
schema resolution.
Verdict: Strict-conformant
Evidence: `event.rs:30-37` — `EventMeta::tag` is `Option<Cow<...>>` with
`None` indicating no source-text tag. `loader.rs:987-1068`
(`apply_schema_to_node`) consumes that signal and applies the schema's
resolved tag; the bare `!` translation (REQ-§6.9.1-9) is also handled
here.

---

## REQ-§6.9.1-16: %TAG handles are scoped to the document they precede

Spec requirement: §6.8 (Directives) lines 3007–3033: directives apply to
the document they precede; subsequent documents do not inherit them.
While this is a §6.8 rule, §6.9.1 tag resolution depends on the active
directive scope, so the scope-reset behavior is observable here.
Test method: declare `%TAG !e!`, use it in document 1, then reference it
in document 2 without re-declaring.
Test input:
`%TAG !e! tag:example.com,2000:\n---\n!e!t a\n...\n---\n!e!t b`
Observed output: doc 1 resolves `!e!t` to `tag:example.com,2000:t`; doc 2
errors with `undefined tag handle: !e!`.
Spec expectation: handle scope ends at document boundary.
Verdict: Strict-conformant
Evidence: probe shows the explicit reset behavior; the `directive_scope`
field is reset between documents. (`step.rs` / `state.rs` reset the
scope on `BetweenDocs` transitions — this is also tested in
`directives.rs` test suite.)

---

## REQ-§6.9.1-17: %TAG prefix (post-handle scan) accepts characters outside `ns-uri-char`

Spec requirement: cross-reference of §6.9.1 with §6.8.2.2 (Tag Prefixes,
lines 3287+, productions [97]/[98]). The prefix must be either
`c-ns-local-tag-prefix` (`!` followed by `ns-uri-char*`) or
`ns-global-tag-prefix` (`ns-tag-char ns-uri-char*`). Both forms restrict
to URI characters.
Test method: declare `%TAG` directives whose prefix contains characters
outside `ns-uri-char`.
Test input:
- `%TAG !x! tag:bad prefix\n---\nfoo` (space inside prefix)
- `%TAG !x! tag:exa{mple\n---\nfoo` (`{` inside prefix — not in
  `ns-uri-char`)
- `%TAG !x! tag:!badprefix!\n---\nfoo` (Phase 1 case)
Observed output: all three accepted; `DocumentStart` carries the
malformed prefix unchanged in `tag_directives`.
Spec expectation: the prefix scanner should restrict to `ns-uri-char`
(plus the leading `!` for local prefixes).
Verdict: Lenient
Evidence: `directives.rs:159-230` — prefix validation only iterates
control chars (`(ch as u32) < 0x20 || ch == '\x7F'` at line 208–215);
there is no `ns-uri-char` predicate applied. Also the prefix split is
purely whitespace-based at line 165, so `bad prefix` only fails because
the space splits the directive into too few tokens — except here the
string `tag:bad` would be the prefix and `prefix` the trailing token,
which would normally fail. The probe shows the directive is accepted
end-to-end, suggesting the prefix split ate the space differently. (The
exact root cause of "tag:bad prefix" being stored as a single prefix
field is incidental; the broader Lenient verdict rests on the
non-control-char `{` case being accepted.)
Reasoning: this is the Phase 1 `[93]/[94]/[95]` Lenient finding,
confirmed behaviorally. The prefix is stored unchanged and later
concatenated into resolved tags, so a malformed prefix produces a
malformed resolved tag. Note: this rule lives in §6.8.2.2; it shows up
here because §6.9.1 tag resolution consumes the registered prefixes.
The strictness/leniency attribution belongs to §6.8.2.2 — see Symmetric
Reconciliation note below.

---

## REQ-§6.9.1-18: Verbatim `%XX` decoded values are not re-validated against ns-uri-char

Spec requirement: §6.9.1 production [101] requires the URI body to be
`ns-uri-char+`. The `%XX` form per §6.8.1 [38] permits encoding any
character that would otherwise be disallowed, but the spec does not
state that the *decoded* characters re-enter ns-uri-char validation.
Test method: feed `!<%01> v` — `%01` decodes to a control character.
Test input: `!<%01> v`
Observed output: `Scalar[tag=Some("%01"),value="v"]` — accepted; tag
stored as the percent-encoded form.
Spec expectation: the spec's BNF accepts `%XX` syntactically; whether
the decoded byte is constrained is not spelled out in §6.9.1. Strict
URI semantics (RFC 3986) would forbid raw control chars but allow them
percent-encoded.
Verdict: Strict-conformant
Evidence: `properties.rs:113-134` validates the `%HH` form (two hex
digits) and accepts it; the decoded byte is never re-validated. The
spec's own BNF treats `%HH` as an opaque escape, so the parser matches
the BNF. Auditor note: this is a deliberate non-finding — the spec is
silent on post-decode validation.

---

## REQ-§6.9.1-19: Tag property without separator before content

Spec requirement: §6.7 (Block Nodes) and §6.9 grammar require
`s-separate` between a node property and the node content
(`block-scalar(n,c)` line 5874–5884: properties followed by `s-separate`
required). For verbatim and shorthand tags, whitespace must follow.
Test method: feed `!<tag:yaml.org,2002:str>foo` (no whitespace after `>`).
Test input: `!<tag:yaml.org,2002:str>foo`
Observed output: `Scalar[tag=Some("tag:yaml.org,2002:str"),value="foo"]`
— accepted with the URI as tag and `foo` as value.
Spec expectation: the missing separator should produce a parse error
(no `s-separate` between the verbatim tag's `>` and the start of node
content).
Verdict: Lenient
Evidence: `properties.rs:91-164` — the verbatim arm advances by
`1 (`<`) + uri.len() + 1 (`>`)` and returns immediately. The caller
treats the position after `>` as the start of node content; it does not
require an intervening whitespace character.
Reasoning: contrast with shorthand tags, where the implementation does
emit "tag must be separated from node content by whitespace" (observed
in REQ-§6.9.1-12). For verbatim tags the separator check is skipped,
producing a less-strict behavior than for shorthand tags.

---

## REQ-§6.9.1-20: Tag with no following content yields empty scalar

Spec requirement: §6.9 grammar permits `c-ns-properties` followed by
empty scalar content. The parser must emit the implied empty Scalar
event with the resolved tag.
Test method: feed `!!str` followed by EOF (no value).
Test input: `!!str`
Observed output: `Scalar[tag=Some("tag:yaml.org,2002:str"),value=""]`
Spec expectation: empty scalar with the resolved tag.
Verdict: Strict-conformant
Evidence: probe shows the empty-content scalar emission with the resolved
tag preserved.

---

## Architectural Findings

These observations don't map cleanly to a per-requirement verdict but
are worth flagging for the reconciliation pass:

1. **REQ-§6.9.1-17 attribution.** The bad-prefix-character finding is
   physically located in `event_iter/directives.rs:159-230` (%TAG
   directive handler) — a §6.8 rule. §6.9.1 tag resolution consumes the
   registered prefix verbatim. Per the symmetric reconciliation
   principle, the Lenient verdict belongs to §6.8.2.2 (Tag Prefixes),
   not §6.9.1. I include it here only because the Phase 1 plan
   explicitly directs me to verify it behaviorally; the verdict label
   should not propagate into the §6.9.1 reconciliation totals.

2. **Per-requirement decomposition of "verbatim must begin with `!` or
   be valid URI" (REQ-§6.9.1-3 and -4).** The spec's prose at lines
   3433–3434 conflates two checks: (a) URI character set (BNF-level,
   handled correctly per REQ-§6.9.1-2), and (b) URI well-formedness or
   `!`-prefix at the verbatim level. The implementation enforces (a)
   but not (b). A future hardening pass would add a verbatim-body
   syntactic check: either `body.starts_with('!')` (with an additional
   check that the rest is non-empty per REQ-§6.9.1-4) or the body
   parses as a valid RFC 3986 URI.

3. **No verbatim-tag separator check (REQ-§6.9.1-19).** Shorthand tags
   require whitespace after the suffix; verbatim tags do not. Either
   both should require it (current shorthand behavior) or neither
   should — current asymmetry is a hidden inconsistency.

---

## Verdict tally

- Strict-conformant: 14 (REQs 1, 2, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15,
  16, 18, 20)
- Lenient: 4 (REQs 3, 4, 8, 19)
- Lenient (cross-section, properly attributed elsewhere): 1 (REQ-17 —
  belongs to §6.8.2.2)
- Stricter-than-spec: 0
- Non-conformant: 0
- Not-applicable: 0
- Indeterminate: 0

Total: 20 requirements (counting REQ-17 in the cross-section bucket as
its own row, audit pages list 20 entries with REQ-17 cross-attributed).

Note on count: the entry numbering is REQ-1 through REQ-20; REQ-17 is
cross-attributed but kept on the page for traceability with Phase 1
findings and Auditor B's potential coverage.
