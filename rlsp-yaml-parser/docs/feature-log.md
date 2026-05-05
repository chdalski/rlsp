# Feature Log

Feature decisions for rlsp-yaml-parser, newest first. Tiered by
user impact, implementation feasibility, and alignment
with existing infrastructure.

**Tiers:**
- **1** — High impact, feasible now
- **2** — Medium impact, moderate effort
- **3** — Valuable but higher effort
- **4** — Niche or high effort / low return

---

### Literal Stream Character Validation [completed]

**Description:** The parser now enforces the YAML 1.2 §5.1 two-tier character-set
rule on all literal stream content. Non-printable characters in the input produce
a parse error naming the offending codepoint in `U+XXXX` format:

- **c-printable rule** (stream-wide): applied to plain scalar content, block scalar
  content (literal `|` and folded `>`), and comment bodies. Rejects C0 controls
  (except TAB), DEL (U+007F), C1 controls (except NEL U+0085), and non-characters
  U+FFFE/U+FFFF.
- **nb-json rule** (quoted scalars): the spec requires that YAML processors allow
  all non-C0 characters inside quoted scalars (§5.1 JSON-compatibility clause).
  Single-quoted and double-quoted scalar content may contain DEL, C1 controls, and
  U+FFFE/U+FFFF — only C0 controls (except TAB) are rejected.

Previously the parser accepted any byte that cleared its delimiter scan, silently
passing non-printable bytes to callers. Inputs such as a BEL character in a plain
scalar or a SOH byte in a comment now produce structured parse errors.

Existing specific checks (NUL in trailing comments, BOM in document body) are
preserved and still fire with their targeted messages; the new checks are backstops
for any other non-printable that reaches a scanner.
**Complexity:** Low
**Comment:** Security hardening: passing raw non-printable bytes through a YAML
parser into downstream log pipelines, LSP diagnostics, and rendered output is a
risk. The fix is strict-reject (not warn or diagnostic) — the user's stated
preference and the spec's normative wording ("YAML streams use only the printable
subset"). The nb-json exception for quoted scalars is a spec mandate ("must allow"),
not optional. The enforcement adds a second byte-scan pass over content slices;
overhead on valid YAML (which contains no non-printables) is near-zero.
**Tier:** 1

---

### LoadError Position Fields [completed]

**Description:** Five `LoadError` variants (`NestingDepthLimitExceeded`,
`AnchorCountLimitExceeded`, `AliasExpansionLimitExceeded`, `CircularAlias`,
`UndefinedAlias`) now carry a `pos: Pos` field that identifies the source
position of the node that triggered the error. LSP diagnostics produced from
these errors now point to the offending node rather than line 0, column 0.
**Complexity:** Low
**Comment:** Previously these variants reported `Pos::ORIGIN` in the LSP
diagnostic range, sending users to the start of the file regardless of where
the error occurred. The position is sourced from existing span tracking —
no new parser state was required.
**Tier:** 1

---

### Single-comparison document dispatch [completed]

**Description:** The core parse loop's per-line handler selection now performs a
single byte comparison instead of up to 15 sequential checks. Each YAML structural
indicator (`-`, `*`, `!`, `&`, `[`, `{`, etc.) has a unique first non-whitespace
byte; the parser matches on that byte directly and routes to the correct handler in
one step. Mapping key detection (which can start with any byte) runs once
unconditionally after the byte match.
**Complexity:** Low
**Comment:** Dense and small documents (Kubernetes manifests, short configuration
files) spend a measurable fraction of parse time in handler selection. The
restructure eliminates redundant probes without changing any parse semantics —
all 726 yaml-test-suite conformance tests continue to pass.
**Tier:** 1

---

### Lazy Position Resolution via `LineIndex` [completed]

