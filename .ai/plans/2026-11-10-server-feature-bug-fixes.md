**Repository:** root
**Status:** InProgress
**Created:** 2026-11-10

# Server Feature Bug Fixes

## Goal

Several language-server features of `rlsp-yaml` crash the server, report wrong diagnostics, return
ranges that miss their position, insert text other than the schema's names, leave out highlighting
tokens, or take time with the square of the document's size. The bugs that `.ai/backlog.md` lists
for these features, apart from the semantic-token positions (see Decisions), and the pre-existing
bugs found in the same code are each fixed where they arise.

## Context

- Paths are under `rlsp-yaml/`; commands are those of the root `CLAUDE.md`. Every task ends with
  build, tests, `cargo clippy --all-targets`, and fmt passing.
- New range code builds positions with `lsp_util::span_to_lsp` and `lsp_util::offset_to_lsp`, except
  the scalar source reader of Task 13 (see Decisions); columns count code points, and the handlers
  in `src/server.rs` convert them to the client's UTF-16 columns (root `CLAUDE.md` Conventions).
- In the inputs of this plan, `\n` and `\r` stand for line breaks; every other backslash is a
  backslash of the text.
- **Depth** is the number of collections that enclose a node; the root node has depth 0.
- The loader bounds depth: documents load with the `maxNestingDepth` setting, default
  `DEFAULT_MAX_NESTING_DEPTH` (256), at most `MAX_COLLECTION_DEPTH` (512,
  `rlsp-yaml-parser/src/limits.rs`). `tests/lsp_lifecycle/stack_depth.rs` runs every request and
  validator on documents 256 levels deep on a 2 MiB stack and, in its ignored release cases, 512
  levels deep on the release stack.
- A **chain** is a selection range with its parent ranges, innermost first. A range contains a
  position when the position lies between its start and its end, both included.
- The Language Server Protocol 3.17 specification (root `CLAUDE.md` References) defines the
  selection-range response and the snippet syntax that Tasks 5, 6, and 10 follow, the Document Link
  Request whose `DocumentLink.range` Task 14 sets, and the Semantic Tokens whose tokens Tasks 15 and
  16 emit, each on one line.
- The YAML 1.2.2 specification (root `CLAUDE.md` References) defines the escape sequences (§5.7), line
  folding (§6.5), the double-quoted style with its escaped line break (§7.3.1), the `''` of the
  single-quoted style (§7.3.2), and the block scalar header with its indicator (§8.1.1) that Tasks 13
  to 16 read; for the characters an escape decodes to, the value the parser loads is the authority,
  and the line breaks of the quoted styles are read as the specification says (see Decisions,
  "Reader against the parser").

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: Long names in validator messages are cut at a character boundary
- [x] Task 2: References and rename share one anchor and alias walk
- [x] Task 3: The unused-anchor validator and its quick fix read the whole document
- [x] Task 4: The other validators read the whole document
- [x] Task 5: No selection range misses its position
- [x] Task 6: A selection-range response has one entry per position
- [x] Task 7: A duplicate-key diagnostic covers the whole key
- [x] Task 8: The block-scalar action never turns a comment into content
- [x] Task 9: A BOM does not hide a modeline or a suppression comment on line 1
- [x] Task 10: The "(all required)" snippet inserts names and defaults as written
- [x] Task 11: The "Delete unused anchor" quick fix answers a request with one pass over the document
- [x] Task 12: A position on a node's text selects that node, not an earlier sibling
- [x] Task 13: Colors and document links read a scalar's source text through one reader
- [x] Task 14: A document link covers the source text of its URL
- [x] Task 15: Every block scalar value gets the token of its indicator
- [x] Task 16: A block scalar key gets the token of its indicator
- [ ] Task 17: The timing tests of document links and colors time the search, not the parser
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Long names in validator messages are cut at a character boundary

`validate_unused_anchors` (`src/validation/validators/anchors.rs`) cuts a long anchor name at byte
100 and panics when that byte lies inside a character; a panic in a handler ends the server.
`push_duplicate_diagnostic` (`src/validation/validators/duplicate_keys.rs`) tests the length in bytes
but cuts by characters, so a key of more than 100 bytes and at most 100 characters gets `...` with
nothing cut.

