**Repository:** root
**Status:** Completed (2026-11-12)
**Created:** 2026-11-12

## Goal

"Convert block to flow style" deletes text the user wrote: its edit runs from the key's colon to
the start of the next sibling, so a comment or blank line after the block's last entry is gone
after the conversion. The conversion must replace only the collection and keep every character
around it, and it must not run where its flow output cannot hold what the block contains.

## Context

- **Specifications:**
  - LSP 3.17
    (<https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#codeAction>):
    `CodeAction.disabled` (`{ reason: string }`, since 3.16) "marks that the code action cannot
    currently be applied"; disabled actions are not shown in automatic lightbulb menus, and the
    reason "is displayed in the code actions UI". A client announces support with
    `textDocument.codeAction.disabledSupport`.
  - YAML 1.2.2 (`.ai/references/yaml-1.2.2-spec.md`): §6.6 Comments and §7.4 Flow Collection
    Styles allow comments inside a flow collection that spans several lines; the entries of a
    flow collection are flow nodes, and the block scalar styles of §8.1 (`|`, `>`) exist only in
    block context.
- **The action:** `block_to_flow` in `rlsp-yaml/src/editing/code_actions/block_to_flow.rs`, called
  from `code_actions` in `rlsp-yaml/src/editing/code_actions.rs`; the `textDocument/codeAction`
  handler and `initialize` are in `rlsp-yaml/src/server.rs`. The server keeps no client
  capability today. The action prints the flow collection through `format_subtree`, which writes
  no comments into it.
- **Spans** (`rlsp_yaml_parser`): a block collection's span ends at the start of the line that
  holds its next sibling, or at the end of input; between the end of its content and that point
  lie only blanks, line breaks, and comments. A comment span from `parse_events` takes in the
  comment's trailing blanks. An empty value or item is a zero-width scalar at the start of the
  next sibling's line.
- **Trivia:** blanks (space and tab only, never other Unicode whitespace), line breaks (LF, CR),
  and the comment spans `parse_events` reports.
- **The converted range:** the text the edit replaces, from right after the key's `:` to the end
  of the collection's content (its last character before the trailing trivia). It holds the
  trivia between the `:` and the collection's first token — its anchor or tag, if it has one —
  and everything inside the collection.
- **Test harnesses:** the code-action fixture harness (`rlsp-yaml/tests/code_action_fixtures.rs`,
  format in `rlsp-yaml/tests/fixtures/CLAUDE.md`) turns CRLF into LF and strips a final line
  break, so line-break cases are unit tests. `rlsp-yaml/tests/parser_boundary_audit.rs` flags a
  function whose first parameter is `text: &str` unless it has a `CarveOut` entry.

## Steps

- [x] Task 1: The block-to-flow test helper applies the edit like a client
- [x] Task 2: The conversion replaces only the converted range
- [x] Task 3: A comment in the converted range disables the action
- [x] Task 4: A block scalar inside the collection disables the action
- [x] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: The block-to-flow test helper applies the edit like a client

`apply_block_to_flow_edit` (test helpers of `code_actions.rs`) rebuilds the document line by
line and appends a line break after `new_text`, so it cannot check an edit that ends inside a
line, and it loses CR line breaks. Behavior is unchanged; only test code changes.

- [x] `apply_block_to_flow_edit` returns the document as a client that applies the action's edit
      would, also for an edit that ends inside a line and for CR and CRLF line breaks
- [x] The tests in `block_to_flow.rs` that use it pass with their assertions unchanged
- [x] `cargo clippy --workspace --all-targets` and `cargo test --workspace` pass

### Task 2: The conversion replaces only the converted range

The edit covers the converted range (Context), with its start and end found as the Decisions
say, and `new_text` ends with the flow collection. Comments in the converted range are still left
out in this task; Task 3 changes that.

- [x] `a:\n  b: 1\n  c: 2\n\n# keep me\nd: 1\n` becomes `a: { b: 1, c: 2 }\n\n# keep me\nd: 1\n`,
      and `a:\n  b: 1\n  c: 2 # note\nd: 1\n` becomes `a: { b: 1, c: 2 } # note\nd: 1\n`
- [x] The text after the collection stays as it was for an own-line comment indented at entry
      level, a comment with trailing blanks and a tab, a comment at the end of the file with no
      next sibling, trailing blank lines at the end of the file, a missing final line break, a
      comment before `---`, a next sibling written as `- y`, `? c`, or `: v`, a cursor on an inner
      collection whose trailing comment sits at the outer level, and output that wraps over
      several lines
- [x] For an empty last value or item (`c:`, a bare `-`), a last value with only properties
      (`c: &x`), and a multi-line flow collection as last value with its `]` on a line of its own,
      the edit ends right after the `:`, the `-`, the property, or the `]`, and the comment after
      it stays
- [x] The blanks and line breaks between the `:` and the collection become the one space before
      the flow collection
- [x] `a :\n  b: 1\nd: 1\n` becomes `a : { b: 1 }\nd: 1\n` and `? a\n:\n  b: 1\n` becomes
      `? a\n: { b: 1 }\n`; both load, and an anchor or tag before the collection is printed once
- [x] In a CRLF document and in a bare-CR document, the line breaks after the converted collection
      are the document's own, and no LF is inserted after the flow collection; unit tests assert
      the exact resulting text
- [x] The cases above are code-action fixtures, except the line-break cases, which are unit tests
- [x] A lifecycle test asserts the exact edit the server answers for a collection followed by a
      comment, and the lifecycle tests that expect a `new_text` ending in a line break
      (`tests/lsp_lifecycle/format_options.rs`) expect the new text
- [x] No comment in `block_to_flow.rs` or `tests/code_action_property_preservation.rs` describes
      the old edit range (a start one column after the key, an end at the next sibling)
- [x] A user can find in `rlsp-yaml/docs/feature-log.md` that the conversion keeps the text around
      the collection
- [x] `cargo clippy --workspace --all-targets` and `cargo test --workspace` pass

### Task 3: A comment in the converted range disables the action

When a comment lies in the converted range, the action is offered disabled (Decisions) instead of
deleting the comment. `should_print_no_comment_inside_the_flow_collection` and the five cases of
`should_print_no_comment_of_a_flow_collection_inside_the_result` (`block_to_flow.rs`) pin today's
deletion and change with this task.

- [x] For a comment on the key's line (`a: # note`), an own-line comment before the first entry of
      a mapping (`a:\n  # lead\n  b: 1\n`) and of a sequence (`a:\n  # lead\n  - x\n`), a comment
      after the collection's anchor or tag (`a: &x # note`), a comment between entries, and a
      comment behind an inner entry, the action is offered with the title "Convert block to flow
      style", the kind `refactor.rewrite`, no edit, and the reason "Flow style would lose the
      comments in this collection"
- [x] The two tests named above assert the disabled action for their inputs
- [x] For a client that does not announce `disabledSupport`, the `textDocument/codeAction` answer
      holds no disabled action, and an answer left without any action is `null`
- [x] A lifecycle test requests code actions for a collection with a comment inside, once with
      `disabledSupport: true`, which gets the disabled action with its reason and no edit, and once
      with empty capabilities, which gets no block-to-flow action
- [x] A code-action fixture can state that an action is offered disabled with a given reason;
      `applies-action` fails on a disabled action; `omits-action` means no action at all;
      `rlsp-yaml/tests/fixtures/CLAUDE.md` documents this, and the disabled cases are fixtures
- [x] A user can find in `feature-log.md` when and why the action appears disabled; the entry that
      says the conversion leaves out the comments inside the collection no longer says so
- [x] `cargo clippy --workspace --all-targets` and `cargo test --workspace` pass

### Task 4: A block scalar inside the collection disables the action

A block scalar inside the collection keeps its `|` or `>` in the flow output, which then does not
load (`a:\n  c: |\n    line1\n  b: 1\nd: 1\n`); YAML 1.2.2 §8.1 allows block scalars only in
block context (Context). The action is offered disabled instead (Decisions).

- [x] When a block scalar (`|` or `>`) is a value, an item, or a key anywhere in the collection,
      the action is offered disabled with the reason "Flow style cannot hold the block scalar (| or
      >) in this collection" and no edit
