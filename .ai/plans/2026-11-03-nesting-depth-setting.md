**Repository:** root
**Status:** InProgress
**Created:** 2026-11-03

# A Configurable Nesting Depth of 256 That Holds on Every Build

## Goal

The loader's nesting limit is meant to protect the stack, but a debug build aborts on a 2 MiB
thread at about 207 levels, below the server's 256 and the library default of 512, and
`format_yaml` aborts at 256. The loader gets smaller stack frames, so a depth of 256 holds in
debug and release, for `load()` and `format_yaml`; the library default becomes 256, and the
server gets a setting `maxNestingDepth` with a default of 256 and a range of 1 to 512. The server
drives its handlers on a thread of 8 MiB on every platform, and the schema validation of a document
embedded in a string counts toward the depth of that string, so neither the setting's upper end nor
a crafted schema exhausts the server's stack.

## Context

- `LoadState::parse_node` in `rlsp-yaml-parser/src/loader.rs` recurses once per nesting level.
  `LoaderOptions::max_nesting_depth` is the loader's limit (default 512);
  `MAX_COLLECTION_DEPTH` in `rlsp-yaml-parser/src/limits.rs` (512) is the event parser's, and
  a document of 512 levels still loads under it.
- The server loads documents with a constant limit of 256 in `rlsp-yaml/src/parser.rs`, and
  `format_yaml` loads with the library default.
- The local branch `spike/small-frames` (44e7a17f; its first commit a8b57afa only raised
  `MAX_COLLECTION_DEPTH` for measuring) is the shape for the loader: a small dispatcher with
  `#[inline(never)]` helpers, each node built in place in a slot of its parent, `LoadError`
  boxed inside the recursion, and an entry's leading comments behind one `Box`.
- A stack check runs code on a thread with a 2 MiB stack, the size of a tokio worker and a test
  thread. The loader tests have a helper for that (`on_stack` in `loader.rs`).

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: The loader uses small stack frames
- [x] Task 2: `format_yaml` holds 256 levels in debug
- [x] Task 3: One builder for the server's format options
- [x] Task 4: Pin the color and code action responses
- [x] Task 5: Code actions and colors read the stored AST
- [x] Task 6: The formatter takes the nesting limit as an option
- [x] Task 7: The default nesting limit is 256
- [x] Task 8: The server setting `maxNestingDepth`
- [x] Task 9: Embedded documents count toward the validation depth
- [ ] Task 10: The server runs on a thread with an 8 MiB stack
- [ ] Task 11: The server holds both ends of the setting
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: The loader uses small stack frames

`parse_node` keeps every temporary of every arm in one frame. Behavior is unchanged.

- [x] `load()` loads 256 and 512 levels of block mappings, block sequences, flow mappings, and
      flow sequences on a 2 MiB thread in a debug build, with a limit of 512, and tests pin it
- [x] The `Debug` output of `load()` at the commit before this task and after it is the same for every
      yaml-test-suite input, every file in `rlsp-yaml/tests/corpus`, and every `*.yml` and
      `*.yaml` file in the repository, in lossless and resolved mode: 0 differences
- [x] The loader's tests and the yaml-test-suite conformance tests pass with their assertions
      unchanged
- [x] Whether a node still has children to read is a type of its own, not a `bool`
- [x] The comment on the dispatcher states that a slot holds an empty scalar until a node
      replaces it
- [x] The state where a scalar or alias has children is handled in one place, with a comment
- [x] Each `#[inline(never)]` says why
- [x] The loader description in `rlsp-yaml-parser/docs/architecture.md`, including the arms of
      `parse_node`, the `Comment` arm, and the paragraph on its recursion, matches the dispatcher
      and its helpers
- [x] No test of `rlsp-yaml/src/parser.rs` needs a thread larger than 2 MiB, and no comment in
      `loader.rs`, `rlsp-yaml/src/parser.rs`, `rlsp-yaml/src/editing/formatter.rs`, or
      `rlsp-yaml/tests/formatter_keeps_data.rs` states the old stack depth of the loader
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 2: `format_yaml` holds 256 levels in debug

After Task 1 the formatter's own recursion decides how deep `format_yaml` reaches in debug. Chains of
`?` keys stop first, at 225 levels, with about 9 KiB per level in `explicit_key_to_doc`,
`entry_to_parts`, `explicit_key_parts`, and `join_entries`. The formatter's output is unchanged.

- [x] The handoff reports the deepest level up to 511 at which `format_yaml` finishes on a 2 MiB
      thread in a debug build, for block mappings, block sequences, flow mappings, flow
      sequences, and chains of `?` keys, with the default options, `format_enforce_block_style`,
      and `format_remove_duplicate_keys`, before and after the change