- [x] A name of at most 100 characters appears in full and without `...` in the unused-anchor and the
      duplicate-key message, whatever its byte length: an anchor `a` followed by 60 `é`, and a
      duplicated key of 60 `é`
- [x] A name of more than 100 characters appears as its first 100 characters followed by `...` in
      both messages, also when byte 100 lies inside a character: an anchor `a` followed by 150 `é`
- [x] One function holds the truncation rule for both messages
- [x] Opening `k: &a` followed by 60 `é` through the server publishes its `unusedAnchor` diagnostic,
      and the server answers a later request

### Task 2: References and rename share one anchor and alias walk

`src/navigation/references.rs` and `src/navigation/rename.rs` hold identical copies of the anchor and
alias walk and its helpers (`collect_anchor_alias_entries`, `collect_node`, `containing_document`,
`span_contains`, `span_to_range`), and `span_to_range` repeats `lsp_util::span_to_lsp`. Task 3 makes
the unused-anchor validator use the same walk. Behavior is unchanged.

- [x] One copy of the walk and its helpers serves references and rename, and code in
      `src/validation/` can call the walk
- [x] Navigation converts spans to ranges only through `lsp_util::span_to_lsp`
- [x] The unit tests of `src/navigation/references.rs` and `src/navigation/rename.rs` and the tests in
      `tests/lsp_lifecycle/navigation.rs` and `tests/lsp_lifecycle/rename.rs` pass with their
      assertions unchanged

### Task 3: The unused-anchor validator and its quick fix read the whole document

`collect_anchors_and_aliases` (`src/validation/validators/anchors.rs`) stops at depth 100 and then
reports from a walk that did not finish: an anchor whose only alias lies deeper is "never used", and
an alias whose anchor lies deeper "has no matching anchor". The quick fix in
`src/editing/code_actions/delete_anchor.rs` refuses every anchor of a document deeper than that and
finds no anchor below depth 100. The validator takes its anchors and aliases from the walk that
Task 2 shares.

- [x] An anchor at depth 0 whose only alias is at depth 99, at 100, at 101, and at 255 gets no
      `unusedAnchor` diagnostic
- [x] An alias at depth 1 whose anchor is at depth 101 and at 255 gets no `unresolvedAlias`
      diagnostic
- [x] An anchor without an alias at depth 101 and at depth 255 gets the `unusedAnchor` diagnostic and
      the "Delete unused anchor" action
- [x] Through the server, the anchor-and-alias document of `tests/lsp_lifecycle/stack_depth.rs`
      publishes no `unusedAnchor` diagnostic at 256 and at 512 levels
- [x] `no_fix_when_an_alias_may_sit_below_the_depth_the_validator_reads` and
      `an_alias_within_the_depth_the_validator_reads_uses_the_anchor` (`delete_anchor.rs`) and
      `the_fix_keeps_the_data_of_an_anchor_with_a_deeply_nested_alias` (`tests/unused_anchor_fix.rs`)
      assert the behavior above; no comment or doc says the validator or the quick fix stops at a
      depth
- [x] The shared anchor and alias walk borrows the names from the document instead of copying them
- [x] The quick fix runs to the bottom of a document 256 levels deep on a 2 MiB stack and, in an
      ignored release case, 512 levels deep on the release stack (`tests/lsp_lifecycle/stack_depth.rs`)

### Task 4: The other validators read the whole document

`flow_style.rs`, `custom_tags_validation.rs`, `yaml11_compat.rs`, `key_ordering.rs`, and
`duplicate_keys.rs` in `src/validation/validators/` each stop at a depth of 100 of their own and miss
what lies deeper: 101 nested block mappings with `k: {a: 1}` at the bottom get no `flowMap`
diagnostic.

- [x] Each of the five validators reports a node at depth 101 and at depth 255 as it reports the same
      node at depth 1
- [x] No validator in `src/validation/validators/` stops its walk at a depth of its own, and no
      comment or doc there says one does