- [x] When the collection holds both a block scalar and a comment, the reason is the block
      scalar's
- [x] Fixtures cover `|` and `>` as a value, nested deeper, and as a key, and a collection with
      both a block scalar and a comment
- [x] A user can find in `feature-log.md` that a block scalar in the collection also disables the
      action
- [x] `cargo clippy --workspace --all-targets` and `cargo test --workspace` pass

## Decisions

- **End of the edit:** the edit ends at the end of the collection's content, found by stepping
  back from the collection's span end over trivia; a comment counts when the offset lies inside
  its span, since the span takes in the comment's trailing blanks. `new_text` ends with the flow
  collection, so the line break after it is the document's own, and a comment behind the last
  entry stays behind the closing bracket.
- **Start of the edit:** the start is found with the same rule, stepping back from the
  collection's first token no further than the end of the key, so it always lies right after the
  `:`. This replaces the fixed one-column offset and fixes `a :\n  b: 1\n`, which loses its colon
  today, and `? a\n:\n  b: 1\n`, which becomes invalid YAML today.
- **One trivia helper, private:** the fold rule of `analysis/folding.rs` gives the line a fold
  ends on and stops before a closing bracket on its own line; the edit needs the byte offset where
  the text ends, bracket included, so the two are not shared. The trivia helper stays private to
  `block_to_flow.rs` until a second user needs it, and it gets a `CarveOut` entry in
  `parser_boundary_audit.rs`: it does byte-range arithmetic on parser spans, which Crate
  Boundaries in `CLAUDE.md` allows.