- [x] After the change, each of these shapes finishes at 256 levels with each of these option sets
- [x] The change is in functions whose frames the measurement shows growing per level
- [x] A test runs `format_yaml` at 256 levels of each of these shapes on a 2 MiB thread, and the
      output loads with the input's data
- [x] The formatter's tests and the fixture tests in `tests/fixtures/formatter/` pass with their
      assertions unchanged
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 3: One builder for the server's format options

`formatting`, `range_formatting`, and `code_action` in `rlsp-yaml/src/server.rs` each build
`YamlFormatOptions` field by field, so Task 6 would add its field three times. Behavior is
unchanged.

- [x] One function builds `YamlFormatOptions` from the settings, the editorconfig, and the tab
      width, and the three handlers call it
- [x] The formatting tests in `server.rs`, `tests/editorconfig_integration.rs`,
      `tests/lsp_lifecycle/formatting_integration.rs`, and `tests/lsp_lifecycle/code_actions.rs`
      pass with their assertions unchanged
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 4: Pin the color and code action responses

Task 5 changes where `code_action` and `document_color` take their AST from; no test sends a
`textDocument/documentColor` request today.

- [x] A test sends `textDocument/documentColor` for a document with colors and pins the response
- [x] The code action tests cover a document that does not load, so the empty AST stays pinned
- [x] The new tests pass at this commit, before Task 5
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 5: Code actions and colors read the stored AST

`code_action` and `document_color` parse the text again, while the other handlers read the AST
the `DocumentStore` holds; each extra parse would need the limit too. Behavior is unchanged.

- [x] `code_action` and `document_color` read the AST of the `DocumentStore`, as
      `folding_range` does, and parse no text
- [x] The tests of Task 4 and the code action tests pass with their assertions unchanged
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 6: The formatter takes the nesting limit as an option

`format_yaml` loads the text at three sites, the main load, the blank-line restore, and the
final-newline check; all three use one limit.

- [x] `YamlFormatOptions` has a nesting limit with the library default as its default, and all
      three loads in the formatter use it
- [x] A text deeper than the limit comes back unchanged, and a test at the limit plus one and at
      the limit pins it
- [x] The tests that format 400 or more levels pass the limit 512, and each uses an input whose
      formatted form differs from its text and asserts that the output differs from the input, so
      the fallback for a text deeper than the limit cannot pass them
- [x] The fixture tests in `tests/fixtures/formatter/` pass unchanged, and no fixture key is added
      for the limit
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 7: The default nesting limit is 256

- [x] `rlsp-yaml-parser` exports one constant for the default nesting limit, 256, and
      `LoaderOptions::default`, `YamlFormatOptions::default`, and `rlsp-yaml/src/parser.rs` use
      it; the constant of `parser.rs` is gone
- [x] `load()` with default options reports `NestingDepthLimitExceeded` at 257 levels and loads
      256, and a test pins the default
- [x] `parse_yaml(text)` loads at the default, and the tests and benchmarks that call it are
      unchanged
- [x] No comment or test names the removed constant of `parser.rs` or states its value
- [x] The docs of the limit in `loader.rs`, `rlsp-yaml-parser/README.md`, and
      `rlsp-yaml-parser/docs/architecture.md` state 256, say that it guards the stack when
      loading without alias expansion, and no longer say that it fits the stack on every build
      regardless of depth
- [x] Both `docs/feature-log.md` files tell a reader that `load()`, `format_yaml`, and YAML
      embedded in strings now stop at 256 levels by default
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 8: The server setting `maxNestingDepth`

- [x] The settings reader takes `maxNestingDepth` as a whole number from 1 to
      `MAX_COLLECTION_DEPTH`; any other value takes the default of 256 and gives the warning other
      rejected values give, and tests pin 0, 1, 512, 513, a string, and a fraction
- [x] A variant of `parse_yaml` takes the limit, `DocumentStore::open` and `DocumentStore::change`
      take the limit, and every server load of a document and every `format_yaml` call of a
      handler use the setting's value; the doc comment of `parse_yaml(text)` says that handlers
      use the variant
- [x] When the effective limit changes, the server parses every open document again and publishes
      its diagnostics; a change of another setting, or of the value to one with the same effective
      limit, parses nothing again; tests pin each
- [x] A document of 300 levels gives a diagnostic at the default and loads with the setting at 512
- [x] `textDocument/formatting` returns no edits for a document of 300 levels at the default, and
      formats it with the setting at 512
- [x] The VS Code extension declares the setting with its default and range in `package.json`, and
      `config.ts` passes it; its tests cover it
- [x] `rlsp-yaml/docs/configuration.md` describes the setting, and `rlsp-yaml/docs/feature-log.md`
      names it
- [x] Build, tests, `cargo clippy --all-targets`, fmt, and the VS Code extension's lint, typecheck,
      and unit tests pass

### Task 9: Embedded documents count toward the validation depth

