**Repository:** root
**Status:** InProgress
**Created:** 2026-10-30

# The Formatter Keeps Every Comment

## Goal

The formatter deletes comments from real files: 136 of 883 files with
comments lose at least one, `a: "q" # c` becomes `a: q`, and a comment-only
file becomes empty. Every comment in a document that loads must reach the
output of `format_yaml`, the output must re-parse to the same data, and
formatting it again must change nothing.

## Context

- Comments are lost in three layers: the lexer and event iterator drop some
  without an event, the loader discards some events, and
  `rlsp-yaml/src/editing/formatter/comment_preservation.rs` re-matches lines
  by text and drops a comment when a line's spelling changes.
- `Event::Comment` documents one event per physical line
  (`rlsp-yaml-parser/src/event.rs`), so a comment without an event is a parser
  bug. The relevant productions are `s-l-comments`, `s-b-comment`, and
  `c-nb-comment-text` in <https://yaml.org/spec/1.2.2/>.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: Indent the closing bracket of a broken flow collection
- [x] Task 2: One helper reads a comment from a line suffix
- [x] Task 3: Two helpers assemble node comments and the collection colon
- [x] Task 4: Emit every comment as an event
- [x] Task 5: Store every comment event in the AST
- [x] Task 6: Print document-level comments and reduce the text matching
- [x] Task 7: Print comments at every node position
- [x] Task 8: Turn a flow collection holding a comment into block style
- [x] Task 9: Keep comments when removing duplicate keys
- [x] Task 10: Fold only own-line comments
- [ ] Task 11: Keep the data when formatting
- [ ] Task 12: Pin that no comment is lost
- [ ] Measure every `*.yml` and `*.yaml` file under `/workspace`,
      `/usr/local/cargo/registry`, and `~/.local/share/pnpm`: for each file
      that loads, the count of `#` at a line start or after whitespace, outside
      scalar spans, equals its count of `Comment` events, and the multiset of
      `Comment` texts of `format_yaml`'s output equals the input's; the number
      of files that fail is 0
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Indent the closing bracket of a broken flow collection

`format_yaml` breaks a long flow collection under a block parent with its
closing bracket at the key's column. The parser and libfyaml reject that form,
because [197] `s-l+flow-in-block(n)` indents flow content at n+1.

- [x] With `print_width: 20`, `format_yaml` of `items: [alpha, bravo, charlie]`
      and of `k:\n  items: [alpha, bravo, charlie]` yields output that `load()`
      accepts with the same data, and a second format changes nothing; the same
      holds for a long flow mapping and a flow collection in a sequence item
- [x] The `flow-*-long-breaks` fixtures in `rlsp-yaml/tests/fixtures/formatter/`
      expect the closing bracket at the column of the items
- [x] A scalar whose text needs quotes keeps them:
      `- [ a, "Up, up and away!", b ]\n` formats to `- [a, "Up, up and away!", b]\n`,
      and `a: {b: "x, y", "c]d": 1}\n`, `k: "a:"\n`, and `- "a:"\n` re-parse to the
      same data
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 2: One helper reads a comment from a line suffix

The suffix logic that finds and validates a trailing comment is copied in
`lexer/plain.rs`, `event_iter/block/sequence.rs`, and `lexer.rs`, and Task 4
needs it at more sites. Behavior is unchanged, apart from the one change named
in the first criterion.

- [x] One function in `rlsp-yaml-parser/src/lexer/comment.rs` holds this logic
      and the three sites use it; the sequence fast path now reports what the
      plain path reports, so `- v # \x01\n` is an error
- [x] `tests/smoke/sequences.rs`, `tests/smoke/comments.rs`,
      `tests/error_reporting.rs`, and the tests in `lexer/plain.rs` and
      `lexer/comment.rs` pass with their assertions unchanged
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 3: Two helpers assemble node comments and the collection colon

`mapping_render.rs` and `sequence_render.rs` repeat the assembly of a node's
leading and trailing comments and of the collection colon, and Task 7 changes
all of those sites. Behavior is unchanged.

- [x] One helper assembles a node's comments and one the collection colon, and
      the render functions use them