- [x] Through the server, 101 nested block mappings with `k: {a: 1}` at the bottom publish the
      `flowMap` diagnostic
- [x] The cases of `tests/lsp_lifecycle/stack_depth.rs` pass, the ignored 512-level release cases
      included, run as their `#[ignore]` notes describe
- [x] The public function of each of the five validators states in its doc comment that the
      document must be loaded with a finite nesting limit and not in resolved mode
- [x] The depth-test helpers serve all validator tests from `src/test_utils.rs`

### Task 5: No selection range misses its position

`src/analysis/selection.rs` compares columns without their lines, so a chain can hold ranges that do
not contain its position: for `k: "aaa\n  bbb" # c\n` at (1,9) the chain is the scalar (0,3)-(1,6),
its entry (0,0)-(1,6), and the document root (0,0)-(2,0). LSP 3.17 requires `positions[i]` to lie in
`result[i].range`.

- [x] For every position of documents with a multi-line plain, double-quoted, and explicit-key
      scalar, a comment after a multi-line value, and comment lines inside and between entries, every
      range of the returned chain contains the position and every parent range contains its child
      range
- [x] Each chain is the chain the code returned before this task, without the ranges that do not
      contain the position: `k: "aaa\n  bbb" # c\n` at (1,9) gets the document root (0,0)-(2,0) as
      its only range, and the existing selection tests pass with their assertions unchanged, apart
      from those that pin a range that does not contain its position
- [x] A position that got no range before this task gets none: `k: aaa\n  bbb\n` at (1,1), and
      `key: value\n# a comment\nother: data\n` at (1,5)
- [x] Through the server, the selection-range request for `k: "aaa\n  bbb" # c\n` at (1,9)
      (`tests/lsp_lifecycle/selection_ranges.rs`) returns the document root (0,0)-(2,0) as its only
      range

### Task 6: A selection-range response has one entry per position

`selection_ranges` (`src/analysis/selection.rs`) drops a position without a range, so later results
move onto earlier positions, while LSP 3.17 pairs `result[i]` with `positions[i]` and allows the empty
range at `positions[i]` for a position without one: `k: v\n# c\n` with the positions (1,1) and (0,0)
returns one range today, which the client takes for (1,1).

- [x] The response holds one entry per requested position, in request order
- [x] A position without a range gets the empty range at the position, without a parent: (1,1) of
      `k: v\n# c\n`, (1,5) of `key: value\n# a comment\nother: data\n`, (1,0) of
      `key: value\n...\nother: val\n`, and (0,0) when there is no document
- [x] `should_return_empty_for_empty_document`, `should_return_empty_for_cursor_on_dot_dot_dot_line`,
      and the `comment_line_returns_no_selection_range` case of `selection_ranges_regression` assert
      the empty range at their position; the doc comment of `selection_ranges` describes the response
- [x] Through the server, the request with (1,1) and (0,0) on `k: v\n# c\n` returns two entries, the
      empty range at (1,1) and then the chain for (0,0), and a request on an empty document returns
      the empty range at each position

### Task 7: A duplicate-key diagnostic covers the whole key

`push_duplicate_diagnostic` (`src/validation/validators/duplicate_keys.rs`) builds the range from the
key's start line and end column, so a multi-line key gets a range on one line.

- [x] For `"a b": 1\n? "a\n  b"\n: 2\n` the duplicate-key diagnostic covers (1,2)-(2,4)
- [x] A single-line duplicate key keeps its range
- [x] Through the server, the diagnostic published for that document covers (1,2)-(2,4)

### Task 8: The block-scalar action never turns a comment into content

When a scalar has a trailing comment and `trailing_comment_line_end`
(`src/editing/code_actions/block_scalar.rs`) does not find it at the end of the scalar's line, the
action replaces only the scalar, and the comment stays after the block text, where it is content.
Only a document and a text that disagree reach this, so no server test can.

- [x] No block-scalar action is offered for a scalar whose trailing comment the rest of its line does
      not hold: the document loaded from `k: ` plus a value of 40 characters plus ` # c\n`, with a text
      whose comment reads `# d`
