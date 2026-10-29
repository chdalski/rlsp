**Repository:** root
**Status:** InProgress
**Created:** 2026-10-29

# Comment Runs Before a Value Use Bounded Stack in the Loader

## Goal

The loader recurses once per comment line between a mapping key and its
value, so a long run of such comments overflows the stack and aborts the
process: the release `rlsp-yaml` server dies on opening a 120 KB file
with 20 000 comment lines after `key:`. Comment runs at a value position
must load with a stack use that does not grow with the number of
comments, while every loaded document stays as it is today.

## Context

- `LoadState::parse_node` in `rlsp-yaml-parser/src/loader.rs`: its
  `Event::Comment` arm pushes the comment into `pending_leading` and
  calls `parse_node` again, one stack frame per comment with no depth
  check. Comments reach that arm only at a mapping value; before keys,
  sequence items, and the document root the loader drains comments with
  loops (`consume_leading_comments` and `consume_leading_doc_comments`
  in `src/loader/stream.rs`). All other `parse_node` recursion counts
  against `max_nesting_depth`.
- Where comments at a value position land today, through `load()`:
  before a nested collection (block or flow), they become leading
  comments of its first entry or item; before a scalar value, an empty
  value, or an alias, they become leading comments of the next key of
  the same mapping, or of the parent collection's next entry when the
  mapping ends there.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: tests pin where comments at a value position land
- [ ] Task 2: comment runs at a value position use bounded stack
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Pin where comments at a value position land

Tests through `load()` pin today's placement of comments between a key
and its value for the value shapes no test covers yet, so the fix in
Task 2 shows it changes no loaded document. Production code is
unchanged.

- [x] Tests through `load()` pin, for comments between a key and a
      scalar value, an empty value, an alias value, and a flow
      collection value, which node receives them as leading comments,
      including the case where the mapping ends after the value and the
      comments go to the parent collection's next entry
- [x] The new tests pass against the code as it is before this plan
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 2: Load comment runs at a value position with bounded stack

The `Event::Comment` arm drains the rest of the run with
`consume_leading_comments` and then calls `parse_node` once, so the
recursion is at most one level deep whatever the number of comments.

- [ ] On a thread with a 2 MiB stack, `load()` of a document with a key
      followed by 100 000 comment lines and a nested mapping returns in
      a debug build, and the nested mapping's first key carries all
      100 000 comments as leading comments; a test pins this, and it
      overflows the stack on the code before this task
- [ ] The comment tests in `loader.rs` and the tests Task 1 added pass
      with their assertions unchanged
- [ ] The yaml-test-suite loader conformance test
      (`rlsp-yaml-parser/tests/conformance/loader.rs`) passes
- [ ] A maintainer reading the `pending_leading` field doc or the
      comment on the `Event::Comment` arm can tell which comments reach
      this path and which node receives them; neither text says they
      appear only before collections
- [ ] A reader of `rlsp-yaml-parser/docs/architecture.md` finds how the
      loader handles a run of comments between a key and its value,
      which node receives them, and what bounds the recursion depth of
      `parse_node`; no passage says the `Comment` arm skips comments or
      recurses once per comment
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

## Non-Goals

- A depth bound for alias expansion in resolved mode (`expand_node`,
  and `Clone`/`Drop` over the expanded tree): `max_nesting_depth` does
  not bound the expanded tree (separate plan after this one)
- Making `parse_node` iterative for nested collections
- Changing where comments at a value position land, including comments
  before an alias that ends a document, which no node receives today

## Decisions

- **Drain with `consume_leading_comments`, then one `parse_node` call:**
  the remaining call runs today's handling of end events, end of stream,
  and errors unchanged, so every loaded document stays the same by
  construction; a hand-written loop would have to repeat that handling
  and could consume the enclosing collection's end event. One bounded
  recursive call remains. Declined: a loop that removes the recursion
  entirely.
- **The fix stays in the `Event::Comment` arm:** declined draining the
  comments in the mapping loop before each value, which leaves the
  unbounded recursion in the arm and adds a check to every mapping value
  in the `load` hot path.
- **Performance:** documents without comments at a value position take
  the same path as today; no benchmark criterion.
