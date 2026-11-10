**Repository:** root
**Status:** NotStarted
**Created:** 2026-11-10

# Server Feature Bug Fixes

## Goal

Several language-server features of `rlsp-yaml` crash the server, report wrong diagnostics, return
ranges that miss their position, or insert text other than the schema's names. The bugs that
`.ai/backlog.md` lists for these features, apart from the semantic-token positions (see Decisions),
and the pre-existing bugs found in the same code are each fixed where they arise.

## Context

- Paths are under `rlsp-yaml/`; commands are those of the root `CLAUDE.md`. Every task ends with
  build, tests, `cargo clippy --all-targets`, and fmt passing.
- New range code builds positions with `lsp_util::span_to_lsp` and `lsp_util::offset_to_lsp`; columns
  stay in code points (see Non-Goals).
- **Depth** is the number of collections that enclose a node; the root node has depth 0.
- The loader bounds depth: documents load with the `maxNestingDepth` setting, default
  `DEFAULT_MAX_NESTING_DEPTH` (256), at most `MAX_COLLECTION_DEPTH` (512,
  `rlsp-yaml-parser/src/limits.rs`). `tests/lsp_lifecycle/stack_depth.rs` runs every request and
  validator on documents 256 levels deep on a 2 MiB stack and, in its ignored release cases, 512
  levels deep on the release stack.
- A **chain** is a selection range with its parent ranges, innermost first. A range contains a
  position when the position lies between its start and its end, both included.
- The Language Server Protocol 3.17 specification (root `CLAUDE.md` References) defines the
  selection-range response and the snippet syntax that Tasks 5, 6, and 10 follow.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [ ] Task 1: Long names in validator messages are cut at a character boundary
- [ ] Task 2: References and rename share one anchor and alias walk
- [ ] Task 3: The unused-anchor validator and its quick fix read the whole document
- [ ] Task 4: The other validators read the whole document
- [ ] Task 5: No selection range misses its position
- [ ] Task 6: A selection-range response has one entry per position
- [ ] Task 7: A duplicate-key diagnostic covers the whole key
- [ ] Task 8: The block-scalar action never turns a comment into content
- [ ] Task 9: A BOM does not hide a modeline or a suppression comment on line 1
- [ ] Task 10: The "(all required)" snippet inserts names and defaults as written
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Long names in validator messages are cut at a character boundary

`validate_unused_anchors` (`src/validation/validators/anchors.rs`) cuts a long anchor name at byte
100 and panics when that byte lies inside a character; a panic in a handler ends the server.
`push_duplicate_diagnostic` (`src/validation/validators/duplicate_keys.rs`) tests the length in bytes
but cuts by characters, so a key of more than 100 bytes and at most 100 characters gets `...` with
nothing cut.

- [ ] A name of at most 100 characters appears in full and without `...` in the unused-anchor and the
      duplicate-key message, whatever its byte length: an anchor `a` followed by 60 `é`, and a
      duplicated key of 60 `é`
- [ ] A name of more than 100 characters appears as its first 100 characters followed by `...` in
      both messages, also when byte 100 lies inside a character: an anchor `a` followed by 150 `é`
- [ ] One function holds the truncation rule for both messages
- [ ] Opening `k: &a` followed by 60 `é` through the server publishes its `unusedAnchor` diagnostic,
      and the server answers a later request

### Task 2: References and rename share one anchor and alias walk

`src/navigation/references.rs` and `src/navigation/rename.rs` hold identical copies of the anchor and
alias walk and its helpers (`collect_anchor_alias_entries`, `collect_node`, `containing_document`,
`span_contains`, `span_to_range`), and `span_to_range` repeats `lsp_util::span_to_lsp`. Task 3 makes
the unused-anchor validator use the same walk. Behavior is unchanged.

- [ ] One copy of the walk and its helpers serves references and rename, and code in
      `src/validation/` can call the walk
- [ ] Navigation converts spans to ranges only through `lsp_util::span_to_lsp`
- [ ] The unit tests of `src/navigation/references.rs` and `src/navigation/rename.rs` and the tests in
      `tests/lsp_lifecycle/navigation.rs` and `tests/lsp_lifecycle/rename.rs` pass with their
      assertions unchanged

### Task 3: The unused-anchor validator and its quick fix read the whole document

`collect_anchors_and_aliases` (`src/validation/validators/anchors.rs`) stops at depth 100 and then
reports from a walk that did not finish: an anchor whose only alias lies deeper is "never used", and
an alias whose anchor lies deeper "has no matching anchor". The quick fix in
`src/editing/code_actions/delete_anchor.rs` refuses every anchor of a document deeper than that and
finds no anchor below depth 100. The validator takes its anchors and aliases from the walk that
Task 2 shares.

