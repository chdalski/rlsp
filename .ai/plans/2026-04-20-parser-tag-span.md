**Repository:** root
**Status:** Completed (2026-04-20)
**Created:** 2026-04-20

## Goal

Expose the `!`/`!!` tag-token span in the parser event stream and
on AST nodes — a mirror of `2026-04-20-parser-anchor-span.md` for
tags. Today the parser records a tag's *resolved name*
(`tag: Option<Cow<'input, str>>` on events, `Option<String>` on
`Node::Scalar/Mapping/Sequence`) but discards the tag's *source
position*. Downstream consumers — `semantic_tokens` specifically —
need the precise token span to emit tag highlighting tokens to
the editor.

## Context

### Current state

Direct probe of `rlsp_yaml_parser::load` confirms: a scalar with
tag records `tag: Some("tag:yaml.org,2002:int")` but the node's
`loc` covers only the value, not the `!!int` prefix. For input
`value: !!int 42`, the value scalar's `loc` points at `42`
(approximately cols 14..16); the `!!int` token at cols 7..12 has
no representation in the AST. Same shape as the pre-retrofit
anchor problem.

### Why this change

Plan `2026-04-20-retrofit-format-folding-semantic.md` Task 3
retrofits `semantic_tokens` to consume the AST. Tag tokens (`!!int`,
`!mytag`, `!<verbatim-URI>`, etc.) are part of the highlighting
output and are user-visible in editors. Without tag spans on
events/AST, the retrofit has two options: (a) drop tag-token
highlighting (user-visible regression) or (b) use brittle
byte-range arithmetic on the node's `loc`. Exposing `tag_loc` in
the parser — the mirror of `anchor_loc` — is the clean fix.

### Specifications

- YAML 1.2 §6.8 (Node Properties) — tag indicators:
  - Verbatim: `!<URI>`
  - Primary shorthand: `!!suffix`
  - Named-handle shorthand: `!handle!suffix`
  - Secondary shorthand: `!suffix`
- LSP spec: `SemanticToken` has a `length` field; the editor
  highlights the exact byte range produced by the server.

### Involved files

- `rlsp-yaml-parser/src/event.rs` — add `tag_loc: Option<Span>`
  to `Event::Scalar`, `Event::MappingStart`, `Event::SequenceStart`
- `rlsp-yaml-parser/src/event_iter/state.rs` — extend `PendingTag`
  to carry the `!name` span alongside the resolved `Cow`
- `rlsp-yaml-parser/src/event_iter/step.rs` — capture the tag
  span at the `!` recognition site (the `scan_tag(after_bang,
  tag_start, bang_pos)` call around line 424 — `bang_pos` is
  already the `Pos` of the `!` byte; the span length is the
  `tag_token_bytes = 1 + advance_past_bang` value already
  computed at the call site). Populate `tag_loc` on every node
  event that consumes a pending tag.
- `rlsp-yaml-parser/src/node.rs` — add `tag_loc: Option<Span>`
  to `Node::Scalar`, `Node::Mapping`, `Node::Sequence`
- `rlsp-yaml-parser/src/loader.rs` — thread event `tag_loc`
  through to the constructed node
- `rlsp-yaml-parser/docs/architecture.md` — update the
  `Node<Span>` schema diagram and `PendingTag` description
- Any `rlsp-yaml` consumer that exhaustively destructures these
  `Node` variants must compile after the field is added; most
  use `..` in the patterns already.

### Invariant

`tag.is_some() == tag_loc.is_some()` on every event and node.
Either both are populated (the source had a tag indicator) or
both are `None`. A corpus-wide invariant test asserts this
(mirror of the `anchor_loc` invariant test).

### Span semantics

The `tag_loc` span covers the entire tag token in the source —
from the leading `!` through the last byte of the tag content:

- Verbatim: `!<URI>` → span covers the full `!<URI>` including
  both angle brackets.
- Primary shorthand: `!!suffix` → covers both `!` bytes plus the
  full suffix.