- [x] The `comment-*.md` fixtures and the anchor tests in
      `rlsp-yaml/src/editing/formatter.rs` pass with their assertions unchanged
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 4: Emit every comment as an event

- [x] `parse_events` emits a `Comment` event, in source order, for the comment
      in each of: `a: "q" # c\n`, `a: 'q' # c\n`, `- "q" # c\n`, `k: # c\n`,
      `- # c\n  a: 1\n`, `? # c\n: v\n`, `? k\n: # c\n  v\n`, `a: &x # c\n  b: 1\n`,
      `a: !t # c\n  b: 1\n`, `a: | # c\n  t\n`, `a: > # c\n  t\n`,
      `a:\n  v\n  w # c\n`, `--- v # c\n`, `%YAML 1.2 # c\n---\na: 1\n`,
      `%TAG ! tag:x: # c\n---\na: 1\n`, `a: 1\n... # c\n`
- [x] `semantic_tokens` returns one comment token with the right line, start,
      and length for the comment of each input above
- [x] A test over every yaml-test-suite input asserts that every `#` at a line
      start or after whitespace, outside a scalar's span, starts a `Comment`
      event's span
- [x] `a: "q" # \x01\n` and the same character at each position above report an
      error; a comment over `MAX_COMMENT_LEN` reports an error after a value
      as it does on its own line
- [x] Values, spans, and errors of comment-free inputs are unchanged, apart from
      the one change named in Decisions: the yaml-test-suite event and loader
      conformance tests pass
- [x] `parse_events` yields nothing after the first `Err`, also when events were
      queued before it
- [x] `docs/conformance/` names the enforcement sites for the affected
      productions, and `rlsp-yaml-parser/docs/feature-log.md` tells a reader
      that comments are now validated at these positions
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 5: Store every comment event in the AST

- [x] `Document` has `prefix_comments` (before `---` or implicit content),
      `comments` (after `---`, before the root), `end_comments` (after the
      root, before `...`, `---`, or the end of input), and
      `after_end_comments` (after the last `...`)
- [x] Through `load()`, each comment of the Task 4 inputs and of
      `---\n# c\na: 1\n`, `# c\n---\na: 1\n`, `a: v\n# d\n---\nb: 1\n`,
      `v # c\n---\nw\n`, `a: 1\n# d\n...\n# e\n---\nb: 2\n`, `a: 1\n...\n# e\n`
      is in exactly one place of the AST, and a test pins which
- [x] A comment on a key's line or a block scalar's header line is that node's
      `trailing_comment`, and the doc comments on `Document` and `NodeMeta`
      describe all of these places
- [x] No test relies on a fabricated comment span, and the single-line filter in
      `loader/stream.rs` is gone
- [x] `rlsp-yaml-parser/docs/architecture.md` (comment attachment and the
      `Document` fields) and the "Comment Preservation" entry in
      `rlsp-yaml-parser/docs/feature-log.md` describe the four places, and
      neither says that prefix comments are discarded
- [x] In resolved mode a node produced by an alias carries none of the anchor's
      comments, the definition site keeps them, and the alias site's own comments
      stay
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 6: Print document-level comments and reduce the text matching

- [x] `format_yaml` keeps each comment of these inputs in the position shown:
      `---\n# c\na: 1\n` (unchanged), `# c\n---\na: 1\n` (unchanged),
      `a: v\n# d\n---\nb: 1\n` (unchanged), `v # c\n---\nw\n` to
      `v  # c\n---\nw\n`, `a: 1\n# d\n...\n# e\n---\nb: 2\n` (unchanged),
      `--- # c\na: 1\n` to `---\n# c\na: 1\n`,
      `%YAML 1.2 # c\n---\na: 1\n` to `# c\n---\na: 1\n`,
      `a: 1\n... # c\n` to `a: 1\n...\n# c\n`, `# only\n` (unchanged)
- [x] `name: "P"\non: x\n` with a comment above each key keeps both comments,
      and `a: |\n  t\n  # d\n` keeps one `# d`
- [x] `# h\n\na: 1\n` keeps its blank line
- [x] Restoring blank lines never changes a scalar's value:
      `a: 1\n\nb: "y"\nc: |\n  b: "y"\n  z\n` keeps the value of `c`
