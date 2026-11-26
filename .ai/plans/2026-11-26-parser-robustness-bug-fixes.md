**Repository:** root
**Status:** NotStarted
**Created:** 2026-11-26

# Parser Robustness Bug Fixes

## Goal

`rlsp-yaml-parser` panics or overflows the stack on small inputs, takes time with the square of a
line's length, drops events it had already recognized before an error, and gives a single pair in a
flow sequence a span that misses its key. The language server and other users of the library read
untrusted documents through this parser: the panic and the time stall the server, the stack
overflow ends a program that loads with alias expansion, and the dropped events and the wrong span
give wrong data. The bugs that `.ai/backlog.md` lists for the parser's robustness are each fixed
where they arise.

## Context

- Paths are under `rlsp-yaml-parser/` unless a task names `rlsp-yaml/`; commands are those of the
  root `CLAUDE.md`. Every task ends with build, tests, `cargo clippy --all-targets`, and fmt
  passing.
- In the inputs of this plan, `\n`, `\r`, and `\t` stand for a line break, a carriage return, and
  a tab, `\uFEFF` for the byte order mark (BOM), and `\x01` and `\x07` for the bytes 0x01 and 0x07;
  every other backslash is a backslash of the text. Spans are byte offsets `start..end`.
- The YAML 1.2.2 specification (root `CLAUDE.md` References) decides parser behavior; each task
  names the sections or productions it rests on.
- `rlsp-yaml` loads every document in `LoadMode::Lossless` (`rlsp-yaml/src/parser.rs`), which keeps
  aliases as nodes; `LoadMode::Resolved`, which expands them, is used by the parser's own tests
  and by other users of the library.
- A **time test** follows the large-input tests of `tests/robustness.rs`: it runs the call under
  `Instant` and asserts that it ends within 30 seconds in a debug build. Each time test of this
  plan uses an input on which the code before the task takes at least five times the bound, unless
  its criterion says otherwise, and the review request reports the measured time before and
  after.
- `parse_events` and `load` are tested separately: a task that fixes one states which, and a time
  bound on `load` holds only after the tasks it names.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: A line that holds only a BOM is skipped between documents
- [x] Task 2: The events recognized before an error come before it
- [ ] Task 10: The events of a flow collection come before an error inside it
- [ ] Task 3: A double-quoted scalar of escapes is scanned in linear time
- [ ] Task 4: `LineIndex::line_column` answers in bounded time on a long line
- [ ] Task 5: A long flow line is tokenized with a running column
- [ ] Task 6: A plain scalar in a flow collection is scanned to its own end
- [ ] Task 7: The "Delete unused anchor" quick fix reads only the blanks before its anchor
- [ ] Task 8: A single pair in a flow sequence spans its key and its value
- [ ] Task 9: Alias expansion keeps to the nesting depth limit
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: A line that holds only a BOM is skipped between documents

`skip_blank_lines_between_docs` (`src/lexer.rs`) tests a line for blankness before it strips the
line's BOM, so a line that holds only a BOM, with or without blanks, is neither skipped nor
content: a debug build panics on the `debug_assert!` in `src/event_iter/directives.rs`, and a
release build emits an empty document. YAML 1.2.2 [202] `l-document-prefix` is an optional BOM
and comment lines, and [211] `l-yaml-stream` allows several prefixes in a row.

- [x] `parse_events` on `\uFEFF`, `\uFEFF  \n`, and `\uFEFF\t\n` yields the stream start and end and
      no document, and `load` returns no document
- [x] `\uFEFF\nk: v\n` yields one document whose start is at 4..4 with the key `k` at 4..5 and the
      value `v` at 7..8; `\uFEFF\r\nk: v\r\n` yields one document whose start is at 5..5
- [x] `a: 1\n...\n\uFEFF\nb: 2\n` yields two documents, the second starting at 13..13, and
      `a\n...\n\uFEFF` yields one document and ends the stream at 9..9
- [x] `\uFEFF\n\uFEFF\nk: v\n`, `\uFEFF\n# c\nk: v\n`, `\uFEFF\n---\nk: v\n`, and
      `\uFEFF\n%YAML 1.2\n---\nk\n` each load one document
- [x] The BOM tests of `src/lines.rs`, `tests/smoke/documents.rs`, `tests/robustness.rs`, and
      `tests/encoding.rs` pass with their assertions unchanged; two BOMs on one line stay the
      error they are
- [x] `%YAML 1.2\n\uFEFF---\nk\n` loads as it does before this task, and the test that pins it
      names [209] and the Decisions entry "Kept deviations"
