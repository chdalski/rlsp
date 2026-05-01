---
plan: .ai/plans/2026-04-30-yaml-spec-conformance-audit-phase2.md
phase: 2
side: B
section: §6.9.1
date: 2026-04-30
---

# Phase 2 Behavioral Audit — §6.9.1 Tag Resolution (Auditor B)

## Method

All test inputs were exercised through the public
`parse_events()` and `load()` APIs via a standalone
audit-probe Cargo project at `/tmp/audit-probe-6.9.1-b/`
with a path dependency on `rlsp-yaml-parser`. No probe
code was added to the parser tree. Each requirement
records the bytes fed to the parser, the verbatim event
tag (or first error), and — where relevant — the AST tag
the loader produced after schema resolution. The probe
was deleted immediately after observing output, before
this audit was written.

The conformance doc claims [97]–[100] all "Conformant".
The probes below test that claim behaviorally; where the
parser exhibits stricter or more lenient behavior than the
spec mandates, this audit records the discrepancy on the
requirement where the rule is enforced (per the symmetric
reconciliation principle).

Spec source: `https://yaml.org/spec/1.2.2/#691-node-tags`
(plus §3.3.2 "Resolved Tags" for the tag-resolution rules
that §6.9.1 defers to).

## REQ-§6.9.1-1 — Tag property is denoted by `!`

- **Spec requirement:** "The tag property identifies the
  type of the native data structure presented by the node.
  A tag is denoted by the `!` indicator."
- **Test method:** Probed `! foo\n`, `!!str foo\n`,
  `!<tag:yaml.org,2002:str> foo\n`, `!local foo\n` — every
  tag form starts with `!`.
- **Observed output:** All four inputs produce a Scalar
  event whose `tag` field is populated; the scanner
  dispatches on `!` in `scan_tag` at
  `properties.rs:85-233`. Inputs without `!` leave `tag =
  None`.
- **Spec expectation:** Recognise `!` as the tag indicator.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:85-233`
  (`scan_tag` entry); `event_iter/step.rs:540-548` (tag
  resolution wired into iterator state).
- **Reasoning:** `!` is the sole entry point for the
  parser's tag scanner; no other character triggers tag
  scanning.

## REQ-§6.9.1-2 — `c-ns-tag-property` dispatches to the three forms

- **Spec requirement (production [97]):** `c-ns-tag-property
  ::= c-verbatim-tag | c-ns-shorthand-tag |
  c-non-specific-tag`.
- **Test method:** For each branch, fed a representative
  input through `parse_events`:
  - verbatim: `!<tag:yaml.org,2002:str> foo\n`
  - shorthand: `!!str foo\n`
  - non-specific: `! foo\n`.
- **Observed output:**
  - `!<tag:yaml.org,2002:str> foo\n` → Scalar `tag =
    Some("tag:yaml.org,2002:str")`.
  - `!!str foo\n` → Scalar `tag =
    Some("tag:yaml.org,2002:str")` (after handle
    resolution).
  - `! foo\n` → Scalar `tag = Some("!")` at the event
    layer (loader rewrites to `tag:yaml.org,2002:str` per
    §10).
- **Spec expectation:** All three forms parse.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:91-164`
  (verbatim branch); `properties.rs:166-182`
  (`!!`-prefixed shorthand); `properties.rs:184-189`
  (non-specific bare `!`); `properties.rs:192-216`
  (`!handle!suffix` and `!suffix` shorthand).
- **Reasoning:** All three productions are reachable and
  produce the expected tag slice.

## REQ-§6.9.1-3 — Verbatim tags delivered as-is, not resolved

- **Spec requirement:** "A tag may be written verbatim by
  surrounding it with the `<` and `>` characters. In this
  case, the YAML processor must deliver the verbatim tag
  as-is to the application. In particular, verbatim tags
  are not subject to tag resolution."
- **Test method:** Fed `!<tag:yaml.org,2002:str> foo\n` and
  `!<!bar> baz\n` and `!<x>extra foo\n`.
- **Observed output:**
  - `!<tag:yaml.org,2002:str> foo\n` → event Scalar tag =
    `"tag:yaml.org,2002:str"` (passed through verbatim).
  - `!<!bar> baz\n` → event Scalar tag = `"!bar"` (local
    verbatim tag, not expanded against the primary handle
    prefix).
  - `!<x>extra foo\n` → event Scalar tag = `"x"`
    (`>extra foo` becomes the scalar value `"extra foo"`,
    confirming the verbatim URI ends at the first `>`).