- [x] `comment_preservation.rs` only restores blank lines: no code in it
      creates or removes a comment line, and its name and doc comment say so
- [x] The comment about comments at `tests/formatter_conformance.rs`, the doc
      comment on `format_yaml`, the mentions of `attach_comments` in
      `scalar_render.rs`, and the "Comments" bullet in
      `rlsp-yaml/docs/configuration.md` match the new behavior
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 7: Print comments at every node position

A comment stays on its line when the output has that line and a slot on it;
otherwise it moves to its own line above the node's line.

- [x] `format_yaml` yields, and a fixture pins with its idempotence:
      `a: "q" # c\nb: 1\n` to `a: q  # c\nb: 1\n`,
      `- "1.32" # msrv\n` to `- "1.32"  # msrv\n`,
      `k: # c\nb: 1\n` to `k:  # c\nb: 1\n`, `k: # c\n  sub: 1\n` to
      `k:  # c\n  sub: 1\n`, `k: # c\n  v\n` to `k: v  # c\n`,
      `k: # c\n  v # d\n` to `# c\nk: v  # d\n`, `a: | # c\n  t\n` to
      `a: |  # c\n  t\n`, `- # c\n  a: 1\n` to `# c\n- a: 1\n`,
      `? k # c\n: v\n` to `k: v  # c\n`, `? k # c\n: v # d\n` to
      `# c\nk: v  # d\n`, `a: &x # c\n  b: 1\n` to `a: &x  # c\n  b: 1\n`,
      `: v # c\n` to `: v  # c\n`, `a:\n  v\n  w # c\n` to `a: v w  # c\n`,
      `a: v\n\n  w # c\n` to `a: "v\nw"  # c\n`
- [x] The block-scalar code action on `a: v # c\n` yields text with one `# c`,
      outside the literal value
- [x] With `rlsp-yaml/docs/feature-log.md`, a reader can tell where a comment
      moves and why
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 8: Turn a flow collection holding a comment into block style

- [x] `format_yaml` of `a: [\n  # c\n  1\n  ]\n` and of
      `a: {\n  # c\n  b: 1\n  }\n` yields block style with the comment, and the
      output re-parses to the same data
- [x] `a: [1] # c\n` stays a flow sequence with its comment
- [x] `block_to_flow` keeps its documented behavior of omitting comments
- [x] `rlsp-yaml/docs/feature-log.md` tells a reader why such a collection
      changes style
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 9: Keep comments when removing duplicate keys

- [x] With `format_remove_duplicate_keys`, the comments of a removed entry,
      nested ones included, become leading comments of the next surviving key,
      or of the last surviving key when none follows: `a: 1 # x\nb: 2\na: 3\n`
      yields `# x\nb: 2\na: 3\n`
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 10: Fold only own-line comments

Task 4 turns more trailing comments into events, and the folding of comment
blocks must not hide the content between them.

- [x] `a: 1 # x\nb: 2 # y\n` has no comment folding range, and a run of own-line
      comments keeps its range
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 11: Keep the data when formatting

The check in Task 12 compares the documents before and after formatting, and
these inputs change today.

- [ ] `a: |+\n  keep\n\n` and a block scalar with leading blank lines keep their
      values through `format_yaml`, and a second format changes nothing
- [ ] `- ! [a, b]\n` keeps its non-specific tag, and an anchor on an empty scalar
      keeps its anchor
- [ ] With `single_quote: true`, `a: 'here''s'\nb: "it's"\n` yields output that
      `load()` accepts with the same values
- [ ] `--- [alpha, bravo]\n` loads as a sequence and `--- *a\n` as an alias through
      `parse_events` and `load()`, the same events as libfyaml 0.9.6, and
      `--- [x] [y]\n` reports a syntax error
- [ ] `.ai/backlog.md` no longer lists `--- *a` and `--- [x] [y]`
- [ ] `docs/conformance/` names the enforcement site of each parser change, and
      `rlsp-yaml-parser/docs/feature-log.md` tells a reader what `--- [a]` loads as
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 12: Pin that no comment is lost

