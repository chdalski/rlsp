**Repository:** root
**Status:** Completed (2026-04-20)
**Created:** 2026-04-20

## Goal

Expose the `&name` anchor-token span in the parser event stream
and on AST nodes. Today the parser records an anchor's *name*
(`anchor: Option<&str>` on `Event::Scalar/MappingStart/SequenceStart`,
`Option<String>` on `Node::Scalar/Mapping/Sequence`) but discards
the anchor's *source position*. Downstream consumers — goto-definition,
find-references, prepare-rename, rename — need the precise token span
to return useful LSP responses, and without it they cannot be
retrofitted to the AST.

## Context

### Current state

Direct probe of `rlsp_yaml_parser::load` on `"key: &anchor value\n"`:

```
Scalar value="value" anchor=Some("anchor") loc=(1,13..1,18)
```

The scalar's `loc` covers only `value` (cols 13–18). The anchor
token `&anchor` at cols 5–12 has no representation in the AST.
Aliases are fine — `Node::Alias.loc` already covers the `*name`
token exactly (verified probe: `ref: *a\n` → `Alias.loc = (2,5..2,7)`).

### Why this change

The "one parser, one AST" rule in `rlsp-yaml/CLAUDE.md` forbids
re-parsing YAML structure from raw text in downstream crates. The
sibling plan `2026-04-20-retrofit-navigation-features.md` retrofits
four LSP navigation features to consume the AST. Two of those
features — `prepare_rename` and `rename` — must return the precise
`&name` range to the editor; a widened range (e.g. the whole node
`loc`) would cause the editor to replace the entire node with
`&newname`, destroying node content.

### Specifications

- YAML 1.2 §6.9 (Node Properties) — anchor indicator `&` followed
  by an anchor name
- LSP spec — `PrepareRenameResult.range` defines the text range the
  editor replaces; must be exact

### Involved files

- `rlsp-yaml-parser/src/event.rs` — add `anchor_loc: Option<Span>`
  to `Event::Scalar`, `Event::MappingStart`, `Event::SequenceStart`
- `rlsp-yaml-parser/src/event_iter/state.rs` — extend `PendingAnchor`
  to carry the `&name` span alongside the borrowed name
- `rlsp-yaml-parser/src/event_iter/step.rs` — capture the anchor
  span at the `&` recognition site (the `scan_anchor_name(after_amp,
  amp_pos)` call around line 571 — `amp_pos` is already the
  `Pos` of the `&` byte; the span end is `amp_pos` advanced by
  `1 + name.len()`) and populate `anchor_loc` on every node event
  that consumes a pending anchor
- `rlsp-yaml-parser/src/node.rs` — add `anchor_loc: Option<Span>`
  to `Node::Scalar`, `Node::Mapping`, `Node::Sequence`
- `rlsp-yaml-parser/src/loader.rs` — thread event `anchor_loc`
  through to the constructed node
- `rlsp-yaml-parser/docs/architecture.md` — update the
  `Node<Span>` schema diagram to include the new field
- Any `rlsp-yaml` consumer that destructures these `Node` variants
  (e.g. `validation/validators.rs`, `hover.rs`, code actions,
  formatter) must compile after the field is added; most use `..`
  in the destructure patterns already

### Invariant

`anchor.is_some() == anchor_loc.is_some()` on every event and node.
Either both are populated (the source had an `&name`) or both are
`None`. A test asserts this invariant.

## Steps

- [x] Task 1: extend event stream with anchor_loc
- [x] Task 2: extend AST nodes with anchor_loc

## Tasks

### Task 1: Extend the event stream with anchor_loc

Committed as `bfae6930bb84d04f419c4b7705655f856996d273` (may be
superseded by follow-up amend for SHA recording). Add
`anchor_loc: Option<Span>` to `Event::Scalar`,
`Event::MappingStart`, and `Event::SequenceStart`. Capture the
`&name` span in the lexer when the anchor indicator is parsed,
carry it through `PendingAnchor`, and populate the new event field
when the pending anchor is consumed by the next node event.

- [x] Add `anchor_loc: Option<Span>` field to `Event::Scalar`,
      `Event::MappingStart`, `Event::SequenceStart` in `event.rs`,
      including rustdoc that describes the semantics: `Some(span)`
      when `anchor` is `Some`; `None` otherwise; span covers the
      `&` indicator through the last byte of the anchor name.
- [x] Extend `PendingAnchor::Standalone` and `PendingAnchor::Inline`
      to carry the anchor's `Span` in addition to the borrowed name
      (e.g. `Standalone { name: &'input str, loc: Span }`). Update
      the existing `PendingAnchor::name()` accessor and add a
      `PendingAnchor::loc()` accessor returning the span.
- [x] Capture the anchor span at the `&` recognition site in
      `event_iter/step.rs` (around line 571, where
      `scan_anchor_name(after_amp, amp_pos)` is called).
      `amp_pos` is already the `Pos` of the `&` byte; compute the
      end position by advancing `amp_pos` by `1 + name.len()`
      (one byte for `&` plus the returned name slice length).
      Construct the `Span { start: amp_pos, end }` at this call
      site and store it on `PendingAnchor`. No changes to
      `scan_anchor_name` in `properties.rs` are required — the
      call site already has both halves.