**Description:** `Span` now stores only byte offsets (`start: u32`, `end: u32`,
8 bytes total, down from 48 bytes). Line and column numbers are resolved on demand
via the new `LineIndex` type. Each `Document<Span>` exposes a `line_index()` accessor
that returns a `&LineIndex` shared across all documents in the same parse. Callers
convert byte offsets to `(line, column)` pairs with `idx.line_column(offset)`.
**Complexity:** Medium
**Comment:** `Span` is the most-allocated type in the parser — every AST node and
event carries one. Shrinking it from 48 to 8 bytes reduces peak heap usage and
improves cache locality for batch parsing workloads. The `Pos` type is retained
for error reporting (`LoadError::Parse { pos: Pos }`). The `LineIndex` is built
once per input string and shared via `Arc` across multi-document streams so the
newline table is not duplicated.
**Tier:** 1

### Named Tag Handle `_` Rejection [completed]

**Description:** Named `%TAG` directive handle names now reject `_` per YAML 1.2.2
§5.6 (production [38] `ns-word-char ::= ns-dec-digit | ns-ascii-letter | '-'`) and
§6.8.1 (production [92] `c-named-tag-handle ::= c-tag ns-word-char+ c-tag`). A
`%TAG` line such as `%TAG !my_handle! tag:example.org,2024:` is now a parse error.
Only `%TAG` directive handle names are affected — inline tag suffixes (e.g.,
`!!my_type`) continue to accept `_` because `ns-uri-char` (production [39])
explicitly permits it.
**Complexity:** Low
**Comment:** Previously the parser accepted `_` in named handle names, silently
diverging from the spec alphabet. The fix aligns the `is_valid_tag_handle` predicate
with `ns-word-char` exactly.
**Tier:** 1

### Flow Collections (`[...]` and `{...}`) [completed]

**Description:** Parse flow sequences and flow mappings. The parser
uses an explicit `Vec<FlowFrame>` stack — no recursion — so deeply
nested flow input cannot overflow the call stack. Flow and block
nesting depths share the same `MAX_COLLECTION_DEPTH` counter so
combined depth is bounded uniformly.
**Complexity:** High
**Comment:** The largest single method in the codebase. Non-recursive
explicit stack was a deliberate security decision: recursive descent
over untrusted flow input would be exploitable via deep nesting.
**Tier:** 1

### Comment Preservation [completed]

**Description:** Comments are emitted as `Event::Comment` events with
their body text. The loader attaches leading comments (comment lines
preceding a node) and trailing comments (inline comments on the same
line as a node value) to AST nodes. One `Comment` event is emitted
per physical line.
**Complexity:** Medium
**Comment:** Comments are fully first-class in the AST. Leading
comments are attached to mapping entries and sequence items;
trailing comments are attached to values. Document-prefix comments
before the first node are discarded per YAML §9.2 — the spec does
not define comment ownership there.
**Tier:** 1

### AST Loader — Lossless Mode [completed]

**Description:** The `loader` module converts the event stream into
a `Vec<Document<Span>>`. In lossless mode (the default), alias
references are preserved as `Node::Alias` nodes rather than expanded.
This is the safe default for LSP use: no expansion budget is needed,
and alias bombs cannot cause memory exhaustion.
**Complexity:** Medium
**Comment:** Lossless mode is safe for untrusted input without any
alias expansion limit because no expansion ever occurs.
**Tier:** 1

### AST Loader — Resolved Mode [completed]

**Description:** An opt-in loader mode that expands aliases inline.
The loader tracks a total expanded-node counter guarded by
`max_expanded_nodes` (default 1 000 000) to prevent Billion Laughs
alias bombs. Circular aliases are detected via an `in_progress` set
and returned as `LoadError::CircularAlias`.
**Complexity:** Medium
**Comment:** Resolved mode is needed by tools that want a fully
materialized document. The expansion limit and cycle detection are
defence-in-depth; the primary recommendation for untrusted input is
lossless mode.
**Tier:** 1

### Security Limits [completed]

