**Repository:** root
**Status:** InProgress
**Created:** 2026-11-01

# The Parser Reads the Data of Every Input

## Goal

The parser loses or misplaces data on some inputs, and `format_yaml` then deletes
the text from the file: `--- x\n  y\n` drops `y`, `a:\n: c\n` moves a value to the
wrong key, and `---\n%a\nb: 1\n` skips a line. Every such input must load as the
spec says or report an error, so that the formatter deletes no text apart from
directives.

## Context

- `rlsp-yaml-parser/CLAUDE.md`, "Spec and Reference Parser", applies to every parser
  change, with libfyaml 0.9.6 as the reference.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: One prepend in the marker handler
- [x] Task 2: The `--- x` path joins the other marker forms
- [x] Task 3: A `%` at a node start is an error
- [x] Task 4: A key reads its own `:`
- [x] Task 5: A bare `-` looks past comment lines
- [x] Task 6: Flow sequences keep their text
- [x] Task 7: The loader rejects a second root node
- [x] Task 8: Pin that no text is lost
- [x] Task 9: A flow plain scalar reads its continuation lines as text
- [x] Task 10: The formatter drops its workarounds for these misreads
- [ ] Task 11: One loop closes collections
- [ ] Task 12: An explicit key keeps its state in its own mapping
- [ ] Task 13: A collection key with an explicit entry prints in block form
- [ ] Measure every `*.yml` and `*.yaml` file under `/workspace`,
      `/usr/local/cargo/registry`, and `~/.local/share/pnpm`: the number of files that
      loaded at the plan's first commit, no longer load, and load in libfyaml 0.9.6 is 0,
      apart from files whose error is the `%` at a node start of Task 3, which are
      listed in the report to the user
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: One prepend in the marker handler

`handle_marker_inline` in `rlsp-yaml-parser/src/lexer.rs` builds the same line at
three sites, and its return value repeats what the caller computes. Task 2 removes
one of its arms. Behavior is unchanged.

- [x] `consume_marker_line` has one site that prepends the rest of a marker line
- [x] The marker tests in `tests/smoke/documents.rs` and `tests/smoke/scalars.rs`, the
      `consume_marker_line` tests in `lexer.rs`, and the yaml-test-suite conformance
      tests pass with their assertions unchanged
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 2: The `--- x` path joins the other marker forms

`--- x` skips the continuation lines and leaves the root node open, so the next line
becomes a second root scalar. "Root" is also encoded three ways across the scalar
readers. Productions: [207], [208], [134], [135], [197], [200].

- [x] `--- x\n  y\n`, `--- x\ny\n`, `  x\ny\n`, `--- x\n%y\n`, and `--- x\n  @y\n` load one
      scalar with the events of libfyaml 0.9.6
- [x] `--- v\n# d\nw\n`, `--- x\ny: 1\n`, `a:\nw\nx\n`, `--- - a\n`, `--- ? a\n`, `--- -\n`,
      and `--- ?\n` report a syntax error, as libfyaml does
- [x] `--- x # y: z\n` loads the scalar `x` with its comment
- [x] One path reads every marker form, and no inline scalar side channel remains in
      the lexer or the event iterator
- [x] No doc or comment describes that side channel: `docs/architecture.md`, including
      its `marker_inline_error` and `consume_marker_line` text, `bnf-§9.md`,
      `tests/smoke/quoted_scalars.rs`, and `tests/smoke/conformance.rs`
- [x] `format_yaml` keeps `y` in `--- x\n  y\n`, and a second format changes nothing
- [x] `docs/conformance/` names the enforcement sites, `bnf-§7.md` [126] states one
      verdict, and `rlsp-yaml-parser/docs/feature-log.md` tells a reader which inputs
      now report an error
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 3: A `%` at a node start is an error

Productions: [126], [22], [82], [209], [211].

- [x] `---\n%a\nb: 1\n`, `---\n%a\n`, and `a: 1\n%b\n` report a syntax error, and
      `format_yaml` returns them unchanged
- [x] `--- |\n%a\n` keeps `%a` in its value, and `%YAML 1.2\n---\na: 1\n` still loads
- [x] No line of the block dispatcher is consumed without an event or an error
- [x] `  \x07zz\n`, `a:\n  \x7fzz\n`, and `  \uFEFFzz\n` report a syntax error, and
      `design-decisions.md` records this deviation from libfyaml with [1]