- [x] `%YAML 1.2\n\uFEFF\n---\nk\n` stays the error it is (gate demand, see Decisions)

### Task 2: The events recognized before an error come before it

`next()` (`src/event_iter/base.rs`) clears its queue when a step yields an error, so events the
parser had already queued are lost: `*a : # \x07\n` reports its error without the alias. The
error is held until the queue is empty and then ends the stream. The inputs rest on YAML 1.2.2
§6.6 (comments), §6.8.1 (the `%YAML` directive), and §7.1 (alias nodes); the specification does
not say which events precede an error.

- [x] `parse_events` on `*a : # \x07\n` yields the block mapping start at 0..0, the alias `a` at
      0..2, and then the error at line 1, column 7
- [x] `[x] : # \x07\n` yields the block mapping start, the flow sequence start at 0..0, the scalar
      `x` at 1..2, the sequence end at 2..2, and then the error at line 1, column 8;
      `{x: 1} : # \x07\n` yields the block mapping start, the flow mapping with `x` and `1`, and
      then the error
- [x] `# c\n%YAML 1.2\n%YAML 1.2\n---\n` yields the comment ` c` and then the error
- [x] The error is the last item of every stream, with the kind and the position it has before
      this task; `- a # \x01\n- b # ok\n` ends with its error and holds no event of the second item
- [x] Every existing test passes with its assertions unchanged, except a test that asserts an
      event queued before an error is missing; the review request names each such test and its
      new assertion
- [x] `load` with the default options in `LoadMode::Lossless` returns the same error as before
      this task for each of these inputs
- [x] `docs/architecture.md` says, where it describes the event queue, that an error waits until
      the queue is empty, and `rlsp-yaml/docs/feature-log.md` records that comments before a parse
      error keep their highlighting
- [x] Through the server, the semantic tokens of `# c\n%YAML 1.2\n%YAML 1.2\n---\n` hold the
      comment token of `# c` (`rlsp-yaml/tests/lsp_lifecycle/semantic_tokens.rs`)

### Task 10: The events of a flow collection come before an error inside it

This task runs after Task 2. `handle_flow_collection` (`src/event_iter/flow.rs`) collects its
events in a local list and hands them to the queue only at its end, so every error return drops
them: `[a, b # \x01\n]\n` yields its error without the sequence. The events read before the error
reach the queue on every return, through one wrapper around the function's body. The inputs are
the flow collections of YAML 1.2.2 §7.4; the specification does not say which events precede an
error. Events are listed after the stream start and the document start.

- [ ] `parse_events` on `[a, b # \x01\n]\n` yields the flow sequence start at 0..0, `a` at 1..2,
      `b` at 4..5, and then the error at line 1, column 8
- [ ] `[a, [b, c # \x01\n` yields the sequence start at 0..0, `a` at 1..2, the sequence start at
      4..4, `b` at 5..6, `c` at 8..9, and then the error at line 1, column 12;
      `{a: 1, b: # \x01\n}\n` yields the flow mapping start at 0..0, `a` at 1..2, `1` at 4..5, `b`
      at 7..8, and then the error at line 1, column 12
- [ ] `[a, # c\n b, \x01]\n` yields the sequence start, `a` at 1..2, the comment ` c` at 4..7, `b`
      at 9..10, and then the error at line 2, column 4; `[a, b` yields the sequence start, `a`,
      `b` at 4..5, and then the error at line 1, column 5
- [ ] `k: [a, \x01]\n` yields the block mapping start at 0..0, `k` at 0..1, the sequence start at
      3..3, `a` at 4..5, and then the error at line 1, column 7; `[a, b]]\n` yields the sequence
      with its end at 5..5 and then the error at line 1, column 6
- [ ] `[a\n b # \x01\n]\n` yields the sequence start, the scalar `a b` at 1..5, and then the error
      at line 2, column 5; `[*a, \x01]\n` yields the sequence start, the alias `a` at 1..3, and
      then the error at line 1, column 5
- [ ] `[a: 1, b # \x01\n` yields the sequence start, a mapping start, `a` at 1..2, `1` at 4..5, a
      mapping end, `b` at 7..8, and then the error at line 1, column 11; the test asserts the kind
      of the mapping start and end, not their spans
- [ ] The error is the last item of every stream, with the kind and the position it has before
      this task, and every existing test passes with its assertions unchanged
- [ ] `load` with the default options in `LoadMode::Lossless` returns
      `NestingDepthLimitExceeded` at line 1, column 256 for `[` 300 times plus `a, \x01` and
      `AnchorCountLimitExceeded` for a flow sequence of 10 001 anchored items that ends in
      `, \x01]`, as the same shapes in block style do; the tests name the Decisions entry "Events
      before an error"