**Description:** Eight compile-time constants (in `src/limits.rs`) cap
inputs from untrusted sources. All limits return structured
`Error`/`LoadError` values — never panics:
- `MAX_COLLECTION_DEPTH` (512) — combined block + flow nesting depth;
  unified to prevent bypass by mixing sequence and mapping nesting.
- `MAX_ANCHOR_NAME_BYTES` (1 024) — anchor and alias name scanning;
  prevents CPU exhaustion on degenerate long names.
- `MAX_TAG_LEN` (4 096) — raw scanned tag (verbatim URI or suffix).
- `MAX_COMMENT_LEN` (4 096) — per-line comment body scanning.
- `MAX_DIRECTIVES_PER_DOC` (64) — `%YAML` + `%TAG` directives per
  document; prevents HashMap exhaustion.
- `MAX_TAG_HANDLE_BYTES` (256) — `%TAG` handle length.
- `MAX_RESOLVED_TAG_LEN` (4 096) — fully-resolved tag string after
  `%TAG` prefix expansion; prevents allocation of oversized resolved
  strings.
- `MAX_SCALAR_LEN` (1 048 576 = 1 MiB) — quoted scalar length for both
  single-quoted and double-quoted styles; applied uniformly on the
  borrow path (no escapes), the escape-decode (owned) path, and
  accumulated multi-line length; prevents DoS via unbounded scalar
  allocation.
**Complexity:** Low
**Comment:** All limits are generous for real-world YAML (Kubernetes
documents rarely exceed 20 levels deep; tags are under 30 bytes) while
bounding worst-case CPU and memory usage.
**Tier:** 1

### Hex Escape Security Hardening [completed]

**Description:** The double-quoted scalar lexer applies two security checks to
hex escapes (`\x`, `\u`, `\U`) that go beyond what YAML 1.2.2 §5.7 requires:
(1) `quoted.rs:594-606` — the decoded character must be a `c-printable`
codepoint; non-printable hex-escape sequences are rejected with a parse error;
(2) `quoted.rs:608-618` — hex escapes that decode to a bidi-override character
(U+202A–U+202E, U+2066–U+2069) are rejected. Named escapes (`\0`, `\a`, `\b`,
`\e`, `\N`, etc.) are intentionally exempt from both checks — they produce
well-known control characters whose semantics are unambiguous. This is a
deliberate divergence from the spec, recorded as `Strict (security-hardened)`
in `docs/conformance/bnf-§5.md` entries [59]–[61].
**Complexity:** Low
**Comment:** The spec permits any codepoint via hex escapes, but accepting
arbitrary non-printable or bidi-override codepoints through a YAML file is a
security risk in LSP and pipeline contexts. Named escapes are exempt because
their output is predictable; hex escapes are not.
**Tier:** 1

### Implicit Mapping Key Length Limit [completed]

**Description:** Implicit mapping keys (those without a leading `?` indicator) are
capped at 1024 Unicode characters in both flow context (YAML 1.2 §7.4.3) and block
context (§8.2.2). A key whose `:` value indicator appears more than 1024 characters
from the key start is rejected with a parse error. Explicit `?`-introduced keys are
not subject to this limit.
**Complexity:** Low
**Comment:** The spec mandates this limit to bound parser lookahead. Enforcement
closes four previously-Lenient conformance entries ([154], [155], [192], [193]) and
brings the parser to full conformance on this point. Only implicit keys are affected;
explicit key content remains unrestricted.
**Tier:** 1

### Encoding Detection and Decoding [completed]

**Description:** The `encoding` module implements YAML 1.2 §5.2
encoding detection. Detects UTF-8, UTF-16 LE/BE, and UTF-32 LE/BE
via BOM and null-byte heuristic. Decodes any supported encoding to
UTF-8 and strips the BOM at stream start. Normalizes CRLF and lone CR
to LF. BOM is also accepted (stripped) at document-prefix positions
within a multi-document stream, implementing the `c-byte-order-mark?`
component of `l-document-prefix` (§9.1.1) via `signal_document_boundary()`
in `lines.rs`.
**Complexity:** Medium
**Comment:** UTF-32 BOM detection precedes UTF-16 because the UTF-32
LE BOM (`FF FE 00 00`) is a prefix of the UTF-16 LE BOM (`FF FE`).
**Tier:** 1

