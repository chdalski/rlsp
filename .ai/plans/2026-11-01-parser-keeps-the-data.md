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
- [ ] Task 4: A key reads its own `:`
- [ ] Task 5: A bare `-` looks past comment lines
- [ ] Task 6: Flow sequences keep their text
- [ ] Task 7: The loader rejects a second root node
- [ ] Task 8: Pin that no text is lost
- [ ] Task 9: The formatter drops its workarounds for these misreads
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

- [ ] These load with the events of libfyaml 0.9.6: `a:\n: c\n`, `b:\n: x\n`, `:\n: c\n`,
      `?\n:\n: c\n`, `? a\n:\n: c\n`, `x:\n  : v\n`, `x:\n  :\n    - 1\n`, `a: 1\n[x]:\n: v\n`,
      `a: &y\n&k : v\n`
- [ ] `*a : v`, `*a :`, `*a : # c`, `[x]: v`, `[x]:`, `[x]: # c`, `&k [x]: v`, `- *a : v`, and
      the yaml-test-suite cases E76Z and 26DV keep their events, and `[x]:2\n` keeps its
      behavior
- [ ] No new `MappingPhase` variant exists, and `Value` means only that the `:` is
      consumed
- [ ] `docs/conformance/` names the enforcement sites, and
      `rlsp-yaml-parser/docs/feature-log.md` tells a reader which inputs load differently
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 5: A bare `-` looks past comment lines

Productions: [184], [185], [197], [78], [79], [81].

- [ ] These load with the events of libfyaml 0.9.6: `-\n# c\n  b\n`, `-\n  # d\n`,
      `-\n\n  b\n`, `-\n  \n`
- [ ] `-\n# about x\n- x\n` keeps its comment with `x`
- [ ] Each comment of these inputs reaches the output of `format_yaml`, and a second
      format changes nothing
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 6: Flow sequences keep their text

- [ ] `[a # c\n  b]\n` reports a syntax error, as libfyaml does
- [ ] `[a\n\n  b]\n` loads the value `a\nb`, the same events as libfyaml 0.9.6
- [ ] `format_yaml` keeps the value of `[a\n\n  b]\n`, and a second format changes nothing
- [ ] `docs/conformance/` names the enforcement sites, and
      `rlsp-yaml-parser/docs/feature-log.md` tells a reader which inputs load differently
      or report an error
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 7: The loader rejects a second root node

The loader's "skip" arm drops a node event after the root.

- [ ] `load()` returns an error for an event stream with a node event after the root,
      and a test builds such a stream
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 8: Pin that no text is lost

- [ ] A sweep in `rlsp-yaml/tests/formatter_keeps_text.rs` inserts, for each
      yaml-test-suite, corpus, and `EXTRA_INPUTS` input and each line, a line with a
      unique token in the forms `zzN`, `%zzN`, `&zzN : v`, `!zzN : v`, and `&zzN`, at
      column 0, the line's own indent, and that indent plus 2. For every variant that
      loads, the token is in the loaded data and in the output of `format_yaml`: the
      number of failures is 0
- [ ] A failure whose input the criteria of Tasks 2 to 7 do not name stops the task
      and goes to the user, as Decisions says
- [ ] `rlsp-yaml-parser/tests/conformance/libfyaml_events.rs` holds a table of the
      inputs of Tasks 2, 4, 5, and 6 with their expected outcome: the events recorded
      from `fy-tool` 0.9.6, or "error" where it reports one. `parse_events` matches each.
      The inputs of Task 3 are outside the table, because they deviate from libfyaml
      and Task 3 pins their errors
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 9: The formatter drops its workarounds for these misreads

- [ ] `!t : 1` after an empty value prints without `?`, `Lead.value_indicator` and the
      `value_indicator` parameter of `holds_explicit_entry` are gone, and the clause
      `prints_no_text(previous) && has_properties(key)` applies only to collection keys
- [ ] `is_empty_key`, `key_needs_space_before_colon`, `may_exceed_implicit_key_limit`,
      `reads_as_another_node_on_its_own_line`, and the
      `lead.properties && is_collection(key)` clause keep their behavior
- [ ] No formatter comment, test comment, or fixture description says the parser reads
      `--- - a` or `--- ... x` as a scalar
- [ ] `rlsp-yaml/tests/formatter_keeps_comments.rs`, `formatter_keeps_data.rs`, and the
      new sweep pass, and the fixtures in `tests/fixtures/formatter/` expect the shorter
      forms
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
- **A loss the sweep of Task 8 finds outside this plan's inputs goes to the user:** the
  plan stops for a revision, because the sweep may reach further than the inputs known
  today. The target stays 0.

## Task Notes

- Task 3: `--- x # c\n%y\n` reports an error since Task 2 while libfyaml skips the line;
  `design-decisions.md` records it with the other `%` inputs
- Task 8: `a: !t &a\n  !zz\n` and `---\n!t &a\n!zz\n` report an error, as `a: !t\n  !zz\n`
  and libfyaml do, instead of dropping the second tag