`validate_content_schema` validates each document embedded in a string from depth 0, so a chain
of `contentSchema` embeddings nests up to nine times `MAX_VALIDATION_DEPTH` levels of
`validate_node`.

- [x] The validation of a document embedded in a string continues at the depth of the string that
      holds it plus one, so no chain of embedded documents nests more than `MAX_VALIDATION_DEPTH`
      levels of `validate_node`
- [x] `embedded_content_is_validated_from_depth_zero` in `budget_tests.rs` is replaced by a test in
      which, with the string at depth `MAX_VALIDATION_DEPTH - 2`, the scalar inside the embedded
      `a: 1` gets its diagnostic, and with the string at depth `MAX_VALIDATION_DEPTH - 1` it gets
      none and the result is undecided; a test through `validate_schema` puts the string at depth
      `MAX_VALIDATION_DEPTH - 2` of an outer document and pins the diagnostic
- [x] `MAX_CONTENT_DEPTH` keeps its incomplete-validation result, and the `contentSchema` cycle and
      embedding tests in `reference_tests.rs`, `budget_tests.rs`, `false_schema_tests.rs`, and
      `schema_validation.rs` pass with their assertions unchanged
- [x] The handoff reports, before and after the change, the smallest thread size in steps of
      16 KiB at which a release build validates a document with eight nested `contentSchema`
      embeddings against a schema that refers to itself through its keywords
- [x] No comment or doc says that an embedded document restarts the depth, and
      `rlsp-yaml/docs/configuration.md` and `rlsp-yaml/docs/feature-log.md` tell a reader that
      content embedded in a string counts toward the depth of the string that holds it
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 10: The server runs on a thread with an 8 MiB stack

tower-lsp polls the handlers inside `serve`, so they run on the thread that drives it: today the
main thread, which has 1 MiB on Windows.

- [ ] The `rlsp-yaml` binary drives the server on a thread with an 8 MiB stack, set by a named
      constant whose comment says why, in a single-thread runtime whose blocking threads get the
      same size
- [ ] A panic on the server thread ends the process with the exit code it has today, a failed
      thread start or runtime build prints its error to stderr and exits non-zero, and the start
      and join path uses no `expect` or `unwrap`
- [ ] `tests/claude_code_stdio_smoke.rs` closes the child's stdin after `exit` and asserts that the
      server exits on its own with success within its timeout
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 11: The server holds both ends of the setting

A stack overflow on the thread that drives the server ends the whole server.

- [ ] A test in a debug build sends a document of 256 levels through `didOpen` and every request
      whose handler in `server.rs` reads the stored AST or calls `format_yaml`, among them
      `documentSymbol`, `selectionRange`, `foldingRange`, `semanticTokens/full`, `hover`,
      `completion`, `definition`, `references`, `prepareRename`, `rename`, `documentLink`,
      `codeLens`, `documentColor`, `formatting`, `rangeFormatting`, `onTypeFormatting`, and
      `codeAction` with the flow-to-block and block-to-flow actions, on a thread of 2 MiB, and the
      server answers each; `didOpen` also validates against a schema whose `contentSchema` loads
      YAML of 256 levels embedded in a string, and against a chain of eight `contentSchema`
      embeddings whose innermost string holds YAML of 256 levels at the deepest validated depth
- [ ] In a release build, a document of 512 levels with a recursive schema that uses composition
      keywords, and the chain of eight embeddings, go through `didOpen` and every request of the
      debug test with `maxNestingDepth` set to 512, and the server answers each; the handoff names
      the command and its result
- [ ] The handoff reports for the debug and the release run the smallest thread size, in steps of
      16 KiB, at which each passes, as a share of the server thread's 8 MiB, and each share is at
      most 75%
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

## Non-Goals

- `MAX_COLLECTION_DEPTH` of the event parser
- The depth of trees that alias expansion builds in resolved mode
- An iterative loader, `stacker`, or a flat AST

## Decisions

- **Smaller loader frames, default 256:** the shape of `spike/small-frames` measured the same
  results and speed, and far more depth. Declined: an iterative loader, `stacker`, a flat arena AST, a default of 128, and only
  documenting the abort.
- **The spike's shape with its shortcuts fixed:** an enum in place of a `bool`, the slot stated as
  a contract, the impossible case explicit, a reason on each `#[inline(never)]`, and depth tests.
  Declined: taking the spike as it is.
- **The formatter's growing frames shrink until every shape holds 256 in debug:** the measurement
  names the functions, and chains of `?` keys stopped at 225 levels in the explicit key path, not
  in the scalar arm of `render_node`. Declined: splitting the scalar arm regardless of the
  measurement, every shape at 511 in debug, leaving chains of `?` keys at 225, and splitting the
  render path in any case.
- **The server is measured at both ends of the setting:** debug at 256 for every request that
  reads the AST, and release at 512. A stop in either goes to the user,
  because the setting's range would then be unsafe. Declined: release only, and no measurement.