- [ ] An anchor at depth 0 whose only alias is at depth 99, at 100, at 101, and at 255 gets no
      `unusedAnchor` diagnostic
- [ ] An alias at depth 1 whose anchor is at depth 101 and at 255 gets no `unresolvedAlias`
      diagnostic
- [ ] An anchor without an alias at depth 101 and at depth 255 gets the `unusedAnchor` diagnostic and
      the "Delete unused anchor" action
- [ ] Through the server, the anchor-and-alias document of `tests/lsp_lifecycle/stack_depth.rs`
      publishes no `unusedAnchor` diagnostic at 256 and at 512 levels
- [ ] `no_fix_when_an_alias_may_sit_below_the_depth_the_validator_reads` and
      `an_alias_within_the_depth_the_validator_reads_uses_the_anchor` (`delete_anchor.rs`) and
      `the_fix_keeps_the_data_of_an_anchor_with_a_deeply_nested_alias` (`tests/unused_anchor_fix.rs`)
      assert the behavior above; no comment or doc says the validator or the quick fix stops at a
      depth

### Task 4: The other validators read the whole document

`flow_style.rs`, `custom_tags_validation.rs`, `yaml11_compat.rs`, `key_ordering.rs`, and
`duplicate_keys.rs` in `src/validation/validators/` each stop at a depth of 100 of their own and miss
what lies deeper: 101 nested block mappings with `k: {a: 1}` at the bottom get no `flowMap`
diagnostic.

- [ ] Each of the five validators reports a node at depth 101 and at depth 255 as it reports the same
      node at depth 1
- [ ] No validator in `src/validation/validators/` stops its walk at a depth of its own, and no
      comment or doc there says one does
- [ ] Through the server, 101 nested block mappings with `k: {a: 1}` at the bottom publish the
      `flowMap` diagnostic
- [ ] The cases of `tests/lsp_lifecycle/stack_depth.rs` pass, the ignored 512-level release cases
      included, run as their `#[ignore]` notes describe

### Task 5: No selection range misses its position

`src/analysis/selection.rs` compares columns without their lines, so a chain can hold ranges that do
not contain its position: for `k: "aaa\n  bbb" # c\n` at (1,9) the chain is the scalar (0,3)-(1,6),
its entry (0,0)-(1,6), and the document root (0,0)-(2,0). LSP 3.17 requires `positions[i]` to lie in
`result[i].range`.

- [ ] For every position of documents with a multi-line plain, double-quoted, and explicit-key
      scalar, a comment after a multi-line value, and comment lines inside and between entries, every
      range of the returned chain contains the position and every parent range contains its child
      range
- [ ] Each chain is the chain the code returned before this task, without the ranges that do not
      contain the position: `k: "aaa\n  bbb" # c\n` at (1,9) gets the document root (0,0)-(2,0) as
      its only range, and the existing selection tests pass with their assertions unchanged, apart
      from those that pin a range that does not contain its position
- [ ] A position that got no range before this task gets none: `k: aaa\n  bbb\n` at (1,1), and
      `key: value\n# a comment\nother: data\n` at (1,5)
- [ ] Through the server, the selection-range request for `k: "aaa\n  bbb" # c\n` at (1,9)
      (`tests/lsp_lifecycle/selection_ranges.rs`) returns the document root (0,0)-(2,0) as its only
      range

### Task 6: A selection-range response has one entry per position

`selection_ranges` (`src/analysis/selection.rs`) drops a position without a range, so later results
move onto earlier positions, while LSP 3.17 pairs `result[i]` with `positions[i]` and allows the empty
range at `positions[i]` for a position without one: `k: v\n# c\n` with the positions (1,1) and (0,0)
returns one range today, which the client takes for (1,1).

- [ ] The response holds one entry per requested position, in request order
- [ ] A position without a range gets the empty range at the position, without a parent: (1,1) of
      `k: v\n# c\n`, (1,5) of `key: value\n# a comment\nother: data\n`, (1,0) of
      `key: value\n...\nother: val\n`, and (0,0) when there is no document
- [ ] `should_return_empty_for_empty_document`, `should_return_empty_for_cursor_on_dot_dot_dot_line`,
      and the `comment_line_returns_no_selection_range` case of `selection_ranges_regression` assert
      the empty range at their position; the doc comment of `selection_ranges` describes the response
- [ ] Through the server, the request with (1,1) and (0,0) on `k: v\n# c\n` returns two entries, the
      empty range at (1,1) and then the chain for (0,0), and a request on an empty document returns
      the empty range at each position

### Task 7: A duplicate-key diagnostic covers the whole key