- Named-handle: `!handle!suffix` → covers the entire handle+suffix.
- Secondary shorthand: `!suffix` → covers the leading `!` plus
  the suffix.

This matches the `tag_token_bytes = 1 + advance_past_bang` value
already computed at the call site; no new scanner logic is
needed.

## Steps

- [x] Task 1: extend event stream with tag_loc
- [x] Task 2: extend AST nodes with tag_loc

## Tasks

### Task 1: Extend the event stream with tag_loc

Committed as `21456579e7cf5ef230893d9dd37eb0bc50646620` (may be
superseded by follow-up amend for SHA recording). Add
`tag_loc: Option<Span>` to `Event::Scalar`,
`Event::MappingStart`, and `Event::SequenceStart`. Capture the
tag span in `event_iter/step.rs` at the `scan_tag(…, bang_pos)`
call site, carry it through `PendingTag`, and populate the new
event field when the pending tag is consumed by the next node
event.

- [x] Add `tag_loc: Option<Span>` field to `Event::Scalar`,
      `Event::MappingStart`, `Event::SequenceStart` in `event.rs`,
      including rustdoc: `Some(span)` when `tag` is `Some`; `None`
      otherwise; span covers the leading `!` through the last byte
      of the tag token (same semantics as `Event::Alias.loc`
      covering `*name`).
- [x] Extend `PendingTag::Standalone` and `PendingTag::Inline` to
      carry the tag's `Span` in addition to the resolved `Cow`.
      Update the existing `PendingTag::into_cow()` accessor and
      add a `PendingTag::loc()` accessor returning the span. Two
      viable shapes: tuple-variant extension (`Standalone(Cow,
      Span)` / `Inline(Cow, Span)`) or struct-variant refactor
      (`Standalone { tag: Cow, loc: Span }`). Match whichever
      shape the existing `PendingAnchor` retrofit chose for
      consistency — read `state.rs` at task start and mirror.
- [x] Capture the tag span at the `!` recognition site in
      `event_iter/step.rs` (around line 424, where
      `scan_tag(after_bang, tag_start, bang_pos)` is called).
      `bang_pos` is already the `Pos` of the `!` byte; the total
      tag-token bytes are `tag_token_bytes = 1 + advance_past_bang`
      (already computed at line 431). Construct
      `Span { start: bang_pos, end: advance(bang_pos, tag_token_bytes) }`
      and store on `PendingTag`. Use whatever `Pos`-advancement
      helper the `anchor_loc` retrofit used — matching the pattern
      for consistency. No changes to `scan_tag` in `properties.rs`
      are required.
- [x] Populate `tag_loc` on every node event that consumes a
      pending tag. Call sites in `event_iter/step.rs` that read
      `self.pending_tag.take().map(PendingTag::into_cow)` also
      read the matching `.loc()` and emit it on the event.
      Handle the `pending_collection_tag` slot symmetrically
      (mirroring `pending_collection_anchor_loc`).
- [x] Unit tests in `event_iter` covering these shapes (one
      rstest case per shape, named per lang-rust-testing.md):
      primary shorthand before a scalar (`key: !!int 42`);
      primary shorthand before a block mapping (`!!map\nk: v`);
      secondary shorthand (`key: !foo 42`); named-handle
      shorthand (`key: !h!s 42` with a `%TAG !h! tag:ex,2026:`
      directive); verbatim tag (`key: !<tag:ex,2026:t> 42`);
      standalone tag on a line (`!!seq\n- item`); tag on a
      sequence (`!!seq [1,2,3]`); tag on a flow mapping
      (`!!map {k: v}`); tag on a mapping key scalar (`!!str key:
      value`); UTF-8 tag handle content (verify span math on
      multi-byte bytes).
- [x] Invariant test: for every event produced by the parser on
      each yaml-test-suite corpus input,
      `event.tag.is_some() == event.tag_loc.is_some()`. One test
      function iterating the full corpus. (Standing team law —
      corpus mandatory.)