- [x] Populate `anchor_loc` on every node event that consumes a
      pending anchor. Call sites in `event_iter/step.rs` that
      currently read `self.pending_anchor.take().map(PendingAnchor::name)`
      also read the matching `.loc()` and emit it on the event.
- [x] Unit tests in `event_iter` / `lexer` verifying the anchor
      span for these shapes (one rstest case per shape, named):
      inline anchor before a plain scalar (`key: &a value`);
      standalone anchor before a scalar on a new line
      (`key:\n  &a value`); anchor before a block mapping
      (`&a\nkey: value`); anchor before a flow mapping
      (`&a {k: v}`); anchor before a block sequence
      (`&a\n- item`); anchor before a flow sequence
      (`&a [item]`); anchor on a mapping key scalar
      (`&a key: value`); UTF-8 anchor name (`&αβγ value`);
      dotted anchor name (`&a.b.c value`).
- [x] Invariant test: for every event produced by the parser on
      a corpus sample, `event.anchor.is_some() == event.anchor_loc.is_some()`.
      Add one test function that iterates all conformance corpus
      inputs and asserts the invariant.
- [x] `cargo test -p rlsp-yaml-parser` passes with zero failures.
- [x] `cargo clippy --all-targets` passes with zero warnings.

### Task 2: Extend AST nodes with anchor_loc

Committed as `2141902ebcece6eada40bfeec866b5446efda3c7` (may be
superseded by follow-up amend for SHA recording). Add
`anchor_loc: Option<Span>` to `Node::Scalar`, `Node::Mapping`,
`Node::Sequence`. Populate from the event stream in `loader.rs`.
Downstream consumers in the workspace must compile; update any
pattern match that destructures without `..`.

- [x] Add `anchor_loc: Option<Span>` field to `Node::Scalar`,
      `Node::Mapping`, `Node::Sequence` in `node.rs` with rustdoc
      matching the event-level semantics: `Some(span)` when
      `anchor` is `Some`; `None` otherwise; span covers the
      `&` indicator through the last byte of the anchor name.
- [x] Update `loader.rs` construction sites (`Node::Scalar { ... }`,
      `Node::Mapping { ... }`, `Node::Sequence { ... }` literal
      initializers) to read the new `anchor_loc` from the event
      and store it on the node.
- [x] Update all `Node::*` constructor helpers in `node.rs` that
      build nodes with `anchor: None` so they initialize
      `anchor_loc: None` too.
- [x] Verify all destructuring pattern matches across the workspace
      (`rlsp-yaml-parser`, `rlsp-yaml`, `rlsp-fmt`) still compile.
      Patterns using `..` for un-named fields continue to work;
      patterns that enumerate all fields must add `anchor_loc`.
- [x] Loader tests asserting `anchor_loc` is populated correctly
      on the resulting AST for these shapes (one rstest case per
      shape, named): scalar with inline anchor; mapping with
      anchor; sequence with anchor; nested anchor on a mapping
      value that is itself a mapping; anchor-less scalar (`anchor_loc`
      is `None`); alias (still uses `Node::Alias.loc` which covers
      the `*name` token — unchanged).
- [x] Invariant test at the AST level: walk every node in each
      conformance corpus input's resulting AST and assert
      `node_anchor.is_some() == node_anchor_loc.is_some()`. One
      test function iterating the corpus.
- [x] Update `rlsp-yaml-parser/docs/architecture.md` to add
      `anchor_loc` to the `Node<Span>` schema line (near line 429
      of the architecture doc). The event-level schema section
      must also gain the field on `Event::Scalar`,
      `Event::MappingStart`, `Event::SequenceStart` wherever those
      shapes are enumerated in the doc.
- [x] `cargo test` across the workspace passes with zero failures.
- [x] `cargo clippy --all-targets` across the workspace passes
      with zero warnings.
- [x] `cargo fmt --check` passes.

## Decisions

- **Separate `anchor_loc` field rather than restructuring `anchor`
  as `Option<AnchorInfo>`.** Additive change preserves all existing
  destructuring patterns that use `..` and minimizes the diff
  surface. Invariant `anchor.is_some() == anchor_loc.is_some()`
  pairs the fields conceptually without a type-level coupling.
- **Span covers `&` through last name byte.** Matches the semantics
  of `Event::Alias` whose span covers `*name` (the `*` plus the
  name). Consistent with what LSP consumers need for rename edits.
- **Pure parser change; no consumer updates beyond compilation.**
  Updating downstream consumers to *use* the new field is the
  retrofit plan's job, not this plan's. This plan only delivers
  the data pathway.

## Non-Goals

- Retrofitting any `rlsp-yaml` LSP feature to use `anchor_loc`
  (separate plan: `2026-04-20-retrofit-navigation-features.md`).
- Exposing tag spans or other property spans — only anchors are
  in scope. Tags have their own use cases and will be handled
  separately if and when a consumer needs them.
- Changing the `Node::Alias.loc` semantics. Aliases already carry
  an exact token span; no change needed.