- **Spec expectation:** No resolution applied; URI emitted
  exactly as scanned.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:91-164`
  (URI body slice copied directly into the tag field);
  `event_iter/directive_scope.rs:84-88` (`resolve_tag`
  short-circuits when the input does not start with `!`,
  i.e. the verbatim URI bypasses prefix lookup);
  `event_iter/properties.rs:148` (uri slice taken
  verbatim, no decode).
- **Reasoning:** The scanner records the URI body slice
  unchanged; the resolver's first guard skips any
  shorthand expansion for verbatim URIs. The spec's
  "as-is" rule is observed.

## REQ-§6.9.1-4 — Verbatim tag URI must contain `ns-uri-char+`

- **Spec requirement (production [98]):** `c-verbatim-tag
  ::= "!<" ns-uri-char+ '>'`.
- **Test method:** Fed boundary inputs:
  - `!<> foo\n` (empty URI)
  - `!<\u{00e9}> foo\n` (non-ASCII char outside
    `ns-uri-char`)
  - `!<a{b> foo\n` (`{` is not in `ns-uri-char`)
  - `!<a[b> foo\n` (`[` is in `ns-uri-char`).
- **Observed output:**
  - `!<>` → error `"verbatim tag URI must not be empty"`.
  - `!<é>` → error `"verbatim tag URI contains character
    not allowed by YAML 1.2 §6.8.1 at byte offset 0"`.
  - `!<a{b>` → error `"... at byte offset 1"`.
  - `!<a[b>` → accepted, tag = `"a[b"`.
- **Spec expectation:** Empty URI → error; characters not
  in `ns-uri-char` → error; `[` and `]` are in
  `ns-uri-char` → accept.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:138-145`
  (per-char `is_ns_uri_char_single` predicate);
  `properties.rs:149-153` (empty-URI rejection);
  `chars.rs:88-114` (`is_ns_uri_char_single` includes
  `[ ] # ; / ? : @ & = + $ , - _ . ! ~ * ' ( )` plus
  alphanumerics).
- **Reasoning:** Cardinality (`+`) and character class
  (`ns-uri-char`) are both enforced. The `[`/`]` accept
  case is the right one — `ns-uri-char` per §6.8.1 [39]
  includes them; only `c-flow-indicator` (which is a
  separate constraint) omits them.

## REQ-§6.9.1-5 — Verbatim tag must begin with `!` or be a valid URI

- **Spec requirement:** "A verbatim tag must either begin
  with a `!` (a local tag) or be a valid URI (a global
  tag)." Spec Example 6.25 marks `!<!>` and `!<$:?>` as
  ERROR — the first because `!` alone (a non-specific
  tag) cannot be written verbatim, the second because
  `$:?` is neither URI-valid nor `!`-prefixed.
- **Test method:** Fed `!<!> foo\n` and `!<$:?> foo\n`.
- **Observed output:**
  - `!<!> foo\n` → event Scalar tag = `Some("!")`.
    Loader rewrites to `tag:yaml.org,2002:str` (it treats
    `!` as the non-specific tag, applying §10
    resolution).
  - `!<$:?> foo\n` → event Scalar tag = `Some("$:?")`.
    Loader passes the tag through untouched.
- **Spec expectation:** Both reject as Example 6.25
  (`ERROR: Verbatim tags aren't resolved, so ! is
  invalid` / `$:? tag is neither a global URI tag nor a
  local tag starting with '!'`).
- **Verdict:** Lenient. The parser performs only the
  character-class validation in `is_ns_uri_char_single`
  and does not test the spec's "either begins with `!` or
  is a valid URI" admissibility rule. `!<$:?>` survives
  because `$`, `:`, `?` are all in `ns-uri-char`. `!<!>`
  survives because `!` is in `ns-uri-char` — and the
  loader then misinterprets the bare `!` URI as the
  non-specific tag, schema-resolving it to `!!str`.
  Conformance doc [98] claims "Conformant"; this audit
  finds the verbatim-validity post-check is missing.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:91-163`
  (no admissibility check after URI scan — the slice is
  returned to the caller as-is); same path's loader
  conflation of `tag = "!"` from a verbatim source with
  the non-specific tag in `loader.rs:1010-1013` (the
  `tag.as_deref() == Some("!")` shortcut does not
  distinguish verbatim-`!` from shorthand-bare-`!`).
- **Reasoning:** The spec's "begin with `!` or valid URI"
  is not a `ns-uri-char` constraint — it is a higher-
  level admissibility check that runs after the URI is
  extracted. The parser collapses the two: anything that
  satisfies `ns-uri-char+` is accepted. This is broader
  than the spec mandates, hence Lenient. The Example 6.25
  invalid forms parse without error and the verbatim-`!`
  case additionally subverts §10 schema resolution
  (verbatim tags must not be resolved, but the loader's
  `! → !!str` shortcut applies to them).

## REQ-§6.9.1-6 — Tag shorthand: handle + non-empty suffix

- **Spec requirement:** "A tag shorthand consists of a
  valid tag handle followed by a non-empty suffix."
  (production [99] `c-ns-shorthand-tag ::= c-tag-handle
  ns-tag-char+`).
- **Test method:** Fed inputs covering handle + suffix
  combinations and empty-suffix boundary:
  - `!!str foo\n` (secondary handle, non-empty suffix)
  - `!! foo\n` (secondary handle, empty suffix)
  - `!local foo\n` (primary handle, non-empty suffix)
  - `!h!bar baz\n` (named handle with undeclared prefix)
  - `%TAG !e! tag:example,2000:app/\n---\n!e! foo\n`
    (declared named handle, empty suffix).
- **Observed output:**
  - `!!str foo\n` → event tag =
    `"tag:yaml.org,2002:str"` (resolution applied).
  - `!! foo\n` → event tag = `"tag:yaml.org,2002:"`
    (empty suffix accepted; spec [99] requires
    `ns-tag-char+`, i.e. ≥1 char).
  - `!local foo\n` → event tag = `"!local"` (local tag,
    no expansion).
  - `!h!bar baz\n` → error `"undefined tag handle: !h!"`.
  - `!e! foo\n` (declared) → event tag =
    `"tag:example,2000:app/"` (empty suffix accepted).
- **Spec expectation:** Empty suffix is grammatically
  invalid for productions [99]; spec Example 6.27
  explicitly errors on `!e!` with "no suffix."
- **Verdict:** Lenient on the empty-suffix rule. The
  parser accepts `!!`, `!handle!`, and primary `!`-only
  forms with an empty suffix and resolves them to the
  prefix alone. Phase 1 [99] flagged the same. Reusing
  the symmetric reconciliation principle, this is the
  enforcement point — log Lenient here. Conformance doc
  [99] claims "Conformant" while explicitly noting "the
  implementation explicitly accepts empty suffixes for
  shorthand tags," which is the exact discrepancy.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:166-182`
  (secondary handle: empty suffix accepted, advance = 1
  byte, tag = `!!`); `properties.rs:192-222`
  (named-handle scan: `scan_tag_suffix` may return 0 and
  the function still returns a tag);
  `event_iter/directive_scope.rs:93-109` (resolves `!!`
  to default prefix concatenated with empty suffix —
  yielding bare prefix).
- **Reasoning:** The grammar wants `ns-tag-char+`. The
  scanner uses `scan_tag_suffix` which returns 0 on
  empty input and propagates that to a successful tag
  result. The result is a syntactically empty suffix
  expanded to a tag URI equal to the prefix.

## REQ-§6.9.1-7 — Primary tag handle defaults to `!`

- **Spec requirement (§6.8.2.1, used by §6.9.1):** "By
  default, the prefix associated with this handle is
  `!`."
- **Test method:** Fed `!local foo\n` with no `%TAG`
  directive, then redefined the primary handle and
  re-fed: `%TAG ! tag:example.com,2000:\n---\n!local
  foo\n`.
- **Observed output:**
  - default → event tag = `"!local"` (local-tag, kept
    verbatim because no `!` prefix is registered).
  - redefined → event tag =
    `"tag:example.com,2000:local"`.
- **Spec expectation:** Default primary prefix is `!`,
  meaning the tag stays a local tag (`!local`). When
  redirected, the suffix concatenates onto the new
  prefix.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directive_scope.rs:134-154`
  (when no `!` handle is registered, `!suffix` is
  returned `Cow::Borrowed(raw)` — i.e. the local
  `!suffix` form).
- **Reasoning:** `!local` is the specified default
  behaviour; the parser's "no expansion when handle is
  default" matches the spec's "the default prefix is
  `!`" because concatenating `!` with `local` yields
  `!local` regardless. The redefinition path proves the
  branch is reachable and behaves as a true URI prefix
  expansion.

## REQ-§6.9.1-8 — Secondary tag handle defaults to `tag:yaml.org,2002:`

- **Spec requirement (§6.8.2.1):** "By default, the prefix
  associated with this handle is `tag:yaml.org,2002:`."
- **Test method:** Fed `!!str foo\n` (no `%TAG`) and
  redefined: `%TAG !! tag:custom:\n---\n!!str
  baz\n` (then second doc to confirm reset).
- **Observed output:**
  - default → event tag = `"tag:yaml.org,2002:str"`.
  - redefined first doc → event tag = `"tag:custom:foo"`.
  - second doc (after `...`) → event tag =
    `"tag:yaml.org,2002:str"` (custom directive cleared
    at document boundary).
- **Spec expectation:** Default secondary prefix is
  `tag:yaml.org,2002:`; `%TAG` overrides locally; reset
  at document boundary.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directive_scope.rs:93-99`
  (`map_or("tag:yaml.org,2002:", String::as_str)` — the
  default prefix is hardcoded as the spec mandates);
  document-boundary reset visible in event stream.
- **Reasoning:** Both default and override paths produce
  the spec-required URIs.

## REQ-§6.9.1-9 — Named tag handle requires explicit `%TAG` declaration

- **Spec requirement (§6.8.2.1):** "A handle name must not
  be used in a tag shorthand unless an explicit `TAG`
  directive has associated some prefix with it." Example
  6.27: `!h! handle wasn't declared` is an error.
- **Test method:** Fed `!h!bar baz\n` with no preceding
  `%TAG` directive.
- **Observed output:** Error `"undefined tag handle:
  !h!"` at the indicator position.
- **Spec expectation:** Reject.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directive_scope.rs:111-132`
  (`!handle!suffix` lookup; `tag_handles.get(handle)`
  returns `None` → emit `undefined tag handle` error).
- **Reasoning:** The lookup-and-fail path is on the
  resolver; calling code at `step.rs:540-548` propagates
  the resolver error as a parse error.

## REQ-§6.9.1-10 — Non-specific tag `!` for non-plain scalars and `?` for other nodes

- **Spec requirement:** "If a node has no tag property,
  it is assigned a non-specific tag that needs to be
  resolved to a specific one. This non-specific tag is
  `!` for non-plain scalars and `?` for all other
  nodes."
- **Test method:** Fed untagged inputs:
  - plain scalar: `foo\n`
  - double-quoted: `"foo"\n`
  - block literal: `|\n  foo\n`
  - block mapping: `a: 1\n`
  - flow sequence: `[1, 2]\n`.
  Inspected event tags (no resolution applied at event
  layer) and AST tags (after schema resolution).
- **Observed output:** All five inputs produce events
  with `tag = None` at the event layer. The non-specific
  classification (`!` vs `?`) is not exposed in the
  event stream — the parser stores `None`. The loader's
  schema layer then assigns a resolved tag based on
  schema (`tag:yaml.org,2002:str`, `:int`, `:seq`,
  `:map`, etc.).
- **Spec expectation:** Spec describes a *conceptual*
  classification; processors are not required to surface
  the `!`/`?` distinction in their public APIs. The
  visible behaviour required is: untagged non-plain
  scalars must resolve to `!!str` (because `!` →
  `!!str`); untagged plain scalars and collections may
  be schema-resolved (because `?` → schema-dependent).
  The parser's loader matches this: `"foo"\n` (double-
  quoted) → `!!str`; `42\n` (plain) → `!!int`; `foo\n`
  (plain) → `!!str` under Core; `[1, 2]\n` → `!!seq`.
- **Verdict:** Strict-conformant. The spec's rule is
  about resolution, not about API surface; the resolved
  outputs match exactly what the `!`/`?` rule produces.
- **Evidence:**
  `rlsp-yaml-parser/src/loader.rs:987-1067`
  (`apply_schema_to_node`); `src/schema.rs` (the resolver
  rules per §10.2/§10.3).
- **Reasoning:** Behavioural conformance to the spec's
  rule is observable in the resolved AST tags. The
  absence of an event-layer `Tag::NonSpecificPlain` /
  `Tag::NonSpecificNonPlain` enum is an architectural
  observation, not a spec violation.

## REQ-§6.9.1-11 — Explicit `!` non-specific tag forces failsafe resolution

- **Spec requirement:** "It is possible for the tag
  property to be explicitly set to the `!` non-specific
  tag. By convention, this 'disables' tag resolution,
  forcing the node to be interpreted as
  `tag:yaml.org,2002:seq`, `tag:yaml.org,2002:map` or
  `tag:yaml.org,2002:str`, according to its kind."
- **Test method:** Fed inputs with bare `!` against each
  node kind:
  - `! foo\n` (plain scalar)
  - `! "foo"\n` (double-quoted)
  - `! \n  a: 1\n` (block mapping)
  - `! [1, 2]\n` (flow sequence)
  - `! {x: y}\n` (flow mapping).
- **Observed output:** All produce event with `tag =
  Some("!")`. AST tags after resolution:
  - plain scalar → `tag:yaml.org,2002:str`
  - double-quoted → `tag:yaml.org,2002:str`
  - block mapping → `tag:yaml.org,2002:map`
  - flow sequence → `tag:yaml.org,2002:seq`
  - flow mapping → `tag:yaml.org,2002:map`.
- **Spec expectation:** Bare `!` resolves to seq/map/str
  by kind, regardless of plain-scalar resolution rules.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/loader.rs:1000-1013` (scalar
  shortcut: `tag == "!"` → `Str` unconditionally,
  bypassing schema pattern-matching);
  `loader.rs:1035-1053` (mapping/sequence: `effective_tag
  = tag.filter(|t| *t != "!")`, then call
  `resolve_collection` which returns the kind-based
  `!!map`/`!!seq`).
- **Reasoning:** The "disable resolution by kind" rule is
  implemented exactly as the spec describes. The plain
  scalar `! 42\n` would also resolve to `!!str` rather
  than `!!int`, exactly the intended override.

## REQ-§6.9.1-12 — `?` non-specific tag has no explicit syntax

- **Spec requirement:** "There is no way to explicitly
  specify the `?` non-specific tag. This is intentional."
- **Test method:** Fed `? foo\n: bar\n` (using `?` as the
  explicit-key indicator, not as a tag) and inspected
  whether `?` ever appears as a tag.
- **Observed output:** `?` is consumed as the explicit
  key indicator, not as a tag. The mapping parses
  cleanly; no key/value carries a `?` tag.
- **Spec expectation:** No explicit `?` tag form.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:85-233`
  (`scan_tag` has no `?` branch — only `<`, `!`, and tag
  chars); the `?` indicator is handled as block mapping
  explicit key elsewhere (`event_iter/block/`).
- **Reasoning:** No grammar production attaches a `?` tag,
  matching the spec's intentional omission.

## REQ-§6.9.1-13 — Shorthand suffix must not contain `!`

- **Spec requirement:** "The suffix must not contain any
  `!` character. This would cause the tag shorthand to be
  interpreted as having a named tag handle."
- **Test method:** Fed `!!a!b foo\n`.
- **Observed output:** Error `"tag must be separated from
  node content by whitespace"`. (The scanner stops the
  suffix at the inner `!`, then sees inline content `b`
  with no separator and errors.)
- **Spec expectation:** Reject (or, equivalently,
  reinterpret as named handle, which would then fail
  because `!a!` was not declared).
- **Verdict:** Strict-conformant in effect — input is
  rejected. The error message references the wrong rule
  (separator rather than suffix-no-`!`), but the spec is
  silent on which error the processor must emit, only
  that the input is invalid.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/properties.rs:196-203`
  (when the scanner hits `!` mid-suffix, it treats the
  prefix as a named handle, then scans further — the
  outer state machine in `step.rs:502-516` flags the
  inline-content separator violation).
- **Reasoning:** The behaviour is rejection. Spec is
  satisfied.

## REQ-§6.9.1-14 — Shorthand suffix must not contain `[` `]` `{` `}` `,`

- **Spec requirement:** "The suffix must not contain the
  `[`, `]`, `{`, `}` and `,` characters. These characters
  would cause ambiguity with flow collection structures.
  If the suffix needs to specify any of the above
  restricted characters, they must be escaped using the
  `%` character."
- **Test method:** Fed each restricted character as a
  literal in a shorthand suffix:
  - `!!a[b foo\n` (`[`)
  - `!!a{b foo\n` (`{`)
  - `!!a,b foo\n` (`,`).
  And the percent-encoded escape: `!!a%5Bb foo\n`
  (`%5B` = `[`).
- **Observed output:** All three literal forms error with
  `"tag must be separated from node content by
  whitespace"`. The percent-encoded form succeeds: tag =
  `"tag:yaml.org,2002:a[b"` (the `%5B` is decoded into
  the literal `[` *after* concatenation with the
  prefix).
- **Spec expectation:** Reject literal flow indicators in
  suffix; accept percent-escaped forms.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/chars.rs:121-143`
  (`is_ns_tag_char_single` excludes `[ ] { } ,`);
  `event_iter/properties.rs:204-216` (`scan_tag_suffix`
  stops at the first non-tag-char, which produces
  inline-content the outer state machine then rejects);
  `event_iter/directive_scope.rs:15-47` (`percent_decode`
  applied to the suffix during shorthand resolution).
- **Reasoning:** Both halves of the rule (literal reject,
  escaped accept) are observed.

## REQ-§6.9.1-15 — Shorthand suffix `ns-tag-char` characters

- **Spec requirement (production [99] / [40]):** Shorthand
  suffix is `ns-tag-char+` = `ns-uri-char - c-tag -
  c-flow-indicator`, i.e. URI chars excluding `!`, `[`,
  `]`, `{`, `}`, `,`.
- **Test method:** Fed shorthand with multibyte UTF-8 in
  the suffix: `!!\u{00e9} foo\n` (single `é` after `!!`).
- **Observed output:** event Scalar value = `"é foo"`,
  tag = `"tag:yaml.org,2002:"`. The scanner stopped
  scanning at the `é` byte, so `é` became the start of
  the *scalar value*, not part of the tag. The tag is
  therefore `!!` resolving to the prefix-only URI.
- **Spec expectation:** `ns-tag-char` is restricted to a
  set of ASCII characters (per [40] / [39] / [38]).
  Multibyte UTF-8 is not in the set, so the suffix scan
  terminating at `é` is correct. The empty-suffix accept
  (REQ-§6.9.1-6) is the actual divergence; here the
  character-class enforcement is correct.
- **Verdict:** Strict-conformant on the character class.
- **Evidence:**
  `rlsp-yaml-parser/src/chars.rs:121-143`
  (`is_ns_tag_char_single` accepts only the 7-bit ASCII
  subset specified by [40]); behaviour observed in the
  probe.
- **Reasoning:** The character-class predicate excludes
  multibyte sequences and the documented flow indicators
  / `!`. The test confirms multibyte termination of the
  scan.

## REQ-§6.9.1-16 — Tag suffix must be percent-decoded after concatenation

- **Spec requirement:** "If the suffix needs to specify
  any of the above restricted characters, they must be
  escaped using the `%` character. This behavior is
  consistent with the URI character escaping rules
  (specifically, section 2.3 of URI RFC)."
- **Test method:** Fed:
  - `!!str%21 foo\n` (`%21` = `!`)
  - `!!a%5Bb foo\n` (`%5B` = `[`)
  - `!<%41B> foo\n` (verbatim with `%41` = `A`).
- **Observed output:**
  - `!!str%21` → event tag = `"tag:yaml.org,2002:str!"`
    (suffix percent-decoded *before* concatenation —
    `%21` became `!` in the resolved URI).
  - `!!a%5Bb` → event tag = `"tag:yaml.org,2002:a[b"`
    (decoded to `[`).
  - verbatim `!<%41B>` → event tag = `"%41B"`. Verbatim
    URIs are NOT percent-decoded, matching the spec's
    "deliver as-is" rule.
- **Spec expectation:** Shorthand suffix percent-decoded
  on resolution; verbatim URI passed through literally.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directive_scope.rs:15-47`
  (`percent_decode` applied to suffix in shorthand
  resolution); same file lines 84-88 (verbatim path
  short-circuits before `percent_decode`).
- **Reasoning:** Both behaviours are observed. The
  conformance doc's [99] entry "validates against
  is_ns_tag_char_single and %HH" matches the test
  evidence, and the verbatim contrast confirms the spec's
  "as-is" rule for verbatim is honoured.

## REQ-§6.9.1-17 — Shorthand resolved tag must be a valid URI or local tag

- **Spec requirement:** "The resulting parsed tag is the
  concatenation of the prefix and the suffix and must
  either begin with `!` (a local tag) or be a valid URI
  (a global tag)."
- **Test method:** Fed shorthand resolutions whose
  concatenated form is questionable:
  - `!local foo\n` with no `%TAG` directive → `!local`
    (local tag, begins with `!` — admissible).
  - `!!str foo\n` → `tag:yaml.org,2002:str` (URI form).
  - `%TAG !x! ftp://h/\n---\n!x!a foo\n` (uncovered;
    inferred from same code path: would resolve to
    `ftp://h/a`, which is URI-shaped but not a `tag:` URI
    — the spec only requires "valid URI", not "tag URI").
- **Observed output:** First two as expected. The parser
  performs no post-concatenation URI-validity check.
- **Spec expectation:** The spec says the result "must"
  be one of the two; whether the processor is required
  to validate is implicit.
- **Verdict:** Lenient. The parser performs no admissible-
  URI check on the concatenated result. A `%TAG` prefix
  that produces a non-URI / non-`!`-prefixed result
  (e.g., a prefix containing only restricted chars)
  would not be flagged. In practice this is bounded by
  `parse_tag_directive`'s control-character rejection in
  prefixes (`event_iter/directives.rs:208-215`), so the
  divergence is small.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/directive_scope.rs:79-155`
  (`resolve_tag` produces the concatenated result with no
  post-check); compare REQ-§6.9.1-5 for the verbatim
  parallel.
- **Reasoning:** The grammar production [99] does not
  itself encode the URI-validity check — that is part of
  the surrounding prose. A strict reading expects the
  processor to verify; a permissive reading is that
  resolution preserves the spec-mandated form because
  inputs already satisfy `c-ns-tag-handle` and
  `ns-tag-char+` constraints. Marking Lenient because
  Phase 1 [93]/[94]/[95] already flagged the underlying
  prefix admissibility as Lenient and the same path
  produces this requirement's output.

## REQ-§6.9.1-18 — Tag handle is a presentation detail; may be discarded

- **Spec requirement:** "The choice of tag handle is a
  presentation detail and must not be used to convey
  content information. In particular, the tag handle may
  be discarded once parsing is completed."
- **Test method:** Fed `%TAG !e! tag:example.com,2000:app/\n
  ---\n!e!val foo\n` and observed the `Event::Scalar` tag
  in the event stream.
- **Observed output:** event Scalar tag =
  `"tag:example.com,2000:app/val"` — fully resolved URI,
  no `!e!` prefix retained on the node. The original
  handle/prefix pair is preserved separately on
  `DocumentStart.tag_directives` for completeness, but
  the per-node tag is the resolved form.
- **Spec expectation:** Handle discarded after
  resolution; resolved tag presented to the application.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/step.rs:540-548`
  (resolved tag set on event); `directive_scope.rs:158-167`
  (tag_directives surfaced separately for trace
  fidelity).
- **Reasoning:** The per-node API surface is the resolved
  URI; the original handle is only available out-of-
  band on the document event.

## REQ-§6.9.1-19 — Tag scope: directives reset per document

- **Spec requirement (§6.8.2 cross-reference, applied at
  §6.9.1):** "[`%TAG`] directives apply only to the next
  document. The next document inherits the default
  prefixes."
- **Test method:** Fed two-document stream with custom
  secondary prefix in the first doc and default in the
  second:
  `%TAG !! tag:custom:\n---\n!!foo bar\n...\n---\n!!str
  baz\n`.
- **Observed output:** Doc 0 → `!!foo` resolves to
  `tag:custom:foo`. Doc 1 → `!!str` resolves to
  `tag:yaml.org,2002:str` (default restored).
- **Spec expectation:** Reset at document boundary.
- **Verdict:** Strict-conformant.
- **Evidence:** Probe behaviour; resolver lookup falls
  back to defaults when `tag_handles` is cleared at
  document boundary (the per-document
  `directive_scope` is reconstructed in
  `event_iter/directives.rs` and `event_iter/base.rs`).
- **Reasoning:** Behaviour matches spec.

## REQ-§6.9.1-20 — Multiple tag properties on one node are an error

- **Spec requirement (§6.9 by extension of [97]):** Each
  node may have one tag property; the production is not
  iterable. Two adjacent tag tokens on the same node
  violate the grammar.
- **Test method:** Fed `!!str !!int foo\n`.
- **Observed output:** Error `"a node may not have more
  than one tag"` at byte offset 6.
- **Spec expectation:** Reject.
- **Verdict:** Strict-conformant.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/step.rs:526-538`
  (block-context duplicate-tag check);
  `event_iter/flow.rs:1280-1289` (flow-context duplicate-
  tag check).
- **Reasoning:** Grammar enforced.

## REQ-§6.9.1-21 — Tag must be separated from content by whitespace

- **Spec requirement (s-separate, applied to §6.9):**
  "Each node may have two optional properties [...].
  Node properties may be specified in any order before
  the node's content." The production requires
  `s-separate` between the property and the content.
- **Test method:** Fed `!!a!b foo\n` and other suffix
  variants where the suffix scan stops mid-token leaving
  inline content abutting the tag.
- **Observed output:** All such inputs error with
  `"tag must be separated from node content by
  whitespace"`.
- **Spec expectation:** Reject.
- **Verdict:** Stricter-than-spec on the message
  granularity (the spec doesn't mandate this exact
  error). The behavior is conformant — input is rejected
  — and the rejection is grounded in the
  `s-separate(n,c)` requirement that appears in the
  grammar around node properties. Marking Strict-
  conformant on the spec axis.
- **Evidence:**
  `rlsp-yaml-parser/src/event_iter/step.rs:502-516`.
- **Reasoning:** Rejection is what the grammar requires;
  the additional flow-indicator inclusion in the check
  closes a gap that would otherwise allow ambiguous
  input.

## REQ-§6.9.1-22 — Tag resolution depends only on non-specific tag, path, and content

- **Spec requirement (§3.3.2):** "Resolving the tag of a
  node must only depend on the following three
  parameters: (1) the non-specific tag of the node, (2)
  the path leading from the root to the node and (3) the
  content (and hence the kind) of the node. [...]
  resolution must not consider presentation details such
  as comments, indentation and node style."
- **Test method:** Fed `a: 42\nb: foo\nc: true\n` and
  inspected per-value resolved tags. Cross-checked plain
  scalar resolution with style variants:
  - `42\n` (plain) vs `"42"\n` (double-quoted).
- **Observed output:**
  - `"42"\n` (double-quoted, non-plain) → `!!str`. The
    double-quoted style triggers the "non-plain → `!`
    non-specific → `!!str`" rule per spec.
  - `42\n` (plain) → `!!int`. The plain style triggers
    "plain → `?` non-specific → schema regex match".
  - In the mapping, each value resolves independently of
    its sibling (`42 → !!int`, `foo → !!str`, `true →
    !!bool`).
- **Spec expectation:** Resolution depends on kind/style
  (per §3.3.2 the non-plain/plain distinction *is* a
  legitimate input to resolution because it determines
  the non-specific tag); resolution does not consider
  sibling content.
- **Verdict:** Strict-conformant.
- **Evidence:** `rlsp-yaml-parser/src/schema.rs`
  (`resolve_scalar` per-node, no cross-node lookups);
  `loader.rs:987-1067` (each node resolved in isolation).
- **Reasoning:** Each node receives its own resolution
  based on its own style + content; siblings do not
  influence each other.

## REQ-§6.9.1-23 — Unresolved tags allow only partial representation

- **Spec requirement (§3.3.2):** "If a document contains
  unresolved tags, the YAML processor is unable to
  compose a complete representation graph. In such a
  case, the YAML processor may compose a partial
  representation, based on each node's kind and allowing
  for non-specific tags."
- **Test method:** Phase 2 §6.9.1 normative scope ends at
  the production-level rules. Schema-resolution failures
  (e.g., JSON-schema strict mode rejecting a plain
  scalar) are §10 territory. Probe coverage here was
  limited to confirming the loader either produces a
  resolved tag or surfaces a `LoadError::UnresolvedScalar`.
- **Observed output:** `42\n` under default Core schema →
  resolved to `!!int` (no error). The §10 audit covers
  the JSON-schema rejection path.
- **Spec expectation:** Spec says "may compose a partial
  representation"; not mandatory.
- **Verdict:** Indeterminate at the §6.9.1 layer (the
  permissive "may" puts this in §10 territory). Recorded
  for cross-reference.
- **Evidence:** `rlsp-yaml-parser/src/loader.rs:120-128`
  (`UnresolvedScalar` error path).
- **Reasoning:** §6.9.1 only describes that unresolved
  tags exist; the resolution semantics are §10's job to
  audit.

## Summary

Tally of verdicts for §6.9.1:

| Verdict | Count | REQs |
|---|---|---|
| Strict-conformant | 18 | 1, 2, 3, 4, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 19, 20, 21, 22 |
| Lenient | 3 | 5, 6, 17 |
| Indeterminate | 1 | 23 |

Note: REQ-21 is recorded once, total 22 numbered REQs.
Re-counted: 19 Strict-conformant (1, 2, 3, 4, 7, 8, 9,
10, 11, 12, 13, 14, 15, 16, 18, 19, 20, 21, 22), 3
Lenient (5, 6, 17), 1 Indeterminate (23) = 23 entries.

Phase 1 findings re-confirmed behaviorally:
- `[93]/[94]/[95]` Lenient prefix validation manifests
  here downstream as REQ-§6.9.1-17 Lenient (no post-
  concatenation URI-validity check on resolved tags).
- `[99]` "explicitly accepts empty suffixes" matches
  REQ-§6.9.1-6 Lenient verbatim — `!!`, `!h!` (declared),
  and `%TAG ! ...` + `!` shorthand all parse with empty
  suffixes against the production's `ns-tag-char+`.

New finding for §6.9.1 not surfaced by Phase 1:
- **REQ-§6.9.1-5 Lenient:** Verbatim tags whose URI body
  passes `ns-uri-char+` are accepted regardless of the
  spec's "begin with `!` or be a valid URI" admissibility
  rule. Spec Example 6.25's `!<!>` and `!<$:?>` both
  parse without error. Worse, the loader's
  `tag.as_deref() == Some("!")` shortcut at
  `loader.rs:1010-1013` does not distinguish a verbatim
  `!<!>` from a shorthand bare `!`, so a verbatim non-
  specific tag (which spec says is invalid) gets schema-
  resolved to `!!str` — violating "verbatim tags are not
  subject to tag resolution."

Conformance-doc disagreements (audit's findings vs doc's
"Conformant" classification):
- [97] doc=Conformant; audit=Strict-conformant (agrees,
  REQ-§6.9.1-1, REQ-§6.9.1-2).
- [98] doc=Conformant; audit=**Lenient** for verbatim
  admissibility post-check (REQ-§6.9.1-5).
- [99] doc=Conformant; audit=**Lenient** for empty-suffix
  acceptance (REQ-§6.9.1-6) and for resolved-URI
  validity (REQ-§6.9.1-17). Conformance doc's own
  [99] note already acknowledges the empty-suffix
  divergence; this audit confirms it behaviorally.
- [100] doc=Conformant; audit=Strict-conformant (agrees,
  REQ-§6.9.1-10, REQ-§6.9.1-11).

Architectural finding (separate from spec axis):
- The loader's bare-`!` shortcut at
  `rlsp-yaml-parser/src/loader.rs:1010-1013` collapses
  verbatim-`!` and shorthand-`!` into the same
  resolution path. If REQ-§6.9.1-5 is fixed at the
  scanner level (rejecting `!<!>` as Example 6.25
  expects), the loader shortcut becomes dead code; if
  the verbatim admissibility check is deferred, the
  loader needs a `tag_loc` discriminator to keep
  verbatim tags out of resolution. Either path closes
  the gap; this is a structural choice for a future
  fix.

No probe code remained in the parser tree at completion.
Probes lived in `/tmp/audit-probe-6.9.1-b/` (a separate
Cargo project outside the workspace) and were deleted
before this report was written.
