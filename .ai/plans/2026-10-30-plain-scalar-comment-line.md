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
byte so that a comment after it is not taken for a trailing comment. An
alias or flow key after a finished entry must load as its own key, so that
the stricter rejection reports no error for valid documents.

## Context

- YAML 1.2.2 [130] `ns-plain-char(c)` allows `#` only after an `ns-char`,
  and [134] `s-ns-plain-next-line(n,c)` requires a continuation line to
  start with `ns-plain-char`, so a `#` that starts a line after its
  indentation cannot continue a plain scalar. Spec:
  <https://yaml.org/spec/1.2.2/>.
- Every behavior this plan changes is checked against the spec text and
  compared with the reference parser libfyaml 0.9.6. A difference from
  either is fixed in this parser, or reported to the user with the spec
  production and, for a suspected libfyaml defect, the result of checking
  <https://github.com/pantoniou/libfyaml/issues>; the user decides.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: multi-line plain scalar spans end at their last content byte
- [x] Task 2: content after a finished value is a syntax error
- [x] Task 3: alias and flow keys after a finished entry load correctly
- [x] Task 4: a comment line ends a plain scalar
- [ ] Compare `rlsp-yaml-parser-v0.11.3`, `main`, and libfyaml over every
      yaml-test-suite input and every `*.yml` and `*.yaml` file under
      `/workspace`, `/usr/local/cargo/registry`, and
      `~/.local/share/pnpm`: each result that differs from the release
      traces to a task of this plan, and each that differs from libfyaml
      is fixed in an added task or reported to the user as stated in
      Context
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
content today: `- "q"\n  w\n` loads as two items. Task 4 would make more
inputs reach this path, so the rejection comes first.

- [x] These inputs report a syntax error through `parse_events` and
      `load()`, and each error points at the stray line:
      `- "q"\n  w\n`, `- v # c\n  w\n`, `- v\n  k: 1\n`,
      `a: "q"\n  [x]\n`, `a: "q"\n  *y\n`, `a: v # c\n  [x]\n`,
      `? k # c\n  w\n: v\n`, `- "q"\n[x]\n`, `- a: "q"\n [x]\n`,
      `? k\n- b\n: v\n`, `a: "q"\n- b\nc: 1\n`, `a: "q"\n&x\n`,
      `- a: "q"\n  &x\n- b\n`, `a: "q"\n&x\n# c\n`,
      `a:\n  b: "q"\n  &x\n---\n`, `a: 1\n&x\nb: 2\n`, `  - a\n&x\n`,
      `  a: 1\n&x\n`, `  a:\n[x]\n`, `  - a\n*y\n`, `  a: 1\nb: 2\n`,
      `  a: 1\nw\n`, `  - "q"\n w\n`, `  - "q"\n - r\n`,
      `  a: 1\n&x b: 2\n`, `    a: 1\n  &x b: 2\n`
- [x] These inputs report a syntax error through `parse_events` and
      `load()` at the content after the finished node on the same line:
      `- [x] [y]\n`, `- *a w\n`, `a: *b w\n`, `a: [x] y\n`, `- 'q' w\n`,
      `- 'q' [x]\n`, `- 'q' 'r'\n`, `a: 'q' w\n`, `'k' x: v\n`, `"k" x: v\n`,
      `- 'k' x: v\n`
- [x] `  a: 1\n---\nb: 2\n` and `  a: 1\n# c\n  b: 2\n` load as they do
      today
- [x] These inputs load with today's values: `- v\n  w\n`,
      `- a: 1\n  b: 2\n`, `- - x\n  - y\n`, `- !!str\n  v\n`,
      `- &a\n  v\n`, `-\n  v\n`, `? k\n  w\n: v\n`, `? k\n: v\n`,
      `? |\n  t\n: v\n`, `a:\n- b\n&x c: 1\n`
- [x] `a: 1\n[x]: 2\n` reports no error; Task 3 corrects its tree
- [x] `&x\na: 1\n`, `k:\n  &x\n  a: 1\n`, `k: &x\n  a: 1\n`, and
      `a: 1\n&x b: 2\n` load with the property on the expected node
- [x] `- a\n\n  b\n` and `a:\n  - b\n\n    c\n` load one item with the
      value `a\nb` and `b\nc`, matching what a mapping value does today
- [x] The yaml-test-suite event and loader conformance tests
      (`rlsp-yaml-parser/tests/conformance/`) pass
- [x] `docs/conformance/` names the enforcement site for the affected
      block-collection productions
- [x] With `rlsp-yaml-parser/docs/feature-log.md`, a reader can tell that
      documents with content after a finished sequence item or explicit
      key now report an error, and that a blank line inside a sequence
      item's plain scalar folds into one value
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 3: Load alias and flow keys after a finished entry

A key that starts with an alias or a flow collection is taken for the
content of the preceding entry, which nests a mapping: `a: 1\n[x]: 2\n`
loads with a wrong tree, and valid forms such as
`a:\n*y :\n  {x: 1}: v\n` report an error since Task 2.

- [x] A test pins that `parse_events` yields the same events as libfyaml
      0.9.6 for
      `a: 1\n[x]: 2\n`, `a: 1\n{x: 1}: 2\n`,
      `x: &y 1\n[x]: 2\n*y : 3\n`, `? [x]\n[y]: 1\n`,
      `x: &y 1\n? a\n*y : b\n`,
      `x: &y 1\n?\n  !!str k\n*y :\n  !!str v\n`, and
      `x: &y 1\na:\n*y :\n  {x: 1}: v\n`, and `load()` builds the mapping
      those events describe