- **Resolved mode stays out:** `expand_node` can build trees deeper than the limit from alias
  chains; the limit's documentation says what it guards, and the topic stays on `.ai/backlog.md`.
  Declined: a task in this plan.
- **Two refactors in the server come first:** one builder for the format options, and the stored
  AST in `code_action` and `document_color`, so the limit is set at fewer sites.
- **`DocumentStore::open` and `change` take the limit:** the server parses open documents again
  itself when the setting changes. Declined: a store that holds the limit and parses again on its
  own.
- **Embedded YAML in strings keeps the library default:** it is a value inside a document, and
  passing the setting there would change the schema validation's entry points. With the setting
  above 256, embedded YAML deeper than 256 levels reports that it cannot be parsed.
- **A rejected value takes the default:** 0, a value above `MAX_COLLECTION_DEPTH`, and a wrong
  type take 256 with the existing warning; no value is clamped.
- **One constant for the default:** the parser crate exports it, and the server's default uses it.
- **The limit is a server setting, not a formatter setting:** `YamlFormatOptions` carries it for
  the handlers, and `maxNestingDepth` reaches the VS Code extension through Task 8; it has no
  fixture key, since fixtures test formatting styles.
- **Gate demands of Task 1:** the mixed-nesting test counts each collection exactly, and the handoff
  carries a table of stack use per depth.
- **Gate demands of Task 2:** a margin of about 282 levels in debug, a measurement per shape in the
  handoff, and no larger stack for the new test.
- **Gate demands of Task 3:** tests through all three handlers pin every format setting and the
  editorconfig fallbacks, and they pass against the code before the refactor.
- **Gate demands of Task 6:** each load site of the formatter has a mutation check, the field has a
  doc comment and one shared loader serves all three loads, and the root `CLAUDE.md` Settings Sync
  section says that the limit gets no fixture key.
- **Gate demands of Task 8:** the reload never overwrites a newer text and skips its publish once
  the limit has moved on, the check for that is the tested predicate `load_is_current`, the old and
  new limit are compared under one settings lock, and the reader reads whole numbers only.
- **The server drives its handlers on a thread of 8 MiB:** tower-lsp polls them on the thread
  that drives `serve`, the formatter needs about 81% of 1 MiB at 512 levels in release and of 2 MiB
  at 256 in debug, and the main thread has 1 MiB on Windows. A single-thread runtime, since the
  handlers already run on one thread and workers would each reserve the size. Declined: accepting
  the 81%, shrinking the formatter's frames further, and a multi-thread runtime.
- **Embedded documents count toward the validation depth, in this plan:** a crafted document and
  schema nested `contentSchema` up to nine times `MAX_VALIDATION_DEPTH` levels, about 956 KiB in
  release. An embedded document at depth d is now validated to `MAX_VALIDATION_DEPTH - d` levels,
  silently below that as in the outer document, and a `contentSchema` cycle deeper than eight
  levels between its embeddings ends complete without the incomplete-validation warning. Declined: a
  separate plan, the backlog only, and a warning at the depth cut.
- **Gate demands of Task 9:** the depth is a required parameter of every function in the scalar
  chain, the measurement also covers an embedded string of 250 levels, and the docs name no
  number.

## Task Notes

- Task 2: with the small loader frames, four of the depth tests in `rlsp-yaml/tests/formatter_keeps_data.rs`
  still overflow 2 MiB at 511 levels, in the formatter; the measurement covers their shapes
- Task 7: the depth tests that rely on the default limit (`nested_collection_keys_load_on_a_two_mib_stack`
  and `a_parse_error_deep_in_the_tree_is_returned_unchanged` in `tests/robustness.rs`,
  `a_comment_below_a_deep_chain_of_collections_reaches_what_follows` in `tests/loader_comments.rs`,
  `a_stream_that_ends_deep_inside_collections_is_an_error` in `src/loader.rs`) pass the limit 512
  explicitly and keep their depth
- Task 7: a test runs `format_yaml` with the default `YamlFormatOptions` at the default limit on a
  2 MiB thread in a debug build, for the shape with the least margin
- Task 7: the limit tests of Task 6 in `formatter.rs` and `formatter_keeps_data.rs` pass unchanged
  once the default is 256
- Task 8: a value out of range takes the default with a warning, as Decisions says; it is not
  clamped
- Task 8: `rlsp-yaml/docs/configuration.md` no longer says that embedded `contentSchema` content is
  validated at full depth, since it stops at the default nesting limit
- Task 11: the local branch `wip/server-stack-check` (855543f1) holds the unreviewed debug and release
  tests of the earlier server check (`tests/lsp_lifecycle/stack_depth.rs`) as a starting point; the
  `#[ignore]` reason of the release test names the command for each shape in its own process