- [x] No block-scalar action is offered for a scalar without a trailing comment whose line holds more
      than blanks after it in the text (gate demand, see Decisions)
- [x] The existing tests of the block-scalar action pass with their assertions unchanged

### Task 9: A BOM does not hide a modeline or a suppression comment on line 1

`modeline_values` (`src/schema/association.rs`) and `build_suppression_map`
(`src/validation/suppression.rs`) read line 1 with its leading U+FEFF, which `trim` keeps, so no
`# yaml-language-server:` modeline and no suppression comment on that line matches.

- [x] After a leading BOM, the `$schema`, `$tags`, and `$yamlVersion` modelines and the
      `# rlsp-yaml-disable-file` and `# rlsp-yaml-disable-next-line` comments on line 1 take effect
      as they do without the BOM
- [x] `lsp_util::source_lines` still yields the BOM as part of line 1
- [x] Through the server, a BOM document with a `$schema` modeline gets that schema's diagnostic, and
      a BOM document with `$tags=!x` gets no `unknownTag` diagnostic for `!x`

### Task 10: The "(all required)" snippet inserts names and defaults as written

`src/completion/schema_completions.rs` puts property names and placeholder defaults into the
"(all required)" snippet without the snippet escapes for `\`, `$`, and `}`: a property named
`$CLIPBOARD` makes the client insert the clipboard, and the placeholder of an object default `{}`
ends at its first `}`.

- [x] The snippet inserts each required name exactly as the schema writes it, for names holding `$`,
      `}`, and `\`, and for `$CLIPBOARD`
- [x] The placeholder of an object property covers the whole default `{}`
- [x] Through the server, a completion request against a schema with such names
      (`tests/lsp_lifecycle/completion.rs`, schema served by the fake proxy) returns the escaped
      snippet

### Task 11: The "Delete unused anchor" quick fix answers a request with one pass over the document

`code_actions` (`src/editing/code_actions.rs`) answers each `unusedAnchor` diagnostic in the requested
range on its own, and `delete_unused_anchor` (`src/editing/code_actions/delete_anchor.rs`) walks the
document again for each one: `find_anchor` until the anchor, then `has_alias` over the whole
document. A request whose range covers a document with N unused anchors therefore costs time with
the square of N, about 0.8 s for 5 000 in a release build. One index of the anchors, built at most
once per request, answers every diagnostic.

- [x] One `code_actions` call with all `unusedAnchor` diagnostics of
      `&k key: &v val\nl: [&a, b]\nm: &x 1\nn: &x 2\n---\no: &y 1\np: *y\n`, plus an `unusedAnchor`
      diagnostic at `&y`, whose anchor has an alias, returns the same actions, edits, and order as one
      call per diagnostic, joined in diagnostic order
- [x] Through the server, a code-action request over all lines of a document of the 2 000 lines
      `k0: &a0 v` to `k1999: &a1999 v` returns 2 000 "Delete unused anchor" actions in line order,
      the action of line i removing `&ai ` from line i
- [x] A benchmark group in `benches/insight.rs` measures `code_actions` over the whole range of a
      document with 1 000, 2 000, 4 000, and 8 000 unused anchors; each doubling raises the time by
      at most 2.5 times, and the review request reports the four times
- [x] The benchmark group also measures documents that mix unused anchors with used anchor-alias
      pairs and texts of many documents with one anchor each, with the same bound (gate demand, see
      Decisions)
- [x] The quick-fix cases of `tests/lsp_lifecycle/stack_depth.rs` pass, the ignored 512-level release
      case included
- [x] The unit tests of `src/editing/code_actions/delete_anchor.rs` and `src/editing/code_actions.rs`,
      `tests/unused_anchor_fix.rs`, and the unused-anchor cases of `tests/lsp_lifecycle/code_actions.rs`
      pass with their assertions unchanged

### Task 12: A position on a node's text selects that node, not an earlier sibling

`collect_ancestor_spans` (`src/analysis/selection.rs`) takes the first sibling whose lines hold the
position's line and that yields a range. A block collection's range ends at column 0 of the next
sibling's first line, and flow siblings share a line, so an earlier sibling that does not contain
the position wins, and the sibling that holds it is never tried: for `- a: 1\n  b: "p\n    q"\n- c\n`
at (3,2) the chain belonged to the first item before Task 5 and is the document root alone after
it. The sibling whose range contains the position is tried first; the other siblings follow in
document order as before.

- [x] `a:\n- k: 1\nb: 2\n` at (2,3) gets the chain `2` (2,3)-(2,4), the entry `b` (2,0)-(2,4), and
      the document root
- [x] `{a: 1, b: 2}` at (0,7) gets the key `b` (0,7)-(0,8), the entry (0,7)-(0,11), and the root
      (0,0)-(0,11); at (0,10) it gets the value `2` (0,10)-(0,11), the entry, and the root
- [x] `[a, b]` at (0,4) and `- a: 1\n  b: "p\n    q"\n- c\n` at (3,2) get the scalar under the
      position as their innermost range
- [x] A position that gets no node range before this task gets none: (3,0) and (3,1) of
      `- a: 1\n  b: "p\n    q"\n- c\n`, `k: aaa\n  bbb\n` at (1,1), and
      `key: value\n# a comment\nother: data\n` at (1,5); `{a: 1, b: 2}` at (0,6) keeps the root as
      its only range