- [ ] A property test checks, for every yaml-test-suite input and every file in
      `rlsp-yaml/tests/corpus/` that loads, under the options default,
      `format_enforce_block_style`, `print_width: 20`,
      `format_indent_sequences: false`, `preserve_quotes`, `single_quote: true`,
      and `format_remove_duplicate_keys`: the multiset of
      `Comment` texts from `parse_events` is the same before and after
      `format_yaml`, `load()` of the output succeeds, the documents are
      equivalent, and a second format changes nothing
- [ ] The same check runs on variants of those inputs with ` # cN` appended to,
      and separately an own-line `# cN` inserted before, each line outside a
      block or multi-line quoted scalar, whenever the variant's documents are
      equivalent to the original's
- [ ] Every data difference the check finds is fixed in this task, with its spec
      production named in the handoff: the check passes on every input that loads
- [ ] Only inputs for which `load()` fails are skipped: `format_yaml` returns
      those unchanged. They are a pinned list in the test that may only shrink,
      and each has its deviation on `.ai/backlog.md`
- [ ] `rlsp-yaml/tests/corpus/comment-positions.yaml` contains every comment
      position of Tasks 4 to 7
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

## Non-Goals

- Deriving blank lines from the AST; blank lines can still be lost after a line
  whose spelling changes (separate plan after this one)
- Fixing the other parser deviations on `.ai/backlog.md`; the comments of a
  document that loads with such a deviation are still kept
- Keeping `--- # c` or `- # c` on the marker's line

## Decisions

- **Every comment travels through the parser and the AST:** the parser emits
  each comment as an event, the loader stores each one in the AST with new
  `Document` fields, and `attach_comments` keeps only blank-line restoration.
  Declined: keeping the text matching.
- **A long flow collection's closing bracket is indented under its key:** the
  formatter's output is what is wrong. Declined: relaxing the parser.
- **A flow collection that holds a comment becomes block style:** no comment
  logic in the flow renderers. Declined: flow rendering with comments inside.
- **A flow collection as a key stays flow (security review of Task 8):** the parser
  misreads or rejects block collections in a key position, and the formatter must
  print what the current parser loads (root `CLAUDE.md`, Crate Boundaries). Its
  comments print above the entry. A comment behind the closing bracket of a
  collection that turns block prints before the comments inside it.
- **Comments of entries removed by `format_remove_duplicate_keys` move to the
  next surviving key:** the Goal holds for every option. Declined: dropping
  them with the key.
- **Comments are validated at every position, with one length limit:** a
  comment that moves to its own line must still parse, so documents with
  non-printable or over-long comments at the newly checked positions now report
  an error, as [75] `c-nb-comment-text` requires, also after `---` and `...`;
  libfyaml accepts non-printable comment text and differs here. Declined: leaving
  those positions unchecked.
- **Positions after a multi-line double-quoted scalar become correct:**
  `collect_double_quoted_continuations` passes the trimmed line with the position
  of its untrimmed start, so a comment after the closing quote would carry a span
  short by the line's indentation. Tokens and error positions after that quote
  move to their real offset, also in comment-free inputs (yaml-test-suite 8UDB,
  9BXH, 9SA2); values stay the same. Declined: keeping the wrong spans.
- **An error ends the event stream (security review of Task 4):** a comment error
  at `...`, `---`, or a sequence item used to leave queued events or further
  parsing after the `Err`. `EventIter::next` now clears its queue and stops on the
  first `Err`, for every input; the first error itself stays the same, and
  `invalid_trailing_comment_leaves_no_comment_for_the_next_sequence_item` in
  `tests/error_reporting.rs` expects no comment after the error.
- **Alias expansion does not copy comments (security review of Task 5):** the
  expansion budget counts nodes, so copied comments multiply memory beyond it. A
  comment belongs where it stands; the formatter and the language server do not
  use resolved mode. Declined: counting comments against `max_expanded_nodes`, and
  leaving it open.
- **Blank-line restoration stays out of scalars (security review of Task 6):** it
  pairs lines by text, so a blank line could land inside a block scalar and change
  its value. Task 6 closes that without deriving blank lines from the AST, which
  stays the separate plan.