- [x] `percent_prefixed_line_inside_explicit_doc_is_treated_as_content` in
      `tests/smoke/documents.rs` has a name that says so and asserts the error, and the
      doc comment on `is_blank_or_comment` matches [126]
- [x] `docs/conformance/design-decisions.md` records this deviation from libfyaml with
      its productions, and the entries of [22], [82], and [209] state the behavior
- [x] `rlsp-yaml-parser/docs/feature-log.md` tells a reader which inputs now report an
      error, and no longer says that a `%` at the start of a line still opens a directive
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 4: A key reads its own `:`

`MappingPhase::Value` means both "the `:` is consumed" and "an emitted alias or flow
key still waits for its `:`", so any later `:` line is read as the current key's
indicator. Productions: [191], [192], [194], [96], [106], [154].

- [x] These load with the events of libfyaml 0.9.6: `a:\n: c\n`, `b:\n: x\n`, `:\n: c\n`,
      `?\n:\n: c\n`, `? a\n:\n: c\n`, `x:\n  : v\n`, `x:\n  :\n    - 1\n`, `a: 1\n[x]:\n: v\n`,
      `a: &y\n&k : v\n`
- [x] `*a : v`, `*a :`, `*a : # c`, `[x]: v`, `[x]:`, `[x]: # c`, `&k [x]: v`, `- *a : v`, and
      the yaml-test-suite cases E76Z and 26DV keep their events, and `[x]:2\n` keeps its
      behavior
- [x] No new `MappingPhase` variant exists, and `Value` means only that the `:` is
      consumed
- [x] `docs/conformance/` names the enforcement sites, and
      `rlsp-yaml-parser/docs/feature-log.md` tells a reader which inputs load differently
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 5: A bare `-` looks past comment lines

Productions: [184], [185], [197], [78], [79], [81].

- [x] These load with the events of libfyaml 0.9.6: `-\n# c\n  b\n`, `-\n  # d\n`,
      `-\n\n  b\n`, `-\n  \n`
- [x] `-\n# about x\n- x\n` keeps its comment with `x`
- [x] Each comment of these inputs reaches the output of `format_yaml`, and a second
      format changes nothing
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 6: Flow sequences keep their text

- [x] `[a # c\n  b]\n` reports a syntax error, as libfyaml does
- [x] `[a\n\n  b]\n` loads the value `a\nb`, the same events as libfyaml 0.9.6
- [x] `format_yaml` keeps the value of `[a\n\n  b]\n`, and a second format changes nothing
- [x] `docs/conformance/` names the enforcement sites, and
      `rlsp-yaml-parser/docs/feature-log.md` tells a reader which inputs load differently
      or report an error
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 7: The loader rejects a second root node

The loader's "skip" arm drops a node event after the root.

- [x] `load()` returns an error for an event stream with a node event after the root,
      and a test builds such a stream
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 8: Pin that no text is lost

- [x] `? &x\n&zz : v\n: v\n`, `? !!str\n!zz : v\n? &x\n`, and `- ? &d\n  &zz : v\n` load with
      the events of libfyaml 0.9.6: the key that holds only properties gets an empty
      value, and the next line is a new entry
- [x] `&zz\n&a4 !!map\nk: v\n` and `---\n&zz\n&a4 !!map\nk: v\n` report a syntax error, as
      libfyaml does, and `&a4 !!map\n&a5 !!str key5: value4\n` keeps loading as the
      yaml-test-suite case 9KAX expects
- [x] A sweep in `rlsp-yaml/tests/formatter_keeps_text.rs` inserts, for each
      yaml-test-suite, corpus, and `EXTRA_INPUTS` input and each line, a line with a
      unique token in the forms `zzN`, `%zzN`, `&zzN : v`, `!zzN : v`, and `&zzN`, at
      column 0, the line's own indent, and that indent plus 2. For every variant that
      loads, the token is in the loaded data and in the output of `format_yaml`: the
      number of failures is 0
- [x] A failure whose cause the criteria of Tasks 2 to 9 do not name stops the task
      and goes to the user, as Decisions says; a failure on a flow continuation line
      is Task 9's, and the 0 holds once Task 9 is in