- [x] The every-position test of Task 5 covers these inputs and still holds
- [x] Through the server, `- a: 1\n  b: "p\n    q"\n- c: 2\n` at (3,2)
      (`tests/lsp_lifecycle/selection_ranges.rs`) gets the key `c` (3,2)-(3,3), the entry
      (3,2)-(3,6), the item (3,2)-(4,0), and the root (0,0)-(4,0)
- [x] The selection cases of `tests/lsp_lifecycle/stack_depth.rs` pass, the ignored 512-level release
      cases included

### Task 13: Colors and document links read a scalar's source text through one reader

`src/decorators/color.rs` reads a scalar's source text with a reader of its own (`Cell`,
`ScalarLines`, `mask_escapes`, and the header skip of `scan_scalar_for_colors`), and Task 14 needs
the same reading for document links. The reader moves to `src/decorators/scalar_source.rs`. It yields
the scalar's content one logical line at a time: each character with its decoded value, whether an
escape wrote it, and its start and end position. It removes the quotes, reads `''` as one `'`, joins
the lines around an escaped line break, skips a block scalar's header line, and decodes every escape
of the YAML 1.2.2 double-quoted style. Colors turn the characters an escape wrote into the
placeholders they use today, so a color written only through escapes stays unreported.

- [x] For every escape of the YAML 1.2.2 double-quoted style, the characters the reader decodes from
      `k: "` plus the escape plus `"` are the value the parser loads
- [x] The reader's position of each character is the position `LineIndex::line_column` gives its
      source offset, for plain, single-quoted, double-quoted, literal, and folded scalars, with `\n`,
      `\r\n`, and `\r` line breaks and after a BOM
- [x] The colors of a one-line scalar, `k: "` plus `😀 red ` 20 000 times plus `"`, are all reported
      within 5 seconds, the limit of `the_links_of_a_long_scalar_are_found_in_linear_time`
- [x] `src/decorators/color.rs` keeps no source reader of its own, and the tests of
      `src/decorators/color.rs`, `tests/lsp_lifecycle/document_color.rs`, and the color cases of
      `tests/lsp_lifecycle/utf16_positions.rs` pass with their assertions unchanged

### Task 14: A document link covers the source text of its URL

`collect_node_links` (`src/decorators/document_links.rs`) finds a URL in the scalar's decoded value
and uses the URL's character offset there as a column on the scalar's first source line. An escape
or a `''` before the URL moves the link to the left, one inside the URL shortens the link, and a URL
on a continuation line of a multi-line plain or quoted scalar lands on the scalar's first line. The
links of plain and quoted scalars come from the reader of Task 13, one logical line at a time; block
scalars and `!include` keep the node's range.