- [x] `cargo test -p rlsp-yaml-parser` passes with zero failures.
- [x] `cargo clippy --all-targets` passes with zero warnings.
- [x] `cargo fmt --check` passes.
- [x] The workspace still compiles. Consumers that destructure
      `Event::*` with `..` continue to work; exhaustive
      destructures must add `tag_loc`.

### Task 2: Extend AST nodes with tag_loc

Committed as `0b6f5f942141c305f477985f7ae389b4bcf42ebd` (may be
superseded by follow-up amend for SHA recording). Add
`tag_loc: Option<Span>` to `Node::Scalar`, `Node::Mapping`,
`Node::Sequence`. Populate from the event stream in `loader.rs`.
Downstream consumers in the workspace must compile.

- [x] Add `tag_loc: Option<Span>` field to `Node::Scalar`,
      `Node::Mapping`, `Node::Sequence` in `node.rs` with rustdoc
      matching the event-level semantics.
- [x] Update `loader.rs` construction sites (`Node::Scalar { ... }`,
      `Node::Mapping { ... }`, `Node::Sequence { ... }` literal
      initializers) to read the new `tag_loc` from the event and
      store it on the node.
- [x] Update the `loader/reloc.rs` helper if it relocates nodes —
      `tag_loc` is preserved through reloc the same way
      `anchor_loc` is.
- [x] Update all `Node::*` constructor helpers in `node.rs` and
      `loader/comments.rs` (and any other file) that build nodes
      with `tag: None` so they initialize `tag_loc: None` too.
- [x] Add accessor `Node::tag_loc() -> Option<Span>` mirroring
      the existing `Node::tag()` and `Node::anchor_loc()`
      accessors.
- [x] Verify all destructuring pattern matches across the
      workspace compile. Patterns using `..` continue to work;
      exhaustive patterns must add `tag_loc`.
- [x] Loader tests asserting `tag_loc` is populated correctly for
      these shapes (one rstest case per, named): scalar with
      primary tag; mapping with anchor+tag; sequence with tag;
      tag-less scalar (`tag_loc` is `None`); verbatim tag on a
      scalar; handle-based tag via `%TAG` directive.
- [x] Corpus-wide invariant test at the AST level: walk every
      node in every document produced by loading each
      yaml-test-suite corpus input; assert
      `node.tag().is_some() == node.tag_loc().is_some()` for
      every node. Mandatory corpus coverage.
- [x] Update `rlsp-yaml-parser/docs/architecture.md`: add
      `tag_loc` to the `Node<Span>` schema diagram. Update the
      `PendingTag` description to reflect the added span. Update
      the event-level schema wherever those shapes are
      enumerated.
- [x] `cargo test` across the workspace passes with zero
      failures.
- [x] `cargo clippy --all-targets` across the workspace passes
      with zero warnings.
- [x] `cargo fmt --check` passes.

## Decisions

- **Separate `tag_loc` field rather than restructuring `tag` as
  `Option<TagInfo>`.** Additive change preserves all existing
  `..` destructuring patterns. Matches the `anchor_loc` choice in
  `2026-04-20-parser-anchor-span.md`.
- **Span covers `!` through the last tag-token byte.** Matches
  the semantics of `Event::Alias.loc` covering `*name` and
  `anchor_loc` covering `&name`. Covers all four tag forms
  uniformly using the already-computed `tag_token_bytes`.
- **Pure parser change; no consumer updates beyond compilation.**
  `semantic_tokens` uses the new field in the sibling plan
  `2026-04-20-retrofit-format-folding-semantic.md`. This plan
  only delivers the data pathway.
- **Mirror of the anchor_loc plan.** Same two-task shape, same
  call-site pattern, same test coverage (including corpus
  invariant).

## Non-Goals

- Retrofitting `semantic_tokens` or any other feature — separate
  plan.
- Exposing any other property span (directives, document
  markers). Only tags are in scope here.
- Changing `Event::Alias.loc` semantics or any existing span
  conventions.