### Span Tracking [completed]

**Description:** Every event carries a `Span` covering the source
bytes that produced it — `start` and `end` are both `Pos` values
(byte offset, line, column). Zero-width spans mark synthetic events
(e.g. `StreamStart`). The loader propagates spans into the AST so
every `Node` carries source location for LSP diagnostics.
**Complexity:** Low
**Comment:** Accurate spans are essential for LSP use. The parser
tracks both byte offsets (for range operations) and line/column
(for LSP `Position` types) in a single `Pos` struct to avoid
redundant re-scanning.
**Tier:** 1

### Streaming Event API [completed]

**Description:** `parse_events(input)` returns a lazy
`Iterator<Item = Result<(Event, Span), Error>>` that produces
events on demand. First-event latency is O(1) — the caller receives
`StreamStart` before any bulk processing. The iterator is zero-copy
for most scalars: `Event::Scalar.value` is a `Cow::Borrowed(&str)`
that slices directly from input when no transformation is needed.
**Complexity:** Medium
**Comment:** The streaming design is a fundamental architectural
decision. It allows the LSP to begin processing before the full
document is parsed and avoids materializing an intermediate
representation when the caller only needs events.
**Tier:** 1

### Anchors and Aliases [completed]

**Description:** `&name` anchor definitions and `*name` alias
references are scanned and included in the respective events as
`anchor: Option<&str>` and `Event::Alias { name }`. The loader
builds an anchor map and resolves aliases in resolved mode or
preserves them as `Node::Alias` in lossless mode.
**Complexity:** Medium
**Comment:** Zero-copy: anchor and alias names borrow directly from
input without allocation.
**Tier:** 1

### Tag Resolution [completed]

**Description:** All four tag forms are recognized and resolved at
parse time: verbatim (`!<URI>`), shorthand with `!!` default handle
(`!!str` → `tag:yaml.org,2002:str`), shorthand with user-defined
handles from `%TAG` directives, and local tags (`!suffix`). Resolved
tags are included in `Scalar`, `SequenceStart`, and `MappingStart`
events.
**Complexity:** Medium
**Comment:** Tag resolution against `%TAG` directives is performed
at scan time. The resolved string is `Cow::Borrowed` for verbatim
tags and `Cow::Owned` for expanded shorthands.
**Tier:** 1

### Block Scalar Chomping [completed]

**Description:** All three chomping modes are supported for block
scalars: strip (`-`), clip (default), and keep (`+`). The `Chomp`
enum is part of `ScalarStyle::Literal(Chomp)` and
`ScalarStyle::Folded(Chomp)`.
**Complexity:** Low
**Comment:** Chomping controls trailing newline handling. All three
modes are required for spec conformance.
**Tier:** 1

### Multi-document Support [completed]

**Description:** A YAML stream can contain multiple documents
separated by `---` and optionally terminated by `...`. The parser
emits `DocumentStart`/`DocumentEnd` events for each document, carries
the `%YAML` version and `%TAG` directives in `DocumentStart`, and the
loader returns a `Vec<Document<Span>>`.
**Complexity:** Low
**Comment:** Each document gets a fresh anchor map in the loader;
anchors do not cross document boundaries.
**Tier:** 1

### Loader Conformance — Full AST Fidelity [completed]

**Description:** The `load()` API passes 375/375 loader conformance
cases derived from the YAML Test Suite. Every valid input that the
event stream accepts is correctly materialized into a `Vec<Document>`,
preserving scalars, collections, anchors, tags, multi-document
streams, and empty documents.
**Complexity:** High
**Comment:** A correct event stream does not automatically imply a
correct AST — the loader is a separate conformance surface that must
be tested independently. Gaps found and fixed include empty-document
handling and anchor/alias resolution edge cases.
**Tier:** 1

