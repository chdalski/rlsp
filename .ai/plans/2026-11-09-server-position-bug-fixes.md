**Repository:** root
**Status:** InProgress
**Created:** 2026-11-09

# Server and Position Bug Fixes

## Goal

The open server-and-position bugs of `.ai/backlog.md` give users of `rlsp-yaml` wrong ranges, lost
text, and settings and quick fixes that never take effect. Each is fixed where it arises, together
with three defects of the same causes that the backlog does not list.

## Context

- Paths are under `rlsp-yaml/` unless they name `rlsp-yaml-parser/`; commands are those of the root
  `CLAUDE.md`. New code builds positions with `lsp_util::span_to_lsp` and
  `lsp_util::offset_to_lsp` and adds no column arithmetic of its own.
- **Source lines** are the lines of a text as the parser's public `LineBuffer` splits them: a line
  ends at `\n`, `\r\n`, or a lone `\r`, the line breaks of YAML 1.2 production [28] `b-break`
  (https://yaml.org/spec/1.2.2/#53-line-break-characters). `LineIndex` numbers lines the same way.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: The unused-anchor diagnostic covers `&name`, and its quick fix deletes only the anchor
- [x] Task 2: A multi-line quoted scalar's span ends at its closing quote
- [x] Task 3: Semantic tokens skip multi-line quoted scalars
- [x] Task 4: The block-scalar action keeps trailing blanks out of the value
- [x] Task 5: Color ranges come from the scalar's source text
- [x] Task 6: One modeline reader serves the three modeline keys
- [x] Task 7: Modelines and suppressions read source lines
- [ ] Task 8: Context actions and block-to-flow read source lines
- [ ] Task 9: Range and full formatting keep the formatter's line breaks
- [ ] Task 10: File patterns match characters, not bytes
- [ ] Task 11: `?` in SchemaStore catalog patterns matches one character
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: The unused-anchor diagnostic covers `&name`, and its quick fix deletes only the anchor

`validate_unused_anchors` (`src/validation/validators/anchors.rs`) ranges its diagnostic over the
anchored node, so "Delete unused anchor" (`src/editing/code_actions/delete_anchor.rs`) never finds
the anchor name and is never offered; the code-action and suppression filters also see the wrong
line for collections and empty nodes. The quick fix rewrites the node with `format_subtree`, which
misindents block scalars and merges the next line into them. The AST already holds the anchor's
span as `Node::anchor_loc()`, as `navigation/references.rs` uses it.

- [x] The unused-anchor diagnostic's range is the span of `&name`, for scalars, collections, empty
      nodes, and block scalars, in every document of a stream
- [x] A test feeds the diagnostics of `validate_unused_anchors` into the code-action handler and gets
      "Delete unused anchor" for an anchored scalar, mapping, sequence, empty node, and block scalar
- [x] The quick fix deletes `&name` and the blanks after it, or the blanks before it when a line
      break, a comment, or the end of input follows; the rest of the document stays byte-identical:
      `key: &a value  # keep me` becomes `key: value  # keep me`, `a: &x |\n  line one\nb: 1\n`
      becomes `a: |\n  line one\nb: 1\n`, and `a: &x\n` becomes `a:\n`
- [x] An anchor with only blanks before and after it on its line is deleted with the line and its
      line break: `&x\nk: v` and `&x  \nk: v` become `k: v`; a comment after it stays on the line:
      `&x # note\nk: v` becomes `# note\nk: v`
- [x] No action is offered where deleting the anchor would leave an entry of a flow collection
      without a node, such as `[&a, b]`, `[b, &a]`, and `{&a, b}`; `{k: &a}` gets one
- [x] Each applied fix reloads to the same data as the input without the anchor
- [x] A `rlsp-yaml-disable-next-line` above the `&name` line suppresses the diagnostic for a
      collection and for an empty node
- [x] The tests that pinned the node ranges or the rewrite assert the new ranges and the applied
      document; no comment says the diagnostic covers the node
- [x] `Node::clear_anchor` in `rlsp-yaml-parser/src/node.rs` keeps its behavior and test, and its doc
      comment says what it does without naming a code action
- [x] No action is offered for the anchored empty root of a document without `---`, where an alias
      may exist (an alias found by a fresh check, or a walk that stops at the validator's depth
      limit), or where a character outside the BMP precedes the anchor on its line; a leading BOM
      stays when the anchor's line is deleted
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 2: A multi-line quoted scalar's span ends at its closing quote

`try_consume_single_quoted` and `try_consume_double_quoted` (`rlsp-yaml-parser/src/lexer/quoted.rs`)
end a multi-line scalar's span after the line break, and after a trailing comment, instead of at
the closing quote. The wrong span reaches both `parse_events()` and `load()`.

- [x] The span of a multi-line single- or double-quoted scalar ends right after its closing quote,
      in block and flow context, with `\n`, `\r\n`, and `\r` breaks, and with a trailing comment,
      in the events of `parse_events()` and in the nodes of `load()`
- [x] `["a\n b"\n]` and `["a\n b", c]` yield the same events as before this task, apart from the
      scalar's span
- [x] The span of a mapping or sequence whose last entry is a multi-line quoted scalar is the same
      as before this task
- [x] The event kinds and values and the loaded data of the yaml-test-suite conformance tests are
      unchanged, and the tests that pinned the old scalar spans assert the new ones
- [x] In `rlsp-yaml`, the hover range, document-symbol range, selection range, and `!include` link
      range of a multi-line quoted scalar end at its closing quote; its URL links, the folding
      ranges, and the completion context of a mapping whose last entry is such a scalar are the
      same as before this task
- [x] Completion with the cursor on each line of `"a\n b"\n` and of `"a\n b"\n...\n`, the `...`
      line included, gives the same result as on the same line of `a\n b\n` and `a\n b\n...\n`
- [x] A multi-line quoted scalar gets no semantic string token once its span ends at the closing
      quote
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 3: Semantic tokens skip multi-line quoted scalars

The quoted branch of `classify_scalar_node` (`src/analysis/semantic_tokens.rs`) returns before the
check that skips multi-line scalars.

- [x] A multi-line quoted scalar gets no string token, as a multi-line plain scalar does, and the
      tokens after it on its last line keep their positions
- [x] A single-line quoted scalar keeps its token
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 4: The block-scalar action keeps trailing blanks out of the value

The block-scalar conversion (`src/editing/code_actions/block_scalar.rs`) ends its edit at the
scalar, so blanks after it without a comment end up on the value's last line.

- [x] Converting a scalar followed by blanks only, or by blanks and its own comment, leaves no
      trailing blank in the value, for the literal and the folded variant
- [x] Converting a multi-line quoted scalar leaves the next key on its own line with its data
- [x] An offset off the text gives no action instead of a position at column 0, and blanks before a
      comment that the header cannot carry stay as the comment's separator
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 5: Color ranges come from the scalar's source text

`scan_scalar_for_colors` (`src/decorators/color.rs`) adds offsets in the decoded value to the
column of the scalar's start, so ranges in quoted scalars start one character early and lines of
block and multi-line scalars get the wrong columns. `color_presentation` returns labels without an
edit, which a client applies to the range that `documentColor` reported (LSP 3.17,
`ColorPresentation.textEdit`,
https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#colorPresentation).

- [x] `documentColor` reports each color at its place in the source: `primary: '#ff0000'` gives
      0:10..17, `a: |\n  red\n  blue\n` gives 1:2..5 and 2:2..6, and the same holds for folded,
      double-quoted, and multi-line plain and quoted scalars
- [x] A color in a block scalar's header comment or in a comment after a quoted scalar is not
      reported, and neither is a color written only through escapes or a color function broken
      across a line break, such as `a: rgb(1,\n  2, 3)`
- [x] The tests that pinned the shifted ranges assert the source ranges
- [x] A color function's range counts characters, not bytes, and a function longer than 128
      characters is not reported, so the scan stays linear in the scalar's size
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 6: One modeline reader serves the three modeline keys

`extract_schema_url`, `extract_custom_tags`, and `extract_yaml_version`
(`src/schema/association.rs`) each repeat the loop over the first ten lines; one reader of modeline
values serves all three, so the line-break fix lands in one place. Behavior is unchanged.

- [x] The three extractors read the first ten lines through one function that yields the values of
      a modeline key
- [x] The tests of the three extractors in `association.rs` pass with their assertions unchanged
- [x] New tests pin each extractor's first-match rule and passed against the extractors before
      the refactor
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 7: Modelines and suppressions read source lines

The modeline reader and `build_suppression_map` (`src/validation/suppression.rs`) split with
`str::lines()`, which keeps a lone `\r` inside the line, while the diagnostics they filter number
lines like `LineIndex`.

- [x] The modeline reader and the suppression map read source lines through one function in
      `src/lsp_util.rs` built on `LineBuffer`
- [x] In a document with lone `\r` breaks, `$schema`, the custom-tags modeline, `$yamlVersion`,
      `rlsp-yaml-disable-next-line`, and `rlsp-yaml-disable-file` take effect as they do with `\n`,
      through `didOpen`, and the schema URL and tags hold no text of a later line
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 8: Context actions and block-to-flow read source lines

The context code actions (`src/editing/code_actions.rs`) and block-to-flow
(`src/editing/code_actions/block_to_flow.rs`) split with `str::lines()` and see a `\r` document as
one line.

- [ ] In a document with lone `\r` breaks, both offer and apply the same actions as in the same
      document with `\n`
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 9: Range and full formatting keep the formatter's line breaks

`range_formatting` (`src/server.rs`) splits both texts with `str::lines()`, joins them with `\n`,
and appends `\n`, so editorconfig `end_of_line` and `insert_final_newline` do not reach range
edits, and a `\r` document is one line. `formatting` computes its end position with
`str::lines()`, so for a `\r` document its edit covers only the first line.

- [ ] A range edit's text is the slice of the formatted document for the requested lines, with the
      formatted document's own line breaks: with `end_of_line = crlf`, `cr`, or `lf`, and with
      `insert_final_newline = false`, applying it gives the same lines as full formatting
- [ ] Without an editorconfig `end_of_line`, range formatting writes LF, as full formatting does
- [ ] Requested lines that differ from the formatted document only in their line breaks get an
      edit
- [ ] In a document with lone `\r` breaks, a range that starts below the first line gets an edit,
      and the full-format edit replaces the whole document
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 10: File patterns match characters, not bytes

The glob matcher of `FilePatterns` (`src/schema/association.rs`) compares bytes; a wildcard for
one character needs it to compare characters. Behavior is unchanged.

- [ ] A glob's literals and wildcards stand for characters, so a token can match exactly one
      character; the doc comments of the matcher speak of characters
- [ ] The glob tests and proptests in `association.rs` pass with their assertions unchanged, apart
      from the type of a literal in a test helper
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 11: `?` in SchemaStore catalog patterns matches one character

The SchemaStore catalog (`https://www.schemastore.org/api/json/catalog.json`, fetched on
2026-11-09) holds 11 `fileMatch` patterns with `?`, all for winget manifests, such as
`**/manifests/?/*/*/*/*.*.installer.yaml`; picomatch reads `?` as one character other than a path
separator (https://github.com/micromatch/picomatch, "Basic globbing"). `parse_schemastore_catalog`
(`src/schema.rs`) builds its patterns with `FilePatterns::without_limits`, which reads `?` as a
literal like the `schemas` setting.

- [ ] Catalog patterns are built with their own constructor, in which `?` matches exactly one
      character other than `/`; the `schemas` setting reads `?` as before
- [ ] `/home/u/winget-pkgs/manifests/m/Microsoft/PowerToys/0.80.0/Microsoft.PowerToys.installer.yaml`
      matches the winget installer pattern, and `manifests/mm/` and `manifests//` do not
- [ ] The tests that pin `without_limits` agreeing with `new` within the limits pass unchanged
- [ ] `docs/configuration.md` tells a reader how catalog patterns read `?`, `docs/feature-log.md`
      tells them that winget manifests now get their SchemaStore schema, and no doc comment says
      that catalog patterns follow the rules of the `schemas` setting
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

## Non-Goals

- UTF-16 positions for everything the server sends and receives, including the offsets of document
  links, whose ranges on the continuation lines of a multi-line quoted scalar are wrong today
  (separate plan after this one)
- Color presentations that change data in their scalar's context, such as a hex label replacing an
  unquoted `red` (separate plan after this one)
- `?` and `[...]` in the `schemas` setting

## Decisions

- **Order:** the unused-anchor fix goes first, as the defect with the most effect that depends on
  nothing; the semantic tokens, the block-scalar action, and the colors follow the parser's span
  fix, and range formatting follows the source-line function.
- **The parser fixes the span:** the Crate Boundaries table of the root `CLAUDE.md` allows a parser
  change for an incorrect parse; the event content stays the same.
- **"Delete unused anchor" deletes the token:** the rest of the text stays byte-identical, and the
  node is no longer reformatted. Declined: keeping `format_subtree`.
- **`Node::clear_anchor` stays public:** it loses its last caller in this repository, but the
  Crate Boundaries table allows parser API changes only for a parser bug or new event information,
  and removing it would be a breaking change of the published crate.
- **No quick fix where an entry of a flow collection would lose its node:** deleting the token
  gives `[, b]` or loses the item, and writing `null` changes a tagged value.
- **Colors from the source text:** no mapping from decoded offsets back to the source exists, and
  the parser crate may not add one for this. A color that exists only through escapes is no longer
  reported. Declined: correcting the decoded-value scan.
- **Colors are matched within one source line:** a color function broken across a line break in a
  plain, quoted, or folded scalar is no longer reported, though the decoded-value scan found it at
  a wrong range; matching across lines needed a lookahead that made the scan quadratic. Declined:
  ranges that span lines.
- **Range formatting slices the full-format output:** range and full formatting then agree on every
  line. Without an editorconfig `end_of_line`, range formatting writes LF; a client that inserts
  text without matching the document's line breaks gets mixed breaks in that range. Declined: the
  document's own line break.
- **Source lines through `LineBuffer`:** one definition of a line, the parser's. Declined: a split
  at each site.
- **`?` in catalog patterns matches a leading dot:** picomatch's `?` does not match a leading dot,
  but this project's `*` does, and the catalog's winget patterns place `?` where no dot occurs.
- **Multi-line quoted scalars get no semantic token,** as multi-line plain scalars: a token may span
  lines only if the client declares `multilineTokenSupport` (LSP 3.17,
  https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_semanticTokens),
  and the server does not read that capability.
- **A multi-line quoted root scalar is placed like a multi-line plain one:** completion located
  documents by the old span, which ran to the next line; a multi-line plain scalar already ends at
  its last content byte (`multiline_plain_scalar_span_ends_at_last_content_byte` in
  `rlsp-yaml-parser/tests/smoke/scalars.rs`).
- **A range differing only in line breaks gets an edit:** range formatting yields the lines of full
  formatting, line breaks included.
- **Gate demands of Task 1:** the quick fix rechecks for an alias and is refused where the check stops
  at the validator's depth limit, for a bare document's empty root, and where a character outside
  the BMP precedes the anchor on its line (columns stay code points until the UTF-16 plan); the
  flow-entry rule reads the AST; a leading BOM stays when the anchor's line is deleted.

- **Gate demands of Task 2:** the semantic-token guard for multi-line scalars moves ahead of the
  quoted branch in this task, so the shorter span gives no string token of meaningless length.
- **Task 2's link criterion is narrowed:** an `!include` link of a multi-line quoted path takes the
  scalar's span and so ends at its closing quote, like hover and symbols; URL links do not read the
  span end and stay the same.

- **Gate demands of Task 4:** the action's position conversion fails closed on an offset off the
  text, and the blanks before a comment that stays after the replacement remain its separator.

- **Gate demands of Task 5:** `try_color_function` counts characters, not bytes; the search for `)`
  stops after 128 characters; text and AST are read under one lock; double-quoted escapes are
  masked so a name joined through an escape is not reported, and lines joined by an escaped line
  break are scanned together with matches kept within one source line; linearity tests cover large
  scalars.

- **Gate demands of Task 6:** the existing tests did not pin the extractors' first-match rules, so
  tests that pin them came first and passed against the unchanged extractors (WIP commit 39124edb)
  before the refactor.

## Task Notes

- **Task 1:** An anchor alone on its line before a comment loses the anchor and the blanks after
  it; the blanks before it stay: `  &x # n\nk: v` becomes `  # n\nk: v`