- [x] A tag on an empty value stays on that value, as libfyaml 0.9.6 has
      it, before a plain, alias, or flow key: `a: !!str\nb: 1\n`,
      `a: !!str\n*z : 2\n`, `a: !!str\n[x]: 2\n`
- [x] No anchor or tag outlives the node it belongs to: `&y\n&k\nb: 2\n` and
      `&y\n&k\n` report an error, `- &y !!str\n- &k\n` and
      `a: &y !!str\n  "q"\nb: &k\nc: 1\n` load, and
      `a: &y !!str\n  [x]\nz:\n  c: 1\n` and
      `a:\n  b: &y !!str\n[x]: 1\nz:\n  c: 1\n` yield the same events as
      libfyaml 0.9.6; a property before `---` or `...` does not reach the
      next document
- [x] Every reject and keep input of Task 2 keeps its result
- [x] The yaml-test-suite event and loader conformance tests pass
- [x] `docs/conformance/` names the enforcement site for the affected
      productions
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

### Task 4: End a plain scalar at a comment line

`scan_plain_line_block` treats offset 0 as following whitespace, so the
continuation loop ends the scalar and leaves the comment line unconsumed.
The loader also skips comments after the root node, so a `...` after them
still sets `explicit_end`.

- [x] Through `parse_events` and `load()`, each of these yields the value
      shown and then a comment: `a: v\n  # d\nb: 1\n` (`v`),
      `a:\n  v\n  w\n  # d\nb: 1\n` (`v w`), `a: v\n\n  # d\nb: 1\n`
      (`v`), `a: v\n\t# d\nb: 1\n` (`v`), `- v\n  # d\n- z\n` (`v`),
      `v\n# d\n` (`v`)
- [x] `load()` of `port: 8080\n  # default\n` gives `8080` the int tag;
      `a: &x v\n  # d\n` keeps its anchor and `a: !!str v\n  # d\n` its tag
- [x] These inputs report a syntax error that points at the line after
      the comment: `key: word1\n  # xxx\n  word2\n`, `- v\n  # d\n  w\n`,
      `? k\n  # c\n  w\n: v\n`, `a: v\n  # d\n  [x]\n`, `v\n# d\nw\n`
- [x] `load()` sets `explicit_end` for `v\n# d\n...\n` and `v # c\n...\n`,
      and `format_yaml` keeps their `...`
- [x] A test over every yaml-test-suite input asserts, through
      `parse_events` and `load()`, that no plain scalar starts with `#` or
      contains a `#` after whitespace
- [x] Formatter fixtures in `rlsp-yaml/tests/fixtures/formatter/` pin
      `a: v\n  # d\nb: 1\n` to `a: v\n# d\nb: 1\n`, `a: v\n  # d\n` to
      `a: v\n# d\n`, and `- v\n  # d\n- z\n` to `- v\n# d\n- z\n`, each
      idempotent
- [x] The yaml-test-suite event and loader conformance tests pass
- [x] `docs/conformance/bnf-§7.md` entries [130] and [134] name the
      enforcement site and the tests that cover it
- [x] With `rlsp-yaml-parser/docs/feature-log.md`, a reader can tell that
      a comment line ends a plain scalar and that a continuation after it
      is an error
- [x] The comment on the wrong-indent guard in
      `rlsp-yaml-parser/src/event_iter/block/sequence.rs` matches whether
      `- v\n  # d\n  - w\n` reaches that guard
- [x] Build, tests, `cargo clippy --all-targets`, and fmt pass

## Non-Goals

- Comments before a later `---` or `...`, which have no place in the AST
  and which the formatter deletes, and a comment on the last continuation
  line (`a:\n  v\n  w # c\n`), which the lexer drops (separate plan after
  this one)
- Flow sequences that drop entries after a comment or accept entries
  without a comma, such as `["q" "r"]` (separate plan, queued after the
  comment-preservation plan)
- Alias expansion depth in resolved mode (separate plan, queued)
- `a: 1\n? b: 2\n: c\n`, a valid compact mapping as an explicit key that
  reports an error today; the explicit-key indentation check is unchanged
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
- **Valid alias and flow key forms error between Task 2 and Task 3:** both
  land in one push, so no pushed state rejects them without its fix.
- **A blank line continues a sequence item's plain scalar:** the item's
  fast path looks only at the next line, so `- a\n\n  b\n` loads as two
  items today, although YAML 1.2.2 folds it to one value. The rejection
  would turn that into a false error, so the fast path is corrected.
  Declined: exempting blank lines from the rejection.
- **Behavior that changes besides the values:**
  - A root alias followed by more content (`*a\nb: 1\n`), a second
    standalone anchor on one node (`&y\n&k\nb: 2\n`), and a tag on the
    column of a bare `?` before an alias key (`?\n!!str\n*y : 2\n`) report
    an error; a property before `---` or `...` no longer reaches the next
    document.
  - A value such as `8080` before a comment line resolves to its real
    type, so schema results and the YAML 1.1 validators see a different
    value.
  - The formatter moves such a comment to the indentation of the next key
    or item.
  - A comment before a later `---` or `...` that survives today as part
    of the value is deleted by the formatter until the plan named in
    Non-Goals lands, as the column-0 form already is.
- **Performance:** no new branch per line in Task 4; Task 2 adds one
  comparison per scalar line and one line scan for flow, alias, and
  property lines; Task 3 adds a few `Option::take` calls per node; no
  benchmark criterion.