- **Comments in the converted range:** YAML allows comments in a flow collection that spans
  several lines (§6.6, §7.4), but this action prints the flow collection without them. When a
  comment lies in the converted range, the action is offered disabled with a reason instead of
  deleting the comment. Printing a multi-line flow collection that keeps the comments is not part
  of this plan; moving the comments above the key parts them from their entries; dropping them
  silently loses them.
- **Block scalars inside the collection:** when a block scalar lies in the collection, as a
  value, an item, or a key, the action is offered disabled with a reason as well; a flow
  collection cannot hold it (§8.1).
- **Which reason wins:** with both a block scalar and a comment inside, the reason names the block
  scalar.
- **Shape of a disabled action:** the title and the kind (`refactor.rewrite`) stay, and it carries
  no edit, so code that applies every action's edit skips it.
- **Clients without `disabledSupport`:** `block_to_flow` always returns the disabled action; the
  server records the client's `disabledSupport` at `initialize` and drops disabled actions from
  the answer for a client without it, so the feature function stays free of the flag.
- **VS Code:** `vscode-languageclient` 11 announces `disabledSupport` and shows the reason (its
  `lib/common/codeAction.js`), so the extension needs no change. Whether the Zed and Claude Code
  clients announce it was not checked; without it they get no action in these cases.
- **Older feature-log history:** the dated comment in an older `feature-log.md` entry that comments
  inside a flipped subtree are not preserved stays as history; the current entry carries the new
  behavior.

## Non-Goals

- The same content end for selection ranges, document symbols, and schema diagnostics on block
  collections (separate plan after this one)
- `format_subtree` joining wrapped lines with LF in CRLF and CR documents (`.ai/backlog.md`)
- Changes to the VS Code extension

## Task Notes

- **Task 4:** the cases `hash_in_a_literal_block_scalar` and `hash_in_a_folded_block_scalar` of
  `should_end_the_edit_after_the_content_of_the_collection` (`block_to_flow.rs`) expect an edit
  and move to the block-scalar disabled cases; the fixtures
  `block-to-flow-disabled-by-comment-between-a-block-scalar-and-the-next-entry` and
  `block-to-flow-disabled-by-comment-in-a-block-scalar-header` hold a block scalar and a comment,
  so their `disabled-reason` becomes the block scalar's