### Document Marker Flags in AST [completed]

**Description:** `Document<Loc>` exposes two new boolean fields:
`explicit_start` (set when the document begins with a `---` marker)
and `explicit_end` (set when the document ends with a `...` marker).
The flags are populated by the loader from `DocumentStart`/
`DocumentEnd` events and preserved in the AST for downstream consumers
(e.g. the formatter round-trips these markers faithfully).
**Complexity:** Low
**Comment:** Required for formatter conformance — documents with
explicit `---`/`...` markers must have them preserved in formatted
output. The flags are sourced from the event stream, so no extra
parsing is needed; the loader already consumed both events.
**Tier:** 1

### YAML 1.2 Conformance [completed]

**Description:** The parser passes 368/368 cases in the YAML Test
Suite (all valid and invalid test cases). Spec-faithful implementation
following YAML 1.2 §§5–9.
**Complexity:** High
**Comment:** 100% conformance on the authoritative test suite.
Achieved across block sequences/mappings, flow collections, all scalar
styles, directives, anchors, aliases, multi-document streams, and
error cases.
**Tier:** 1

### All Scalar Styles [completed]

**Description:** All five YAML scalar styles are supported: plain,
single-quoted, double-quoted, literal block (`|`), and folded block
(`>`). Line folding is applied for folded scalars. Escape sequences
are decoded for double-quoted scalars.
**Complexity:** Medium
**Comment:** The scalar value in events is the fully decoded logical
content — callers do not need to handle quoting or escape sequences.
**Tier:** 1

### YAML Directives (`%YAML`, `%TAG`) [completed]

**Description:** `%YAML` version directives and `%TAG` handle
directives are parsed and scoped to the document they precede. The
version tuple is carried in `DocumentStart`. Custom tag prefixes from
`%TAG` are applied during tag resolution.
**Complexity:** Low
**Comment:** Directive scope resets at each `---` marker, matching
YAML 1.2 §6.8.
**Tier:** 2

### Explicit Keys (`? key:`) [completed]

**Description:** YAML explicit mapping keys (`? ` indicator) are
supported, including multi-line key content and keys that are
themselves block sequences or mappings.
**Complexity:** Medium
**Comment:** Explicit keys interact with `? ` on the preceding line
and block sequence indicators — handled via `explicit_key_pending`
state.
**Tier:** 2

### §10 Schema Resolution [completed]

**Description:** The loader applies YAML 1.2.2 §10 schema tag resolution to every `load()` call.
`Schema::Core` (§10.3) is the default, matching the YAML spec recommendation for processors.
Three schemas are selectable via `LoaderBuilder::schema(schema)`:
- `Schema::Failsafe` (§10.1) — all untagged scalars resolve to `tag:yaml.org,2002:str`; all
  untagged sequences to `tag:yaml.org,2002:seq`; all untagged mappings to `tag:yaml.org,2002:map`.
  The `!` non-specific tag resolves by kind.
- `Schema::Json` (§10.2) — untagged plain scalars are matched against the JSON pattern table
  (`null`, `true|false`, integer, float); non-matching plain scalars are rejected with
  `LoadError::UnresolvedScalar`. Non-plain scalars resolve to `str`. Untagged collections resolve
  by kind.
- `Schema::Core` (§10.3, default) — superset of JSON; unmatched plain scalars fall back to
  `tag:yaml.org,2002:str` instead of being rejected.