- [x] A URL after an escape or a `''` covers its source text: `a: "\t https://x.com"` (0,7)-(0,20),
      `key: "\x41 https://example.com"` (0,11)-(0,30), `k: "a\0https://x.com"` (0,7)-(0,20),
      `k: 'it''s https://x.com'` (0,10)-(0,23), and `k: "\x41\x41😀https://e.com"` (0,13)-(0,26)
- [x] A URL holding an escape or a `''` covers its source text and keeps its target:
      `k: "https://x.com/a\x41b"` (0,4)-(0,24) to `https://x.com/aAb`, `k: "https://x.com/\u00e9"`
      (0,4)-(0,24), `k: "https:\/\/x.com\/a"` (0,4)-(0,22) to `https://x.com/a`, and
      `k: 'https://x.com/it''s'` (0,4)-(0,23)
- [x] A URL on a continuation line covers it there: `k: a\n  https://x.com`,
      `k: "a\n  https://x.com"`, `k: 'a\n  https://x.com'`, `k: "a\r\n  https://x.com"\r\n`, and
      `k: "a\` followed by a line break and `  https://x.com"` each get (1,2)-(1,15)
- [x] A URL across an escaped line break, `k: "https://x.com/a\` followed by a line break and `  b"`,
      gets (0,4)-(1,3) and the target `https://x.com/ab`
- [x] The ranges of block scalars and `!include` links, the targets of all links, and
      `url_in_multi_line_quoted_scalar_keeps_its_first_line_range` stay as they are, except for a URL
      on either side of an empty line that follows an escaped line break (next criterion and
      Decisions), and `the_links_of_a_long_scalar_are_found_in_linear_time` passes
- [x] `k: "https://a.b/x\` followed by a line break, an empty line, and `  https://a.b/y"` gets two
      links, (0,4)-(0,17) to `https://a.b/x` and (2,2)-(2,15) to `https://a.b/y`
- [x] Through the server, `key: "\x41 https://example.com"` gets (0,11)-(0,30)
      (`tests/lsp_lifecycle/document_links.rs`, where a test pins (0,8)-(0,27) today) and
      `k: "\x41\x41😀https://e.com"` gets (0,14)-(0,27) in the client's columns
- [x] The doc comment of `find_document_links` and the name and comment of the escaped-link test in
      `tests/lsp_lifecycle/document_links.rs` describe the ranges this task gives

### Task 15: Every block scalar value gets the token of its indicator

`classify_scalar_node` (`src/analysis/semantic_tokens.rs`) computes a token length as the end column
minus the start column and returns at length 0 before its block-scalar branch. A block scalar's span
ends on a later line, so a block scalar followed by a line break, the usual case, gets no operator
token for its `|` or `>`. Tokens are written as (line, start, length, type).

- [x] A block scalar value gains the operator token of its indicator, and every other token stays as
      it is today: (0,3,1,operator) for `k: |\n  text\n`, `k: >-\n  some long text\n`, and
      `k: |\nk2: v\n`; (0,0,1,operator) for `|\n x\n`; (0,2,1,operator) for `- |\n    abcdef\n`;
      (1,5,1,operator) for `a:\n  b: |\n    xyz\n`; (0,6,1,operator) for `k: !t |\n  x\n`, after
      its type token (0,3,2,type); (0,3,1,operator) for `k: | # c\n  x\n`, before its comment
      token (0,5,3,comment)
- [x] `text: |` and `k: |\n  tttt` at the end of the text keep their operator tokens (0,6,1,operator)
      and (0,3,1,operator), and every existing semantic-token test passes with its assertions
      unchanged, or, when new table rows cover it (see Decisions), is removed
- [x] Through the server, the semantic tokens of `k: |\n  text\n` hold the operator token
      (0,3,1,operator)

### Task 16: A block scalar key gets the token of its indicator

The key branch of `collect_node_tokens` (`src/analysis/semantic_tokens.rs`) reads every scalar key
as key text: it emits a property token for a key on one line and never the operator token of a block
indicator. A block scalar key spans lines, so it gets no token, and `? |` at the end of the text gets
a property token for its indicator. The key branch emits the operator token of Task 15 for a block
scalar key; other keys keep today's rule.