- [x] `rlsp-yaml-parser/tests/conformance/libfyaml_events.rs` holds a table of the
      inputs of Tasks 2, 4, 5, and 6 and of this task with their expected outcome: the events recorded
      from `fy-tool` 0.9.6, or "error" where it reports one. `parse_events` matches each.
      The inputs of Task 3 are outside the table, because they deviate from libfyaml
      and Task 3 pins their errors
- [x] `docs/conformance/` names the enforcement sites, and
      `rlsp-yaml-parser/docs/feature-log.md` tells a reader which inputs load differently
      or report an error
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 9: A flow plain scalar reads its continuation lines as text

A continuation line of a plain scalar in a flow collection is read as a new token when
it starts with a property, a quote, an alias, or an indicator, so the text lands in a
different place or the input is rejected. Productions: [130], [134], [135].

- [x] These load with the events of libfyaml 0.9.6, one plain scalar each: `[a\n &x b]\n`,
      `[a\n\n !t b]\n`, `[a\n "b"]\n`, `[a\n\n 'b']\n`, `[a\n *x]\n`, `[a\n\n - b]\n`,
      `[a\n\n |b]\n`, and `{a\n &x b: 1}\n`
- [x] `format_yaml` keeps their values, and a second format changes nothing
- [x] `libfyaml_events.rs` holds these inputs, and the sweep of Task 8 reports 0
- [x] `docs/conformance/` names the enforcement sites, and
      `rlsp-yaml-parser/docs/feature-log.md` tells a reader which inputs load differently
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 10: The formatter drops its workarounds for these misreads

- [x] `!t : 1` after an empty value prints without `?`, `Lead.value_indicator` and the
      `value_indicator` parameter of `holds_explicit_entry` are gone, and the clause
      `prints_no_text(previous) && has_properties(key)` applies only to collection keys
- [x] `is_empty_key`, `key_needs_space_before_colon`, `may_exceed_implicit_key_limit`,
      `reads_as_another_node_on_its_own_line`, and the
      `lead.properties && is_collection(key)` clause keep their behavior
- [x] No formatter comment, test comment, or fixture description says the parser reads
      `--- - a` or `--- ... x` as a scalar
- [x] `rlsp-yaml/tests/formatter_keeps_comments.rs`, `formatter_keeps_data.rs`, and the
      new sweep pass, and the fixtures in `tests/fixtures/formatter/` expect the shorter
      forms
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 11: One loop closes collections

`close_all_collections` in `rlsp-yaml-parser/src/event_iter/base.rs` repeats
`close_collections_at_or_above`, and Task 12 changes both. Behavior is unchanged.

- [ ] `close_all_collections` holds no loop of its own and closes through
      `close_collections_at_or_above`
- [ ] The yaml-test-suite conformance tests, `tests/conformance/libfyaml_events.rs`, and the
      sweep in `rlsp-yaml/tests/formatter_keeps_text.rs` pass with their assertions unchanged
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 12: An explicit key keeps its state in its own mapping

`complex_key_inline` holds one column, and a nested `?` overwrites the column of the outer
key, so the outer key's `:` starts a new entry and its value becomes a key. A stale
`explicit_key_pending` accepts a key without `?`, and the indentation guard rejects a `?`
that holds a mapping after the first entry. Productions: [189], [190], [191], [192], [195].

- [ ] These load with the events of libfyaml 0.9.6: `?\n  ? a\n: v\n`, `? ? a\n: v\n`,
      `?\n  ?\n: v\n`, `? ?\n: v\n`, `? &a\n  ? a\n: v\n`, `?\n  ? &a\n: v\n`,
      `?\n  ? [a]\n: v\n`, `?\n  ? *x\n: v\n`, `?\n  - ? a\n: v\n`, `? - ? a\n: v\n`,
      `?\n- ? a\n: v\n`, `?\n- a\n- ? b\n: v\n`, `- ?\n    ? a\n  : v\n`,
      `x:\n  ?\n    ? a\n  : v\n`, `?\n  a: 1\n  ? b\n: v\n`,
      `?\n  ? a\n  : b\n  ? c\n: v\n`, `?\n  x:\n    ? a\n: v\n`,
      `?\n  x:\n  - ? a\n: v\n`, `?\n  ? ? a\n: v\n`, `?\n  ?\n    ? a\n: v\n`, and
      `?\n  ? a\n:\n`