Explicit source tags always take precedence over schema-derived resolution. Schema-resolved tags
have `tag_loc: None` (no source position); source-tagged nodes have `tag_loc: Some`. Callers
that need raw unresolved tags should use `parse_events()`, which is schema-agnostic.
**Complexity:** Medium
**Comment:** `Schema::Core` as the default follows YAML 1.2.2 §10.3 ("The Core Schema is the
recommended default schema that YAML [processors] should use unless instructed otherwise").
Schema resolution is decoupled from the streaming event layer and lives entirely in the loader.
**Tier:** 1

### Event and Node Variant Memory Layout Optimization [completed]

**Description:** Two-stage restructuring of the hot types in the event pipeline:

*Stage A — Node variants.* `Node::Scalar`, `Node::Mapping`, and `Node::Sequence`
carry rare fields (`anchor`, `anchor_loc`, `tag_loc`, `leading_comments`,
`trailing_comment`) behind `meta: Option<Box<NodeMeta>>`. `Node<Span>` size:
288 bytes → 120 bytes per variant.

*Stage B — Event variants.* `Event::Scalar`, `Event::SequenceStart`, and
`Event::MappingStart` carry their anchor and tag fields (`anchor`, `anchor_loc`,
`tag`, `tag_loc`) behind `meta: Option<Box<EventMeta<'input>>>`. The common
case — no anchor, no source-text tag — pays only one 8-byte pointer; source-text
tags and anchors are rare in block-heavy and Kubernetes documents.
`Event` size: 40 bytes (was ~112 bytes per node variant with four inline fields).
Accessor methods `anchor()`, `anchor_loc()`, `tag()`, `tag_loc()` on `Event`
replace direct field access; patterns that previously destructured these four
fields by name must use the accessor methods.
**Complexity:** Medium
**Comment:** Stage A is a semver-breaking API change. Stage B extends
it without an additional version bump — the accessor-method migration is the
same pattern as Stage A. The `tag` field is boxed in `EventMeta` (unlike `Node`
where tag is kept inline because the schema resolver populates it on every loaded
node); events carry a tag only when the source text contained one, which is rare.
**Tier:** 2

### Zero-Allocation Resolver-Injected Tags [completed]

**Description:** `Node::tag` changed from `Option<String>` to
`Option<Cow<'static, str>>`. Tags injected by the schema resolver
(`apply_schema_to_node`) are now `Cow::Borrowed(&'static str)`,
eliminating four heap allocations per loaded node in typical documents.
User-authored tags from the input stream remain `Cow::Owned(String)`.
Callers that previously read `tag` as `Option<String>` must update to
`Option<Cow<'static, str>>` — `as_deref()` and string comparisons
continue to work unchanged via `Deref<Target = str>`.
**Complexity:** Low
**Comment:** The `'static` lifetime bound matches `ResolvedTag::as_str()`
which returns `&'static str` constants. User-authored tags need owned
storage because they are derived from the input buffer which does not
outlive the AST. This is a semver-breaking API change (0.6 → 0.7).
**Tier:** 2

### Block-Sequence Plain Scalar Fast Path [completed]

**Description:** A scan optimization for the common pattern of a
plain scalar on a block-sequence line (`- value`). The fast path
avoids re-scanning the plain scalar from the raw line buffer, reducing
per-item overhead in large flat sequences.
**Complexity:** Low
**Comment:** Added after profiling showed block-sequence parsing as
the hottest path for typical Kubernetes YAML. Verified by the
existing benchmark suite.
**Tier:** 2

---

### Schema-Based Type Coercion [won't implement]

**Description:** Interpret scalar values according to a schema
(e.g. coerce `"true"` → `bool`, `"42"` → `i64`) in the parser
itself.
**Complexity:** Medium
**Comment:** Type coercion is a loader/application concern, not a
parser concern. The parser's job is to produce the logical scalar
string — callers apply their own schema. Adding coercion would couple
the parser to schema logic and make it unsuitable as a general-purpose
streaming layer.
**Tier:** 4

### Pull-Based (Push) Incremental Parsing [won't implement]

**Description:** Accept input in chunks, allowing parsing of very
large YAML streams that do not fit in memory.
**Complexity:** Very High
**Comment:** The parser operates on a `&str` slice. Incremental
chunked input would require a fundamental redesign of the lexer to
handle tokens that span chunk boundaries. The LSP use case operates
on whole documents held in memory by the editor.
**Tier:** 4