- [ ] Through the server, the semantic tokens of `[a, # c\n b, \x01]\n` hold the comment token of
      `# c` (`rlsp-yaml/tests/lsp_lifecycle/semantic_tokens.rs`)
- [ ] `rlsp-yaml/docs/feature-log.md` says that comments inside a flow collection before a parse
      error keep their highlighting and that a document that passes a limit before a syntax error
      gets the limit's diagnostic

### Task 3: A double-quoted scalar of escapes is scanned in linear time

`scan_double_quoted_line` (`src/lexer/quoted.rs`) computes the position of every escape by
counting from the line start, though only an error message uses it, so a line of n escapes costs
time with the square of n. The position is computed only when an escape is in error. The task
rests on YAML 1.2.2 §5.7 (escaped characters) and §7.3.1 (double-quoted style).

- [ ] A time test: `parse_events` and `load` on `k: "` plus `\x41` 400 000 times plus `"` each
      end within the bound
- [ ] The value loaded from that input is `A` 400 000 times
- [ ] The error of `k: "é\x41\q"` has the kind and the position it has before this task, and
      the existing escape-error tests pass with their assertions unchanged

### Task 4: `LineIndex::line_column` answers in bounded time on a long line

`LineIndex::line_column` (`src/pos.rs`) counts the code points from the line start on every call,
and the loader calls it once per node, so `load` and every per-match lookup of `rlsp-yaml` take
time with the square of a long line's length. The index keeps a count of the characters before
every fixed step of bytes, so a call counts a bounded stretch.

- [ ] For every text and every offset from 0 to one past the text's length, `line_column` returns
      what it returns before this task: a property test compares it with a copy of the old
      counting in the test module, over texts that mix ASCII, multi-byte characters, `\n`, `\r`,
      and `\r\n`, with offsets inside a character and between `\r` and `\n` included
- [ ] A time test: 400 000 calls at the end of a 1 MiB line of ASCII end within the bound; the
      same number of calls at the end of that line with a leading `é` also ends within the bound
      (this second case need not take five times the bound before the task)
- [ ] The tests of `src/pos.rs`, `tests/unicode_positions.rs`, and `tests/loader_spans.rs` pass
      with their assertions unchanged
- [ ] For a 1 MiB text, the memory the index adds is at most 64 KiB, and the review request
      reports the measured size

### Task 5: A long flow line is tokenized with a running column

The flow loop (`src/event_iter/flow.rs`) computes the column of every token by counting from the
line start, so a flow collection of n items on one line costs time with the square of n. A column
cursor that moves forward from the last token replaces the count. The task keeps the events of
YAML 1.2.2 §7.4 (flow collection styles).

- [ ] A time test: `parse_events` on `[` plus `"i0", "i1", …` up to `"i199999"` plus `]` on one
      line ends within the bound
- [ ] The events of that input carry the spans a count from the line start gives: a test compares
      the span of every 1 000th item with the offset computed from the text, for that line and
      for the same line with a leading `"é", `
- [ ] The yaml-test-suite runs for events and for the loader and the tests of `tests/` pass with
      their assertions unchanged

### Task 6: A plain scalar in a flow collection is scanned to its own end

`scan_plain_line_flow` (`src/lexer/plain.rs`) searches the whole rest of the line for `:` and `#`
for each plain scalar, though its byte loop stops at the first flow indicator, so n plain items on
one line cost time with the square of n. The scan handles `:` and `#` where it meets them. The
task keeps the scalar ends of YAML 1.2.2 §7.3.3 (plain style) in flow context.

- [ ] A time test: `parse_events` on `[item0, item1, …]` with 200 000 plain items on one line
      ends within the bound
- [ ] A time test: `load` of the same input ends within the bound (it rests on Tasks 4 and 5)
- [ ] Plain scalars in flow collections end where they end before this task: `[a:b, c]`,
      `[a: b, c]`, `[a #c\n]`, `[a#b, c]`, `{a: b:c}`, `[http://x.y, z]`, and `[a :b]` yield the
      same events, and the yaml-test-suite runs for events and for the loader pass

### Task 7: The "Delete unused anchor" quick fix reads only the blanks before its anchor

`deletion_range` (`rlsp-yaml/src/editing/code_actions/delete_anchor.rs`) searches back to the line
start for each anchor, so the quick fixes of n anchors on one line cost time with the square of n.
It reads back only over the blanks before the anchor, which is all its result depends on.