- [ ] These give the outer key its empty value, with the events of libfyaml 0.9.6:
      `?\n  ? a\n`, `?\n  ? a\n  : b\n`, `?\n  ? a\n...\n`, `?\n  ? a\n---\n`,
      `?\n  ? a\n? b\n`, `?\n  ? a\nb: 1\n`, `?\n  ? a\n*x : 1\n`, `?\n  ? a\n[x]: 1\n`,
      `?\n  ? a\n&k : v\n`, and `- ?\n    ? a\n- b\n`
- [ ] `?\n  ? a\n  : b\n: v\n`, `? ? a\n  : b\n: v\n`, `?\n  x: 1\n  ? a\n  : b\n: v\n`, and
      `?\n  - ? a\n    : b\n: v\n` load `v` as the outer key's value, and `?\n  ? a\n  : b\n:\n`
      loads an empty value, as Decisions says; `design-decisions.md` records this deviation
      from libfyaml with [189] and [191]
- [ ] `x:\n  ?\n- a\n` and `x:\n  ?\n[a]\n` report a syntax error, as libfyaml does
- [ ] These load with the events of libfyaml 0.9.6: `? a\n?\n  b: 1\n`, `? a\n? b: 1\n: v\n`,
      `a: 1\n?\n  b: 2\n: v\n`, `? a\n: b\n? c: d\n: e\n`, `? a\n?\n  ? b\n: v\n`, and
      `a: 1\n? b: 2\n: c\n`
- [ ] These keep their behavior: `?\nb\n` and `? a\n? b\n*y : 3\n` keep their errors, and
      `?\n: v\n`, `?\n- a\n: v\n`, `? !!str\n!zz : v\n? &x\n`, `? : v`, `?\n  a: 1\n: v\n`,
      `?\n  - a\n: v\n`, and `?\n  *x :\n: v\n` keep their events
- [ ] `complex_key_inline` and `explicit_key_pending` in `src/lib.rs` are gone, the field of
      the same name in `FlowFrame::Mapping` stays,
      `MappingPhase::ExplicitKey` means that a `?` is consumed and its node is not complete,
      and `Key` means only that no `?` is open
- [ ] `libfyaml_events.rs` holds the inputs that match libfyaml, and `format_yaml` keeps the
      data of every input of this task that loads, with a second format changing nothing
- [ ] For every input and variant of the sweep in `formatter_keeps_text.rs` that loads, every
      mapping in `parse_events` has an even number of child nodes: the number of failures is 0
- [ ] `docs/conformance/` names the enforcement sites, `rlsp-yaml-parser/docs/feature-log.md`
      no longer names a removed field, and it tells a reader which inputs load differently
      or report an error
- [ ] `rlsp-yaml-parser/docs/architecture.md` states the mapping phases, and the rule for a
      `-` entry, as the code has them
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 13: A collection key with an explicit entry prints in block form

`flow_comments.rs` turns such a key into a flow collection because the parser read the `:`
after it as a new entry. Task 12 removes that misread.

- [ ] `holds_explicit_entry` and `block_mapping_after_first` are gone, and no formatter
      comment, test name, or test comment says the parser cannot read these keys
- [ ] `? ? a\n  : b\n: v\n` and `x: 1\n? a: 1\n: v\n` print unchanged, and comments inside
      such a key stay on their nodes
- [ ] A key the input writes in flow style stays flow, and `keep_flow_key` keeps its behavior
- [ ] `rlsp-yaml/tests/formatter_keeps_comments.rs`, `formatter_keeps_data.rs`, and
      `formatter_keeps_text.rs` pass, and the tests that pinned the flow form expect the block
      form
- [ ] `rlsp-yaml/docs/configuration.md` and `rlsp-yaml/docs/feature-log.md` no longer say that
      these keys stay flow or that the parser cannot read them, the comment sentence in
      `configuration.md` states where comments inside a key print now, and the feature log
      tells a reader that these keys keep their block form
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

## Non-Goals