- [x] A block scalar key gains the operator token of its indicator, and every other token stays as it
      is today: (0,2,1,operator) for `? |\n  k\n: v\n`, `? >-\n  k\n: v\n`, `? |\n  k\n`,
      `? |\n: v\n`, and `? | # c\n  k\n: v\n`; (0,5,1,operator) for `? !t |\n  k\n: v\n` and
      `? &a |\n  k\n: v\n`; (0,8,1,operator) for `? &a !t >\n  k\n: v\n`; (1,4,1,operator) for
      `a:\n  ? |\n    k\n  : v\n`; (0,4,1,operator) for `- ? |\n    k\n  : v\n`
- [x] `? |\n  k\n: |\n  v\n` gets (0,2,1,operator) and (2,2,1,operator) and no other token
- [x] `? |` at the end of the text gets (0,2,1,operator) instead of its property token
- [x] Other keys keep their tokens: `? "a\n  b"\n: v\n`, `? a\n  b\n: v\n`, and `{"a\n b": v}\n` get
      no key token; `? "ab"\n: v\n` gets (0,2,4,property) and `? ab\n: v\n` (0,2,2,property); every
      existing semantic-token test passes with its assertions unchanged, or, when new table rows
      cover it (see Decisions), is removed
- [x] Through the server, the semantic tokens of `? |\n  k\n: v\n` hold (0,2,1,operator)

### Task 17: The timing tests of document links and colors time the search, not the parser

The loader takes time with the square of the length of a double-quoted line of escapes (a parser
bug in `.ai/backlog.md`), so `escapes_and_urls_on_one_long_line_are_found_in_linear_time` and
`a_long_scalar_without_a_url_is_skipped_quickly` (`src/decorators/document_links.rs`) and the
`one_long_line_of_escapes` case of `large_scalars_without_colors_are_scanned_in_linear_time`
(`src/decorators/color.rs`), which load such a line inside their timer or test, time the parser.
Each loads before the timer and times only the search, like
`the_colors_of_many_scalars_on_one_line_are_found_in_linear_time`.

- [ ] The two document-link tests load their document before `Instant::now()`, time only
      `find_document_links`, and assert that it ends within 5 seconds; their inputs and their other
      assertions stay as they are
- [ ] The color case of a long line of escapes becomes a test of its own that loads `a: "` plus
      `\x41` 20 000 times plus `"` before `Instant::now()`, times only `find_colors`, asserts that it
      ends within 5 seconds, and asserts that it finds no color
- [ ] No `#[timeout]` and no timed `Instant` region of a test in `rlsp-yaml/` holds a parser load
      of a long line of escapes
- [ ] `cargo test -p rlsp-yaml --lib decorators::` passes, and the review request reports the
      measured time of each of the three timed searches in a debug build

## Decisions

- **Run rules:** a pre-existing bug found while fixing Tasks 4, 6, 11, 12, 16, or 17 goes to
  `.ai/backlog.md`, and one found while fixing another task becomes a new task of this plan —
  Tasks 4, 6, 11, 12, and 16 are already bugs found in this code, Task 17 fixes tests this plan
  added, and attaching only one generation of found bugs lets the run end. A fix whose correct
  behavior is open, or that changes visible behavior beyond its bug, goes to the backlog's Open
  decisions, since the user decides those.
- **Semantic-token positions:** their UTF-16 columns and lengths are not fixed here; the server's
  boundary conversion fixed them.
- **Whole-tree walks:** the anchor validator, its quick fix, and the other validators walk the whole
  document instead of stopping at a depth of their own — the loader already bounds depth, and
  `stack_depth.rs` measures every validator at that bound; a walk that stops reports absences it has
  not checked.
- **Gate demands on Task 3:** the shared walk borrows names instead of copying them — copying made
  the quick fix about three times slower with 5 000 unused anchors; and the quick fix has its own
  stack-depth cases — the anchor-and-alias document never reaches the fix, since its anchor is used.
- **Gate demands on Task 4:** the five validators document the load they rely on, since only that
  load bounds their walks now; and the depth-test helpers moved to `src/test_utils.rs` instead of
  being copied into five test modules.