- [ ] The `one_line` shape joins the `code_actions_unused_anchors` benchmark group
      (`rlsp-yaml/benches/insight.rs`): `[&a0 x, &a1 x, …]` with 1 000, 2 000, 4 000, and 8 000
      anchors on one line; the steps from 2 000 to 4 000 and from 4 000 to 8 000 each raise the
      time by at most 2.5 times, and the review request reports the four times before and after
- [ ] The tests of `rlsp-yaml/src/editing/code_actions/delete_anchor.rs`,
      `rlsp-yaml/tests/unused_anchor_fix.rs`, and the unused-anchor cases of
      `rlsp-yaml/tests/lsp_lifecycle/code_actions.rs` pass with their assertions unchanged

### Task 8: A single pair in a flow sequence spans its key and its value

For a single pair in a flow sequence, such as `a: 1` in `[a: 1, b: 2]`, the flow loop
(`src/event_iter/flow.rs`) puts the mapping start at the `:` or at the following `,` and the
mapping end after the `,` and its blanks, so the loaded mapping spans `: 1, ` and misses its key.
The mapping starts where its key starts, as a block mapping does, or at the `?` of an explicit
pair, and ends at the `,` or `]` that ends the pair. The pair is the single-pair mapping of YAML
1.2.2 §7.4.1 (flow sequences); the specification defines no spans. In `rlsp-yaml`, the flow-style
validator (`src/validation/validators/flow_style.rs`), the "Convert flow mapping to block" action
(`src/editing/code_actions/flow_to_block.rs`), selection ranges, document symbols, and folding read
this span.

- [ ] The mapping start event has the start and the mapping end event has the end of these spans,
      and the loaded mapping has the span: `[a: 1, b: 2]` 1..5 and 7..11, `[a: 1 , b]` 1..6,
      `[&x a: 1]` 4..8, `[: v, b]` 1..4, `[[x]: 1, b]` 1..7, `["a": 1]` 1..7,
      `[a:\n  1,\n b]` 1..7, `[? a, b]` 1..4, and `[? a : 1, b]` 1..8
- [ ] The spans of flow sequences, flow mappings, and their scalars stay as they are, the empty
      scalars of `[&a , b]` and `{a: , b: 1}` included, and the yaml-test-suite runs for events
      and for the loader pass
- [ ] The docs of `Node::Mapping` and `Node::Sequence` (`src/node.rs`) say that a flow
      collection's span ends at its closing indicator, without it, and
      `docs/conformance/bnf-§7.md` says where the events of a single pair stand
- [ ] Through the server, `[a: 1, b: 2]` gets its two `flowMap` diagnostics at (0,1)-(0,6) and
      (0,7)-(0,12), each the pair's span with the end one column past it, as for flow collections
      (`rlsp-yaml/tests/lsp_lifecycle/`)
- [ ] The tests of `rlsp-yaml` pass with their assertions unchanged

### Task 9: Alias expansion keeps to the nesting depth limit

`expand_node` (`src/loader.rs`) never checks `max_nesting_depth`, and an anchored node is stored
expanded, so a chain `a1: &a1 [*a0]`, `a2: &a2 [*a1]`, … deepens the tree by one level per line:
1 000 such lines overflow a 2 MiB stack in a debug build. Expansion counts depth from the depth
of its alias and returns `NestingDepthLimitExceeded` at the alias when the tree would pass the
limit. The task rests on YAML 1.2.2 §7.1 (alias nodes); the limit is the library's own.

- [ ] On a 2 MiB thread in a debug build with the default options in `LoadMode::Resolved`, a chain
      whose expansion reaches 256 levels loads, and one that reaches 257 levels returns
      `NestingDepthLimitExceeded` at the position of its alias
- [ ] The chain of 1 000 lines returns that error on that thread
- [ ] With `max_nesting_depth(512)` on a 16 MiB thread in a debug build, an expansion of 512
      levels loads and one of 513 levels returns the error
- [ ] The nesting, alias-bomb, and deep-anchor tests of `tests/robustness.rs` and the resolved-mode
      tests of `tests/loader.rs` and `tests/loader_comments.rs` pass with their assertions
      unchanged
- [ ] Every place that says what `max_nesting_depth` bounds says that it bounds the tree alias
      expansion builds: the docs in `src/loader.rs`, `README.md`, `docs/architecture.md`, and the
      nesting-limit entry of `docs/feature-log.md`, which also records that an expansion deeper
      than the limit is an error
- [ ] `rlsp-yaml` loads the 1 000-line chain as before this task, without a diagnostic

## Decisions