- `format_yaml` keeping `%YAML` and `%TAG` directives
- The other parser deviations on `.ai/backlog.md`
- The quadratic time on long flow lines, the formatter's run time, and the
  "Convert flow sequence to block style" code action (separate plans after this one)

## Decisions

- **`%` at column 0 inside a document is an error:** [126], [82], and [209] allow
  directives only before `---`, and a skipped line would be deleted by the formatter.
  libfyaml warns and skips it. Declined: following libfyaml.
- **A non-printable character at the start of a line is an error (user decision):**
  [1] `c-printable` and `ns-char` exclude it, so `  \x07zz\n` reports an error, as it
  already did at column 0 and inside a scalar. libfyaml loads it as a scalar. Declined:
  following libfyaml.
- **The separate `--- x` path goes:** one path for every marker form, and the behavior
  changes in Task 2 all match libfyaml. Declined: patching the path.
- **A key reads its own `:`, a bare `-` looks past comment lines, and the loader rejects
  a second root node.** Declined: a new `MappingPhase` variant, which touches about 15
  match sites and adds a state instead of removing a meaning.
- **`[a # c\n  b]` and `[a\n\n  b]` are in this plan:** the same class of wrong reading.
- **A loss the sweep of Task 8 finds with a cause outside Tasks 2 to 9 goes to the
  user:** the plan stops for a revision, because the sweep may reach further than the inputs known
  today. The target stays 0.
- **The sweep's first finds are in this plan (user decision):** `? &x\n&zz : v` and
  `&zz\n&a4 !!map`. Declined: lowering the target with known exceptions.
- **A second anchor or tag for one node reports an error, and properties above a key
  with its own properties go to the mapping (review of Task 8):** `&p1\n&q1 [y]\n`
  dropped an anchor, which the sweep does not see, because only its token counts.
- **A flow plain scalar that misplaces its text is in this plan (user decision):** the
  Goal covers misplaced data, though the text survives and the sweep cannot see it.
  Declined: a separate plan afterwards.
- **A nested `?` in an explicit key is in this plan (user decision):** the value becomes a
  key, which is the Goal's class. Declined: a separate plan after this one.
- **`MappingPhase::ExplicitKey` replaces `complex_key_inline` and `explicit_key_pending`
  (user decision):** the state moves to the mapping it belongs to, and the two adjacent
  defects go with it. The variant declined for Task 4 meant that a finished key waits for its
  `:` and added a state; this one removes two fields. Declined: a `Vec` of open `?` columns,
  a second stack that leaves the adjacent defects.
- **The spec decides `?\n  ? a\n  : b\n: v\n` (user decision):** [189] and [191] make a `:`
  right after the `?` node its value. libfyaml 0.9.6 reads an empty value and then an entry
  with an empty key, though it reads `?\n  ? a\n: v\n` as one entry; its issue tracker does
  not hold this. Declined: following libfyaml.
- **Task 11 keeps behavior:** the shared loop also writes `has_had_item` on a parent sequence,
  which is popped right after, and `root_node_emitted`, which the `---` and `...` paths reset
  before the next document and which is not read after the end of the stream.
- **Task 13 prints block form (user decision):** a shape whose block form does not reload to
  the same data is a formatter defect that Task 13 fixes; a parser misread with a cause outside
  Tasks 2 to 12 stops the task and goes to the user. Declined: leaving the workaround on the
  backlog.
- **The sweep's scope stays Tasks 2 to 12:** a failure of the even-node count whose cause Task
  12 does not name goes to the user like a loss the sweep finds.

## Task Notes

- Task 3: `--- x # c\n%y\n` reports an error since Task 2 while libfyaml skips the line;
  `design-decisions.md` records it with the other `%` inputs
- Task 8: `a: !t &a\n  !zz\n` and `---\n!t &a\n!zz\n` report an error, as `a: !t\n  !zz\n`
  and libfyaml do, instead of dropping the second tag
- Task 8: `[x]:2\n` stays outside the table, because Task 4 keeps its behavior where
  libfyaml errors; Task 4's tests pin it
- Task 12: `step.rs` near line 1052 must match `ExplicitKey` too, or `?\nb\n` stops reporting
  an error; the compiler does not force this site. `block/mapping.rs` near line 547 and
  `block/sequence.rs` near line 130 keep matching only `Key`