- **Truncation:** the shared rule keeps the `...` suffix of both messages; the `…` helpers in
  `server.rs` and `completion/formatting.rs` cap a total length with another suffix and stay apart.
- **Selection:** Task 5 only leaves out ranges that do not contain the position and adds none.
  Selecting a node for a position that gets no range today, such as the indentation of a continuation
  line or a comment line, changes visible behavior beyond the bug. Task 12 changes which sibling is
  tried first, not which positions get a node range.
- **Anchor index:** the quick fix keys its index by the anchor's range and keeps the shared alias
  walk of navigation, so the fix and the validator count an anchor as used by the same rule; its
  actions, edits, and their order stay as they are.
- **BOM:** the modeline and suppression readers skip it; `source_lines` keeps it, because its other
  callers count columns from the line text, and the BOM takes one column there.
- **Block-scalar guard:** when the trailing comment cannot be found on the line, the action is not
  offered — an edit could otherwise turn the comment into content.
- **Gate demands on Task 11:** the benchmark measures two more shapes, anchors mixed with used
  anchor-alias pairs and many documents, since a per-anchor alias scan or document search would stay
  quadratic in a document of unused anchors alone; and a document whose anchors stand on one line
  was measured, which stays quadratic through the line-prefix count (a backlog entry).
- **Gate demand on Task 8:** the guard also covers a scalar without a trailing comment in the
  document: the action is offered only when the rest of its line in the text is blanks, since text
  there, such as a comment the document does not hold, would also become content.
- **Scalar source reader:** colors and document links map a scalar's decoded characters to their
  source text with one reader that decodes the escapes inside the scalar's parser-provided span — the
  arithmetic on parser spans that the root `CLAUDE.md` Crate Boundaries allow; the event stream holds
  no map from value offsets to source offsets. The reader counts positions once per line instead of
  calling `span_to_lsp` or `offset_to_lsp` per match, so a long line stays linear.
- **Link ranges:** a link in a plain or quoted scalar covers its URL's source text on the lines that
  hold it, so a URL on a continuation line moves to that line; a URL across an escaped line break
  gets a range over both lines instead of being dropped, which would remove a link that works today.
  Links in block scalars keep the node's range, an open decision in `.ai/backlog.md`.
- **Reader against the parser:** after an escaped line break, the parser drops the empty lines that
  YAML 1.2.2 [112] reads as line feeds (`"a\` followed by a line break, an empty line, and `  b"`
  loads as `ab`, not `a\nb`; a pre-existing parser bug in `.ai/backlog.md`). The reader follows the
  specification there, so a link stops at such a line instead of joining the text before and after
  it; the parser's value stays the authority for the decoding of escapes. The parser bug lies in
  `rlsp-yaml-parser`, outside this plan's Goal, so it goes to the backlog for a parser plan instead
  of becoming a task here.
- **Replaced tests:** in Tasks 15 and 16, an existing semantic-token test that new table rows cover
  is removed instead of kept beside them, since a test kept beside rows that assert the same is a
  duplicate to maintain. A row covers a test when it has the same input, is checked at the same
  entry point (`absolute_tokens`, or the server request), and asserts every token and every absence
  the test asserted; each case of a parametrized test counts as one test. The handoff names each
  removed test with its rows.
- **Timing tests:** a test that guards the linear time of a feature loads its document before the
  timer and times only the feature, since the parser's own time is not what it guards; a longer
  timeout or a smaller input would keep the parser in the timed part. The color case of escapes
  shrinks to 20 000 escapes, the size of its document-link twin, so its load before the timer stays
  short.
- **Indicator tokens:** a block scalar's `|` or `>` gets one operator token, for a value and for a
  key, built in one place; a block scalar key's indicator at the end of the text turns from a
  property token into that operator token, since the same cause gave it the property token.
  Multi-line keys of the other styles still get no token.

## Non-Goals

- Selecting a node for a position that gets no range today — the indentation of a multi-line
  scalar's continuation line, a comment line inside or between entries — an open decision in
  `.ai/backlog.md`
- Quoting a schema property name that YAML would read as another key, in the completion items and
  the snippet — an open decision in `.ai/backlog.md`
