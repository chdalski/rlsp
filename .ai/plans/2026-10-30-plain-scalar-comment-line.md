**Repository:** root
**Status:** InProgress
**Created:** 2026-10-30

# A Comment Line Ends a Plain Scalar

## Goal

A comment line after a plain scalar becomes part of the value:
`port: 8080` followed by `  # default` loads as the string
`"8080 # default"`, and the formatter rewrites the file to
`port: 8080 # default` plus a second `# default`. A comment line must end
the plain scalar as YAML 1.2.2 requires and reach the existing comment
handling. Content after any finished sequence item, explicit key, or
mapping value must be a syntax error instead of silently changing the
data, and a multi-line plain scalar's span must end at its last content
byte so that a comment after it is not taken for a trailing comment.

## Context

- YAML 1.2.2 [130] `ns-plain-char(c)` allows `#` only after an `ns-char`,
  and [134] `s-ns-plain-next-line(n,c)` requires a continuation line to
  start with `ns-plain-char`, so a `#` that starts a line after its
  indentation cannot continue a plain scalar. Spec:
  <https://yaml.org/spec/1.2.2/>.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: multi-line plain scalar spans end at their last content byte
- [ ] Task 2: content after a finished value is a syntax error
- [ ] Task 3: a comment line ends a plain scalar
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: End multi-line plain scalar spans at the last content byte

A multi-line plain scalar's span ends after the last consumed line,
blank lines included, so the loader treats a comment on the next line as
trailing on the scalar. The scalar's `loc.end` is public and these
readers in `rlsp-yaml/src` observe the change:
`analysis/semantic_tokens.rs`, `editing/code_actions/block_scalar.rs`,
`analysis/symbols.rs`, `hover.rs`, `decorators/document_links.rs`, and
`editing/formatter/content_tracking.rs`. Mapping and sequence spans come
from their end events, so their readers (`analysis/folding.rs`,
`analysis/selection.rs`, `completion/cursor_location.rs`) see no change.

- [x] The span of a multi-line plain scalar from `parse_events` and from
      `load()` ends at the scalar's last content byte, with and without
      blank lines or a comment after it and at end of input without a
      newline; a single-line scalar's span is unchanged
- [x] The spans of mappings and sequences that contain or end in a
      multi-line plain scalar are unchanged, with blank lines after it
      included
- [x] `format_yaml` turns `a:\n  v\n  w\n# d\nb: 1\n` into
      `a: v w\n# d\nb: 1\n` and `a:\n  v\n  w\n# d\n` into
      `a: v w\n# d\n`, and formatting each result again changes nothing
- [x] `a:\n  v\n  w\n` and `a:\n  v\n  w` produce no semantic token for
      the value
- [x] The document-symbol range and the hover range of a multi-line plain
      value end at its last content byte
- [x] No comment in `rlsp-yaml/src/editing/formatter/content_tracking.rs`
      says a plain scalar occupies one line
- [x] The block-scalar code action on a multi-line plain value leaves the
      following line on its own line
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 2: Reject content after a finished value

A sequence item or an explicit key that is already complete accepts more
content today: `- "q"\n  w\n` loads as two items. Task 3 would make more
inputs reach this path, so the rejection comes first.

- [ ] These inputs report a syntax error through `parse_events` and
      `load()`, and each error points at the stray line:
      `- "q"\n  w\n`, `- v # c\n  w\n`, `- v\n  k: 1\n`,
      `a: "q"\n  [x]\n`, `a: "q"\n  *y\n`, `a: v # c\n  [x]\n`,
      `? k # c\n  w\n: v\n`
- [ ] These inputs load with today's values: `- v\n  w\n`,
      `- a: 1\n  b: 2\n`, `- - x\n  - y\n`, `- !!str\n  v\n`,
      `- &a\n  v\n`, `-\n  v\n`, `? k\n  w\n: v\n`, `? k\n: v\n`,
      `? |\n  t\n: v\n`
- [ ] The yaml-test-suite event and loader conformance tests
      (`rlsp-yaml-parser/tests/conformance/`) pass
- [ ] `docs/conformance/` names the enforcement site for the affected
      block-collection productions
- [ ] With `rlsp-yaml-parser/docs/feature-log.md`, a reader can tell that
      documents with content after a finished sequence item or explicit
      key now report an error
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 3: End a plain scalar at a comment line