`push_duplicate_diagnostic` (`src/validation/validators/duplicate_keys.rs`) builds the range from the
key's start line and end column, so a multi-line key gets a range on one line.

- [ ] For `"a b": 1\n? "a\n  b"\n: 2\n` the duplicate-key diagnostic covers (1,2)-(2,4)
- [ ] A single-line duplicate key keeps its range
- [ ] Through the server, the diagnostic published for that document covers (1,2)-(2,4)

### Task 8: The block-scalar action never turns a comment into content

When a scalar has a trailing comment and `trailing_comment_line_end`
(`src/editing/code_actions/block_scalar.rs`) does not find it at the end of the scalar's line, the
action replaces only the scalar, and the comment stays after the block text, where it is content.
Only a document and a text that disagree reach this, so no server test can.

- [ ] No block-scalar action is offered for a scalar whose trailing comment the rest of its line does
      not hold: the document loaded from `k: ` plus a value of 40 characters plus ` # c\n`, with a text
      whose comment reads `# d`
- [ ] The existing tests of the block-scalar action pass with their assertions unchanged

### Task 9: A BOM does not hide a modeline or a suppression comment on line 1

`modeline_values` (`src/schema/association.rs`) and `build_suppression_map`
(`src/validation/suppression.rs`) read line 1 with its leading U+FEFF, which `trim` keeps, so no
`# yaml-language-server:` modeline and no suppression comment on that line matches.

- [ ] After a leading BOM, the `$schema`, `$tags`, and `$yamlVersion` modelines and the
      `# rlsp-yaml-disable-file` and `# rlsp-yaml-disable-next-line` comments on line 1 take effect
      as they do without the BOM
- [ ] `lsp_util::source_lines` still yields the BOM as part of line 1
- [ ] Through the server, a BOM document with a `$schema` modeline gets that schema's diagnostic, and
      a BOM document with `$tags=!x` gets no `unknownTag` diagnostic for `!x`

### Task 10: The "(all required)" snippet inserts names and defaults as written

`src/completion/schema_completions.rs` puts property names and placeholder defaults into the
"(all required)" snippet without the snippet escapes for `\`, `$`, and `}`: a property named
`$CLIPBOARD` makes the client insert the clipboard, and the placeholder of an object default `{}`
ends at its first `}`.

- [ ] The snippet inserts each required name exactly as the schema writes it, for names holding `$`,
      `}`, and `\`, and for `$CLIPBOARD`
- [ ] The placeholder of an object property covers the whole default `{}`
- [ ] Through the server, a completion request against a schema with such names
      (`tests/lsp_lifecycle/completion.rs`, schema served by the fake proxy) returns the escaped
      snippet

## Decisions

- **Run rules:** a pre-existing bug found while fixing Tasks 4 or 6 goes to `.ai/backlog.md`, and one
  found while fixing another task becomes a new task of this plan — Tasks 4 and 6 are already bugs
  found in this code, and attaching only one generation of found bugs lets the run end. A fix whose
  correct behavior is open, or that changes visible behavior beyond its bug, goes to the backlog's
  Open decisions, since the user decides those.
- **Semantic-token positions:** the backlog entry that semantic-token lengths count code points, not
  UTF-16 units, is not fixed here. Every position the server sends and reads counts code points, the
  tokens' start columns included, so a fix in the semantic tokens alone would still misplace them; a
  plan of its own converts positions at the server boundary.
- **Whole-tree walks:** the anchor validator, its quick fix, and the other validators walk the whole
  document instead of stopping at a depth of their own — the loader already bounds depth, and
  `stack_depth.rs` measures every validator at that bound; a walk that stops reports absences it has
  not checked.
- **Truncation:** the shared rule keeps the `...` suffix of both messages; the `…` helpers in
  `server.rs` and `completion/formatting.rs` cap a total length with another suffix and stay apart.
- **Selection:** Task 5 only leaves out ranges that do not contain the position and adds none.
  Selecting a node for a position that gets no range today, such as the indentation of a continuation
  line or a comment line, changes visible behavior beyond the bug.
- **BOM:** the modeline and suppression readers skip it; `source_lines` keeps it, because its other
  callers count columns from the line text, and the BOM takes one column there.
- **Block-scalar guard:** when the trailing comment cannot be found on the line, the action is not
  offered — an edit could otherwise turn the comment into content.

## Non-Goals

- Positions in UTF-16 units for the whole server, the semantic-token columns and lengths included
  (separate plan after this one)
- Selecting a node for a position that gets no range today — the indentation of a multi-line
  scalar's continuation line, a comment line inside or between entries — an open decision in
  `.ai/backlog.md`
- Quoting a schema property name that YAML would read as another key, in the completion items and
  the snippet — an open decision in `.ai/backlog.md`