- **The loader attaches a comment to the node it stands with (review of Task 7):**
  a comment on a later item's dash or `?` line, or below an empty node's properties,
  went to the node before it, which reordered comments and made a second format
  change the output. `attach_line_comment` and the key and item loops now keep it
  with the following node; events stay unchanged.
- **`--- # c` and `- # c` move one line:** the data stays the same and no
  further field is needed; `--- # yaml-language-server: $schema=x` thereby
  becomes a valid modeline line. Declined: keeping them on the marker's line.
- **`# h\n\na: 1\n` keeps its blank line,** as blank lines do everywhere else.
- **No field on `Node` or `NodeMeta`:** `Node` is an exhaustive enum and
  `NodeMeta` can be built by callers, so a new field would break them;
  `Document` has a `pub(crate)` field, so new `pub` fields are additive.
- **Performance:** in the loader, a few branches per key and item and no allocation
  are the only addition on the path without comments.
  `format_yaml` also loads its restored output and compares it, because a blank
  line restored by text could otherwise change a value (security review of Task 6);
  correct data goes before speed here. No benchmark criterion.

## Task Notes

- Task 11: `x: &a 1\n*a : &y\n  b: 1\n` and `x: &a 1\n*a : !t\n  - 1\n` keep the
  space before the colon, so the output loads with the same data
- Task 6: `load()` returns no document for a comment-only stream, and that stays:
  `format_yaml` keeps these comments from `parse_events`, and input without a
  comment still formats to empty
- Task 7: `? k\n: # c\n  v\n` and `? k\n# c\n: v\n` give the same events, so both
  print like an own-line comment before a value: `k: v\n# c\n`, a fixture pins it
- Task 7: a block scalar as an explicit key prints its header comment:
  `? | # c\n  k\n: v\n` keeps `# c`
- Task 11: recheck where `--- [a]\n# c\n` stores `# c` once `--- [a]` loads as a
  sequence; `tests/loader_comments.rs` pins the place
- Task 11: these keep their values too: `a: |\n  x\n\n\n  y\n`, `a: |\n\n\n  x\n`,
  `c: >\n\n  y: 1\n  a: 1\n`, and `a: |+\n  x\n\n\nb: 1\n`
- Task 7: a block scalar as the document root prints its header comment:
  `--- | # c\n  x\n` keeps `# c`
- Task 11: `insert_final_newline: false` keeps the value of `a: |+\n  x\n\n`
- Task 11: `? [a]: x\n` with `format_enforce_block_style` keeps its data, and a
  second format changes nothing; the second-pass check in
  `tests/formatter_fixtures.rs` then covers every fixture, not only `comments`
- Task 11: `a: !!str 123\n` keeps its string type through `format_yaml`
- Task 11: `"": v\n` and `'': v\n` keep their empty string key, not a null key
- Task 12: `- !!str # c\n- # d\n- x\n` and `?\n:\n?\n:\n` are in the check's inputs
- Task 12: `? &x\n: v\n`, `&x\n---\ny\n`, `? !!str\n? &x\n`, and, under
  `format_indent_sequences: false`, `a:\n  - !!str\nb: 1\n` are in the check's
  inputs; the last needs `a:\n- !!str\nb: 1\n` to load as libfyaml does
- Task 11: `--- [ # c\n 1]\n` keeps its data and its comment once `--- [a]` loads as
  a sequence
- Task 11: `{c: }\n` prints a form that loads with the same data, and with
  `single_quote: true`, `a: 0x1F\n` stays an integer
- Task 11: with `format_enforce_block_style`, `x: 1\n{a: 1}: v\n` prints a form that
  loads with the same data, and `flow_collection_key_keeps_its_comments_above_the_entry`
  covers that option too
- Task 12: the fuzz probe at
  `/tmp/claude-1000/-workspace/20045551-9412-4db7-8253-65744d4ffa36/scratchpad/probe-base/src/bin/fuzz3.rs`
  finds comment-free data changes at about 2 to 3 percent of its inputs; run it to
  find inputs for the check
- Task 12: with `format_remove_duplicate_keys`, `a: &x 1\nb: *x\na: 2\n` keeps the
  value of `b`