`scan_plain_line_block` treats offset 0 as following whitespace, so the
continuation loop ends the scalar and leaves the comment line unconsumed.
The loader also skips comments after the root node, so a `...` after them
still sets `explicit_end`.

- [ ] Through `parse_events` and `load()`, each of these yields the value
      shown and then a comment: `a: v\n  # d\nb: 1\n` (`v`),
      `a:\n  v\n  w\n  # d\nb: 1\n` (`v w`), `a: v\n\n  # d\nb: 1\n`
      (`v`), `a: v\n\t# d\nb: 1\n` (`v`), `- v\n  # d\n- z\n` (`v`),
      `v\n# d\n` (`v`)
- [ ] `load()` of `port: 8080\n  # default\n` gives `8080` the int tag;
      `a: &x v\n  # d\n` keeps its anchor and `a: !!str v\n  # d\n` its tag
- [ ] These inputs report a syntax error that points at the line after
      the comment: `key: word1\n  # xxx\n  word2\n`, `- v\n  # d\n  w\n`,
      `? k\n  # c\n  w\n: v\n`, `a: v\n  # d\n  [x]\n`, `v\n# d\nw\n`
- [ ] `load()` sets `explicit_end` for `v\n# d\n...\n` and `v # c\n...\n`,
      and `format_yaml` keeps their `...`
- [ ] A test over every yaml-test-suite input asserts, through
      `parse_events` and `load()`, that no plain scalar starts with `#` or
      contains a `#` after whitespace
- [ ] Formatter fixtures in `rlsp-yaml/tests/fixtures/formatter/` pin
      `a: v\n  # d\nb: 1\n` to `a: v\n# d\nb: 1\n`, `a: v\n  # d\n` to
      `a: v\n# d\n`, and `- v\n  # d\n- z\n` to `- v\n# d\n- z\n`, each
      idempotent
- [ ] The yaml-test-suite event and loader conformance tests pass
- [ ] `docs/conformance/bnf-§7.md` entries [130] and [134] name the
      enforcement site and the tests that cover it
- [ ] With `rlsp-yaml-parser/docs/feature-log.md`, a reader can tell that
      a comment line ends a plain scalar and that a continuation after it
      is an error
- [ ] The comment on the wrong-indent guard in
      `rlsp-yaml-parser/src/event_iter/block/sequence.rs` matches whether
      `- v\n  # d\n  - w\n` reaches that guard
- [ ] Build, tests, `cargo clippy --all-targets`, and fmt pass

## Non-Goals

- Comments before a later `---` or `...`, which have no place in the AST
  and which the formatter deletes, and a comment on the last continuation
  line (`a:\n  v\n  w # c\n`), which the lexer drops (separate plan after
  this one)
- Flow sequences that drop entries after a comment (separate plan, queued
  after the comment-preservation plan)
- Alias expansion depth in resolved mode (separate plan, queued)
- The inline `--- v` scalar path, where `--- v\n# d\nw\n` emits the comment
  first and loses `w`: that path never reaches the continuation loop, and
  this plan leaves it as it is today

## Decisions

- **The fix is in the scanner's start state:** offset 0 never follows an
  `ns-char`, so no new branch runs on the hot path and the existing
  empty-result break ends the loop. Declined: a `#` check in the
  continuation loop.
- **A multi-line plain scalar has no semantic token:** today it gets none
  unless it ends the input, where its length is wrong; a token per line is
  a new feature.
- **The int tag criterion uses the default `load()`:** its lossless mode
  applies Core schema resolution too, so naming resolved mode would pin
  less.
- **Stray content is a syntax error as the spec requires:** this includes
  forms that load today without a comment line. Declined: rejecting only
  the forms with a comment line, and a known limit for explicit keys.
- **Behavior that changes besides the values:**
  - A value such as `8080` before a comment line resolves to its real
    type, so schema results and the YAML 1.1 validators see a different
    value.
  - The formatter moves such a comment to the indentation of the next key
    or item.
  - A comment before a later `---` or `...` that survives today as part
    of the value is deleted by the formatter until the plan named in
    Non-Goals lands, as the column-0 form already is.
- **Performance:** no new branch per line in Task 3; Task 2 adds one
  comparison per scalar line; no benchmark criterion.