- **Found bugs:** a pre-existing bug found while fixing a task of this plan becomes a new task of
  this plan when it lies in the parser's robustness (a panic, a stack overflow, time with the
  square of the input, lost events) and goes to `.ai/backlog.md` otherwise; a bug found while
  fixing such an added task (Task 10) goes to the backlog and gets no task here, so the plan
  ends. A fix whose correct behavior is open, or that changes visible behavior beyond its bug,
  goes to the backlog's Open decisions.
- **Internal design:** the shape of each fix follows the design advisor's report: the BOM is
  stripped at the top of every pass of the blank-line loop
  (stripping once per gap would reject two BOM lines in a row, which [211] allows); the error is
  held until the queue drains; `LineIndex` keeps character counts at fixed byte steps (a flag per
  ASCII line fails on one multi-byte character, and a table of every multi-byte character can
  pass the size of the text); the flow loop gets a column cursor; `expand_node` carries a depth
  and keeps its frame small, without a new option.
- **Events before an error:** every event queued before an error is emitted, comments included,
  so in the server a document with a parse error keeps the comment tokens before the error. A
  fix for alias and flow events alone would need a rule of its own for the same cause. Comment
  folds stay empty for such a document, since `folding_ranges` returns nothing without a loaded
  document. An event that now reaches the loader before the error can raise the loader's own
  error first, so the first error in document order is reported, as block style already does: in
  `LoadMode::Resolved`, `*a : # \x07\n` returns `UndefinedAlias`; with Task 10, a flow collection
  deeper than `max_nesting_depth` or with more anchors than the limit returns the limit's error
  where it returned the later syntax error, and the server shows that diagnostic.
- **Flow events through a wrapper:** Task 10 wraps the body of `handle_flow_collection`, so no
  return can drop its events; a flush before each of its error returns would edit the lines Task 5
  rewrites and miss a return added later. Events are emitted as built at the error: a pair's
  mapping start appears once its `:` was read, and the `?` of `[? a` yields no event. An anchor or
  tag held for a collection from an earlier line (`a: &y\n[x, \x01]`, `a: !!seq\n[x, \x01]`)
  reaches no event; this goes to the backlog's Open decisions.
- **Keys without events:** `k : # \x07` and `"q" : # \x07` still yield no key before their error,
  from another cause: the key's comment is checked before the key's event is queued. The
  specification does not say which events precede an error, so this goes to the backlog's Open
  decisions.
- **Alias depth:** in `LoadMode::Resolved`, a document whose expanded tree is deeper than
  `max_nesting_depth` returns `NestingDepthLimitExceeded` where it loaded on a large enough stack
  before. No fix of the stack overflow avoids this, and it reverses the documented statement that
  the limit does not bound the expanded tree. It reaches users of the library that load with
  alias expansion; `rlsp-yaml` does not.
- **Flow-to-block on a single pair:** Task 8 moves the range of the `flowMap` diagnostic of a
  single pair onto the pair. The edit "Convert flow mapping to block" offers for such a pair
  changes with the span and was already wrong; fixing it goes to the backlog's Open decisions.
- **Kept deviations:** three deviations from YAML 1.2.2 stay for the plan of the parser's
  conformance bugs: a BOM between a `%YAML` directive and `---` loads, though [209]
  `l-directive-document` allows none there (Task 1 keeps it and changes only a line that holds
  nothing but a BOM); a BOM between documents without `...` is rejected, though [211] allows it;
  an empty line after an escaped line break in a double-quoted scalar is dropped, though [112]
  reads it as a line feed.
- **Gate demand on Task 1:** a line that holds only a BOM stays an error after a directive, as
  [209] requires; skipping it there would add a deviation, not keep one.
- **Flow collection spans:** a flow collection's span ends at its closing indicator, without it;
  `flow_style.rs` and the folding tests of `rlsp-yaml` rely on that, so only the node docs change.
- **`deletion_range` in this plan:** the quick fix lies in `rlsp-yaml`, and its backlog entry is
  the one of `line_column`, so both halves are fixed here.
- **No bug:** `- a\0\n- b\n` yields no event after its error since commit 65c32e0b; a BOM in a
  plain scalar is an error by [27] `nb-char`, [34] `ns-char`, and [130] `ns-plain-char`, which
  `docs/conformance/design-decisions.md` records. Their backlog entry is removed.

## Non-Goals

- A length cap on block plain scalars and on the first line of a flow plain scalar — hardening,
  which stays in `.ai/backlog.md`
- The parser's conformance bugs, among them the three kept deviations (see Decisions)
- The events of a key whose own comment is in error, an anchor or tag held for a flow collection
  that an error leaves open, comment folds for a document with a parse error, and "Convert flow
  mapping to block" on a single pair — Open decisions in `.ai/backlog.md`
- A flow collection span that takes in the closing indicator
