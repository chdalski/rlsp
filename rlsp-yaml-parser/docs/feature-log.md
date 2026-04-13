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

**Description:** Seven compile-time constants (in `src/limits.rs`) cap
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
**Complexity:** Low
**Comment:** All limits are generous for real-world YAML (Kubernetes
documents rarely exceed 20 levels deep; tags are under 30 bytes) while
bounding worst-case CPU and memory usage.
**Tier:** 1

### Encoding Detection and Decoding [completed]

**Description:** The `encoding` module implements YAML 1.2 §5.2
encoding detection. Detects UTF-8, UTF-16 LE/BE, and UTF-32 LE/BE
via BOM and null-byte heuristic. Decodes any supported encoding to
UTF-8 and strips the BOM. Normalizes CRLF and lone CR to LF.
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
