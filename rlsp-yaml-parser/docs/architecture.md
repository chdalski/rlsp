<!-- SPDX-License-Identifier: MIT -->

# rlsp-yaml-parser Architecture

This document describes the internal design of `rlsp-yaml-parser` for
contributors and AI agents working on the codebase. It explains the
processing pipeline, how the pieces fit together, and the key design
decisions behind them.

## Table of Contents

1. [Streaming architecture](#streaming-architecture)
2. [Processing pipeline](#processing-pipeline)
3. [Line buffer (`src/lines.rs`)](#line-buffer)
4. [Lexer (`src/lexer/`)](#lexer)
5. [Event iterator (`src/event_iter/`)](#event-iterator)
6. [Loader (`src/loader/`)](#loader)
7. [AST types (`src/node.rs`)](#ast-types)
8. [Position and span tracking (`src/pos.rs`)](#position-and-span-tracking)
9. [Security limits (`src/limits.rs`)](#security-limits)
10. [Key design decisions and trade-offs](#key-design-decisions-and-trade-offs)

---

## Streaming architecture

The parser is a streaming event iterator. It processes the input one line at
a time and yields one `(Event, Span)` pair per call to `Iterator::next`. It
never reads ahead of what is needed to produce the current event, so the first
event is available after inspecting exactly one line of input — O(1) first-event
latency regardless of document size.

**Why events instead of a tree?**

A tree representation requires scanning the entire document before any caller
can act on it. For a language server that displays diagnostics as the user
types, latency to the first structural signal matters more than throughput for
large files. An event stream lets the server start reacting after receiving the
first few events — typically after just a few hundred nanoseconds, well before
a multi-megabyte document is fully parsed.

Callers that need a full tree use the loader (see [Loader](#loader)), which
consumes the event stream and builds an AST. Callers that only need to scan
structure (e.g., to locate a key without caring about the value) can stop
iteration early and discard the rest.

**Zero-copy where possible.** Most `Event` variants borrow directly from the
input `&'input str`. Scalars that require transformation (line folding in block
scalars, escape decoding in double-quoted scalars) use `Cow<'input, str>` —
`Cow::Borrowed` for the common untransformed case, `Cow::Owned` only when
a new string must be built.

---

## Processing pipeline

```
input: &str
  │
  ▼
LineBuffer (src/lines.rs)
  │  splits raw bytes into Line structs with indent/pos/break-type
  │
  ▼
Lexer (src/lexer/, src/lexer.rs)
  │  wraps LineBuffer; classifies lines and implements per-style
  │  scalar scanners (plain, single-quoted, double-quoted, literal, folded)
  │
  ▼
EventIter (src/lib.rs + src/event_iter/)
  │  state machine: BeforeStream → BetweenDocs ⇄ InDocument → Done
  │  drives the Lexer; produces Event + Span items
  │  public entry point: parse_events(input) → impl Iterator<…>
  │
  ▼  (optional — callers that only want events stop here)
Loader (src/loader/, src/loader.rs)
  │  consumes the event stream
  │  builds Vec<Document<Span>> AST
  │  attaches comments to adjacent nodes
  │  resolves or preserves aliases depending on LoadMode
  │
  ▼
Vec<Document<Span>>  (src/node.rs)
```

---

## Line buffer

**File:** `src/lines.rs`

`LineBuffer` is the lowest layer. It wraps a `&'input str` and slices it into
`Line` values one at a time. Each `Line` carries:

- `content` — the line text excluding its terminator, as a `&'input str` slice
- `indent` — the number of leading space characters (tabs are not counted;
  they are a YAML syntax error in indentation context and are rejected by the
  lexer)
- `pos` — the `Pos` of the first byte of the line
- `break_type` — `Lf`, `Cr`, `CrLf`, or `Eof`

`LineBuffer` keeps exactly one line primed in an internal slot (`VecDeque`
with capacity 1). The `peek_next_line()` method returns the primed line without
consuming it; `consume_line()` removes it and primes the next. This single-line
lookahead is sufficient for the state machine to decide what to do next without
scanning further.

No line is read before the state machine needs it — this is what makes
first-event latency O(1).

---

## Lexer

**Files:** `src/lexer.rs`, `src/lexer/block.rs`, `src/lexer/comment.rs`,
`src/lexer/plain.rs`, `src/lexer/quoted.rs`

`Lexer` wraps `LineBuffer` and provides line-classification and scalar-scanning
primitives. The `EventIter` state machine calls into `Lexer` rather than
operating on `LineBuffer` directly, so the grammar logic stays clean.

### Classification methods

The lexer exposes boolean predicates used by the state machine before
consuming anything:

- `is_directives_end()` — the next line is `---`
- `is_document_end()` — the next line is `...`
- `is_comment_line()` — the next line starts with `#` (after optional leading spaces)
- `is_directive_line()` — the next line starts with `%`
- `at_eof()` — no more lines remain
- `has_content()` — the next line has non-whitespace content

### Scalar scanners

Each YAML scalar style has its own scanning method:

| Method | Style |
|--------|-------|
| `try_consume_plain_scalar(parent_indent)` | Plain (unquoted) |
| `try_consume_single_quoted(parent_indent)` | Single-quoted `'…'` |
| `try_consume_double_quoted(block_indent)` | Double-quoted `"…"` |
| `try_consume_literal_block_scalar(parent_indent)` | Literal block `\|` |
| `try_consume_folded_block_scalar(parent_indent)` | Folded block `>` |

Each scanner returns `Option<Result<(Cow<'input, str>, …), Error>>`: `None`
when the current position is not a valid start for that style, `Err` for a
malformed input, `Ok` for a successfully scanned value.

### Side-channel fields

The lexer uses a few `Option` fields as side channels for information that
needs to be passed from a scanner to its caller without a complicated return
type:

- `inline_scalar` — scalar content that immediately follows a `---` marker on
  the same physical line (e.g. `--- value`); drained by the next plain-scalar
  call
- `trailing_comment` — a `# comment` found on the same line as a plain scalar;
  drained by the state machine after it emits the scalar event
- `pending_multiline_tail` — content following the closing quote of a
  multiline quoted scalar; used by the flow parser to continue processing
  `,`, `]`, `}` that appear after the closing quote
- `plain_scalar_suffix_error` — an error detected in the suffix after a plain
  scalar (e.g. a NUL byte), reported after the scalar event rather than
  instead of it
- `marker_inline_error` — an error produced when a marker line carries invalid
  inline content; drained immediately after `consume_marker_line`

---

## Event iterator

**Files:** `src/lib.rs` (outer struct and `Iterator` impl), `src/event_iter.rs`
(re-exports), `src/event_iter/` (implementation split across sub-modules)

`EventIter` is the struct that implements `Iterator<Item = Result<(Event, Span), Error>>`.
It owns the `Lexer` and all parser state. `parse_events(input)` constructs an
`EventIter` and returns it as an opaque `impl Iterator`.

### Top-level state machine

`IterState` has four variants:

| State | Description |
|-------|-------------|
| `BeforeStream` | Initial state; emits `StreamStart` and transitions to `BetweenDocs` |
| `BetweenDocs` | Between documents: consumes blank lines, comments, and directives; detects `---` or bare content |
| `InDocument` | Inside a document: consumes lines until a boundary marker or EOF |
| `Done` | Terminal state after `StreamEnd` or a fatal error |

`Iterator::next` is a tight loop:

1. Drain the event queue (`VecDeque<(Event, Span)>`) if non-empty.
2. Call the appropriate step function for the current state.
3. If the step returns `StepResult::Continue`, loop; if it returns
   `StepResult::Yield(result)`, return `result`.

The queue exists because a single parse step can produce multiple consecutive
events — for example, opening a sequence emits `SequenceStart` before the
first item, and dedenting out of several nested collections emits multiple
`SequenceEnd`/`MappingEnd` events at once.

### BetweenDocs step (`src/event_iter/directives.rs`)

`step_between_docs` calls `consume_preamble_between_docs`, which loops
over:
- blank lines (silently consumed)
- comment lines (pushed to the event queue as `Event::Comment`)
- directive lines (`%YAML`, `%TAG`, or reserved; accumulated into
  `self.directive_scope`)

When a `---` marker or bare document content is detected, the state transitions
to `InDocument` and a `DocumentStart` event is emitted with the version and
tag-directive information collected in `directive_scope`.

`directive_scope` is reset at each document boundary. The `DirectiveScope`
struct (`src/event_iter/directive_scope.rs`) holds `%YAML` version, custom
`%TAG` handle-to-prefix mappings, and a directive counter (used for the
`MAX_DIRECTIVES_PER_DOC` limit). Tag resolution happens through
`DirectiveScope::resolve_tag`, which expands `!!suffix` to the yaml.org
namespace and custom `!handle!suffix` forms via registered prefixes.

### InDocument step (`src/event_iter/step.rs`)

`step_in_document` is the main dispatcher. Each call:

1. Calls `skip_and_collect_comments_in_doc` to skip blank lines and queue any
   comment events.
2. Checks for tab-indented lines (rejected per YAML 1.2 §6.1).
3. Checks for document-boundary markers (`---`, `...`) or EOF to close all
   open collections and emit `DocumentEnd`.
4. Peeks at the next line and dispatches to the appropriate handler based on
   the leading byte(s): `#` (comment), `*` (alias), `&` (anchor), `!` (tag),
   `-` (sequence entry), `?`/`:` (explicit mapping key/value), flow collection
   delimiters, or scalar content.

### Collection stack (`src/event_iter/state.rs`)

Open block collections are tracked on `coll_stack: Vec<CollectionEntry>`.

`CollectionEntry` has two variants:

- `Sequence(indent_col, has_had_item)` — an open block sequence
- `Mapping(indent_col, MappingPhase, has_had_value)` — an open block mapping
  with a phase flag (expecting a key or a value)

When a new line is less-indented than the top of the stack, collections are
closed via `close_collections_at_or_above(threshold, pos)`, which pops entries
and pushes `SequenceEnd`/`MappingEnd` events until the stack depth matches the
new indent. `close_all_collections` handles document-end closure.

The stack depth is bounded by `MAX_COLLECTION_DEPTH` (512). This limit is
checked at push time; exceeding it returns an `Error`.

### Pending anchor and tag

Properties (anchors and tags) precede the node they annotate. After scanning
`&name` or `!tag`, the parser stores the result in `pending_anchor` or
`pending_tag`. These are consumed and attached to the next `Scalar`,
`SequenceStart`, or `MappingStart` event by `try_consume_scalar` and the
collection-open handlers.

`PendingAnchor` and `PendingTag` are enums with two variants — `Standalone`
(the property was on its own line, applies to the next node of any type) and
`Inline` (the property was inline with key content, applies to the key scalar
rather than the enclosing mapping). This distinction is necessary to correctly
handle cases like `&anchor key: value` vs `&anchor\n- item`.

### Block collection handlers (`src/event_iter/block/`)

`src/event_iter/block.rs` re-exports `mapping` and `sequence` sub-modules.
These contain the methods that handle sequence entry lines (`- item`) and
mapping entry lines (`key: value` or `? key`). They call
`close_collections_at_or_above` before opening or advancing collections, and
call `try_consume_scalar` to scan the inline value if present.

### Flow collection handler (`src/event_iter/flow.rs`)

Flow collections (`[…]`, `{…}`) are fully parsed in a single call to
`handle_flow_collection` before returning. Unlike block collections, they do
not leave an entry on `coll_stack`. The combined depth limit (block + flow) is
enforced inside `handle_flow_collection` by summing the block stack length with
a local flow-frame count.

### Line mapping helpers (`src/event_iter/line_mapping.rs`)

Utility functions for inspecting a single line of text to detect mapping
indicators (`key: value` patterns), used by the mapping handlers to classify
lines without consuming them.

### Properties scanner (`src/event_iter/properties.rs`)

`scan_anchor_name` and `scan_tag` extract anchor names and tag strings from a
line slice, enforcing `MAX_ANCHOR_NAME_BYTES` and `MAX_TAG_LEN` limits. Tag
resolution via `DirectiveScope::resolve_tag` is called inside `scan_tag`.

---

## Loader

**Files:** `src/loader.rs`, `src/loader/comments.rs`, `src/loader/reloc.rs`,
`src/loader/stream.rs`

The loader consumes the event stream from `parse_events` and builds a
`Vec<Document<Span>>`. It is optional — callers that only need the raw event
stream skip it.

### Entry points

| Entry point | Description |
|-------------|-------------|
| `load(input)` | Convenience function: lossless mode, default limits |
| `LoaderBuilder::new()…build().load(input)` | Configurable |
| `Loader::load(input)` | Lower-level; requires a constructed `Loader` |

### Load modes (`LoadMode`)

- **Lossless** (default): alias references are preserved as `Node::Alias`
  nodes. No expansion occurs. Safe for untrusted input without any expansion
  limit because no tree growth happens.
- **Resolved**: aliases are expanded inline by deep-cloning the anchor's
  subtree into the alias site. Subject to the `max_expanded_nodes` limit.

The language server uses lossless mode. Resolved mode is available for callers
that need a fully materialised document.

### Internal state (`LoadState`)

`LoadState` holds:

- `anchor_map: HashMap<String, Node<Span>>` — registered anchors for the
  current document (cleared between documents)
- `anchor_count: usize` — count of distinct anchors (checked against
  `max_anchors`)
- `depth: usize` — current nesting depth (incremented on Begin events,
  decremented on End events; checked against `max_nesting_depth`)
- `expanded_nodes: usize` — running count of nodes produced by alias expansion
  in resolved mode (checked against `max_expanded_nodes`)

### Document parsing loop

`LoadState::run` reads `StreamStart`, then loops over `DocumentStart` events,
calling `reset_for_document` (clears anchor map, resets counts) at the start of
each document. Inside each document it calls `parse_node` recursively.

`parse_node` matches on the next event:

- `Scalar` → `Node::Scalar` (with anchor registration if anchored)
- `MappingStart` → enters a loop, collecting (key, value) pairs until
  `MappingEnd`; increments depth on entry, decrements on exit
- `SequenceStart` → enters a loop, collecting items until `SequenceEnd`;
  increments/decrements depth
- `Alias` → delegates to `resolve_alias`
- `Comment` → skips and recurses
- Structural boundary events (e.g. `StreamEnd`) → returns an empty scalar

### Anchor and alias resolution

**Registration.** When a node with an anchor is fully parsed, `register_anchor`
stores a clone of the node in `anchor_map`. If the anchor name was already
present, it overwrites the previous entry without incrementing `anchor_count`
(re-definition). Anchor counts are tracked per document; the map is cleared
between documents so anchors from one document do not bleed into the next.

**Lossless resolution.** `resolve_alias` in lossless mode returns a
`Node::Alias` node with the anchor name. No map lookup occurs.

**Resolved expansion.** In resolved mode, `resolve_alias` looks up the anchor
name in `anchor_map`, then calls `expand_node` recursively. `expand_node`
increments `expanded_nodes` before recursing into children. Circular references
are detected via an `in_progress: HashSet<String>` passed through the
recursion. After expansion, `reloc` re-stamps the result with the alias site's
span so positions remain correct.

Note: `expand_node` does not detect the case where an anchor-within-expansion
references a previously defined anchor (indirect cycle through a second
traversal). The `expanded_nodes` limit provides the backstop.

### Comment attachment strategy

Comments are emitted as `Event::Comment` items in the event stream. The loader
converts these to strings and attaches them to adjacent AST nodes.

**Leading comments.** Before parsing each mapping key or sequence item, the
loader drains all preceding `Comment` events via `consume_leading_comments`. The
resulting `Vec<String>` is passed to `attach_leading_comments`, which writes it
into the next node's `leading_comments` field. Document-level leading comments
are handled separately by `consume_leading_doc_comments` and stored in
`Document::comments`.

**Trailing comments.** After parsing a mapping value or sequence item, the
loader peeks at the event stream via `peek_trailing_comment`. A comment is
considered trailing if it appears on the same line as the node's span end
(`span.end.line`). If found, it is attached to the node's `trailing_comment`
field. Comments on a different line are left in the stream to be picked up as
leading comments for the next node.

**Limitation.** Document-prefix leading comments (before the first node of a
document) are discarded by the tokenizer per YAML §9.2. The `Document::comments`
field captures block-level comments (those with `span.end.line > span.start.line`)
seen between `DocumentStart` and the root node.

---

## AST types

**File:** `src/node.rs`

`Node<Loc>` is the core AST type, parameterized by its location type (typically
`Loc = Span`). The loader produces `Vec<Document<Span>>`.

```
Document<Span>
  root: Node<Span>
  version: Option<(u8, u8)>       -- from %YAML directive
  tags: Vec<(String, String)>     -- from %TAG directives (handle, prefix)
  comments: Vec<String>           -- document-level leading comments

Node<Span>  =  Scalar { value, style, anchor, tag, loc, leading_comments, trailing_comment }
             | Mapping { entries: Vec<(Node, Node)>, anchor, tag, loc, … }
             | Sequence { items: Vec<Node>, anchor, tag, loc, … }
             | Alias { name, loc, … }   -- lossless mode only
```

`Node::Alias` is produced only in lossless mode. In resolved mode, aliases are
expanded and no `Alias` nodes appear in the output.

---

## Position and span tracking

**File:** `src/pos.rs`

`Pos` records a position within the input:

- `byte_offset` — 0-based byte offset from the start of the input
- `line` — 1-based line number
- `column` — 0-based codepoint column within the current line

`Pos::advance(ch)` returns a new `Pos` advanced past one character. For `'\n'`
it increments `line` and resets `column`; for other characters it increments
`column` by 1 and `byte_offset` by `ch.len_utf8()`.

`column_at(line_content, byte_offset_in_line)` computes a codepoint column
with an ASCII fast path: if the prefix is pure ASCII, the column equals the
byte offset (1 byte = 1 codepoint). Non-ASCII input falls back to
`chars().count()`.

`Span` is a half-open `[start, end)` range of `Pos` values covering the input
bytes that contributed to an event or AST node. Zero-width spans (equal start
and end) are used for synthetic events (`StreamStart`, `StreamEnd`) and for
empty scalars generated when a document has no root node.

---

## Security limits

**File:** `src/limits.rs`

All limits are constants enforced during parsing; exceeding any returns an
`Error` or `LoadError`, never a panic.

### Parser limits (enforced by `EventIter`)

| Constant | Default | Enforced at |
|----------|---------|-------------|
| `MAX_COLLECTION_DEPTH` | 512 | `coll_stack.push` in `EventIter` |
| `MAX_ANCHOR_NAME_BYTES` | 1 024 B | `scan_anchor_name` in `properties.rs` |
| `MAX_TAG_LEN` | 4 096 B | `scan_tag` in `properties.rs` |
| `MAX_COMMENT_LEN` | 4 096 B | `try_consume_comment` in `lexer/comment.rs` |
| `MAX_DIRECTIVES_PER_DOC` | 64 | `parse_directive` in `directives.rs` |
| `MAX_TAG_HANDLE_BYTES` | 256 B | `parse_tag_directive` in `directives.rs` |
| `MAX_RESOLVED_TAG_LEN` | 4 096 B | `DirectiveScope::resolve_tag` |

`MAX_COLLECTION_DEPTH` is a unified limit over both sequences and mappings. A
separate per-type limit would allow nesting 512 sequences inside 512 mappings
(total depth 1 024); the unified limit keeps the bound tight.

### Loader limits (enforced by `LoadState`)

| Option | Default | Guards against |
|--------|---------|----------------|
| `max_nesting_depth` | 512 | Stack exhaustion from deeply nested collections |
| `max_anchors` | 10 000 | Unbounded anchor-map memory growth |
| `max_expanded_nodes` | 1 000 000 | Alias bombs (Billion Laughs); resolved mode only |

Loader limits are configurable via `LoaderBuilder` or `LoaderOptions`. Parser
limits are fixed constants — they guard against CPU exhaustion during scanning,
which must be bounded before any user-controlled configuration can be applied.

---

## Key design decisions and trade-offs

### Spec-faithfulness vs raw speed

The parser is designed to pass the full YAML Test Suite (368/368 test cases).
This means it implements edge cases that faster, less-faithful parsers skip:
explicit keys (`? key`), directive handling, all block-scalar chomping modes,
multi-document streams, alias/anchor scoping per document, etc. The cost is
slightly higher per-event overhead compared to a parser that skips those paths.

For the LSP use case, spec-faithfulness matters more than raw throughput —
incorrect diagnostics are worse than slightly slower correct ones.

### Zero-copy by default

`Event` values borrow from the input `&'input str` wherever possible. The
`Cow<'input, str>` type on scalar values means:
- Plain scalars, verbatim tags, comment text, and anchor names are
  `Cow::Borrowed` with no allocation.
- Double-quoted scalars with escape sequences, and folded/literal block scalars
  that require line-break normalization, are `Cow::Owned`.

The loader always converts to `String` (owned) because AST nodes must outlive
the input string.

### Line-oriented processing

The parser operates one line at a time, not one character at a time. Block
context in YAML is governed by indentation, and indentation is a per-line
property. Processing by line avoids re-scanning for indentation on every
character and simplifies the state machine: at the start of each step, the
current line's indent and content are known without additional scanning.

Flow context (inside `[…]` or `{…}`) requires character-level scanning within
a line; the flow handler runs character-by-character within the bounds of the
collected flow lines, then returns.

### Iterative dispatch to avoid stack overflow

`Iterator::next` for `EventIter` is an iterative loop, not a recursive one.
Steps that produce multiple events (e.g., closing several nested collections at
a dedent) push to the event queue and return `StepResult::Continue`, allowing
the outer loop to drain the queue before calling the next step. This prevents
deeply-nested documents from overflowing the call stack.

The loader's `parse_node` is recursive (matching tree structure), but its depth
is bounded by `max_nesting_depth` (default 512), which is well within default
Rust stack limits.

### Lossless mode as the default

The language server needs to show where anchors and aliases are defined and
used. Expanding aliases at load time would lose that information. Lossless mode
preserves `Node::Alias` nodes so the server can navigate the alias graph. It
also avoids the denial-of-service risk of alias bombs without requiring any
expansion limit — there is no expansion to limit.

Resolved mode exists for callers (e.g., configuration file processors) that
need a fully materialised document and are operating in a trusted context.
