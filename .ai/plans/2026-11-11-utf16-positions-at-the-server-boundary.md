**Repository:** root
**Status:** InProgress
**Created:** 2026-11-11

# UTF-16 Positions at the Server Boundary

## Goal

`rlsp-yaml` sends and reads LSP columns in code points, while clients count them in UTF-16 code
units, so on a line that holds an astral character every position after it is off by one column per
such character: diagnostics, semantic tokens, edits, and the cursor positions of requests. Every
position the server sends is converted from code points to UTF-16 and every position it reads is
converted back, at the boundary in `server.rs`, while the features keep counting code points.

## Context

- Paths are under `rlsp-yaml/`.
- The Language Server Protocol 3.17 specification (root `CLAUDE.md` References) defines what the
  tasks convert to: section "Text Documents" (lines end at `\n`, `\r\n`, or `\r`; the position
  encoding is `utf-16` when the server's initialize result names none), section "Position" with
  `PositionEncodingKind` (a column counts code units of that encoding), and section "Semantic
  Tokens" (`deltaStart` and `length` are encoded in that encoding).
- The server names no position encoding (`initialize` in `src/server.rs`), and the VS Code client
  offers only UTF-16: `vscode-languageclient` 11.0.0, the version the VS Code extension depends on,
  sets `generalCapabilities.positionEncodings = ['utf-16']` in `lib/common/client.js`. Text sync is
  `FULL`, so no change ranges come in.
- Features compute positions from parser spans (byte offsets) with `lsp_util::span_to_lsp` and
  `lsp_util::offset_to_lsp`, whose columns come from `rlsp_yaml_parser::LineIndex::line_column` and
  count code points.
- An **astral character** is a character outside the Basic Multilingual Plane (U+10000 and above):
  one code point, two UTF-16 code units.
- An **internal position** has a code-point column: features compute and read these. A **client
  position** has a UTF-16 column: the server sends and receives these.
- The **converter** is the value built from one document text that maps a position between internal
  and client columns in both directions, and the **boundary trait** applies it to every position
  inside a request parameter or response value; both live in `src/position_encoding.rs`.
- A **document handler** is a request handler in `src/server.rs` that reads an open document. Code
  lens and color presentation read none: code lens answers the range (0,0), which is the same in
  both units, and color presentation reads and answers no position.
- `tests/lsp_lifecycle/utf16_positions.rs` holds the server cases of this plan, on documents with an
  astral character before the positions they check.
- Each task leaves no doc comment, comment, test name, test skip, or allowlist reason in `src/` or
  `tests/` that describes the position unit, UTF-16, or the behavior on lines with an astral
  character otherwise than the code does after the task, in the code it changes or elsewhere.

## Steps

- [x] Clarify scope with the user
- [x] Design review of the approach
- [x] Task 1: Tests pin the answers the conversion keeps
- [ ] Task 2: Every document handler reads its document through one function
- [ ] Task 3: Published diagnostics and code actions use client positions
- [ ] Task 4: Requests that take a position read and answer client positions
- [ ] Task 5: Whole-document responses use client positions
- [ ] Push to `main` and confirm the full CI matrix is green

## Tasks

### Task 1: Tests pin the answers the conversion keeps

Tasks 2 to 5 change how handlers read documents and positions, and some answers must come out as
they are today. Server tests pin those answers first, so a later task shows that it kept them.
Test-only.

- [x] For every document handler, a server test sends the request for a URI that was never opened
      and asserts the exact answer without item it gives: `null`, and `[]` for document color
- [x] Server tests pin, on a line with an astral character before the position, the answers that are
      right today: the edit of the block-scalar code action, the end of the full-format edit, and
      the range of a document link whose astral character lies inside the scalar before the URL
- [x] A server test pins the range of a document link in a single-line double-quoted scalar with an
      escape sequence before the URL as it is today, and its comment states the LSP 3.17 clause the
      range departs from and why the range stays as it is
- [x] The tests pass against the code before this task

### Task 2: Every document handler reads its document through one function

Each document handler copies a lock block to read text and AST; Tasks 3 to 5 build the converter
from the text, which then happens in one place. Behavior is unchanged.

- [ ] Every document handler gets the text and the parsed documents it reads from one function in
      `server.rs`, which takes them under one lock of the document store; no document handler locks
      the document store itself
- [ ] A document handler gets a copy of the parsed documents only if it reads them: formatting and
      range formatting get none
- [ ] Each document handler answers an open document without parsed documents as it does today
- [ ] The tests in `tests/lsp_lifecycle/` and the unit tests in `src/server.rs` pass with their
      assertions unchanged

### Task 3: Published diagnostics and code actions use client positions

The converter and the boundary trait come into being, and the first two paths go through them: the
diagnostics the server publishes, and code actions, which read a range and answer edits and the
diagnostics they fix. These two go together so that a published diagnostic and the one attached to
its action never differ. LSP 3.17 sections "Text Documents" and "Position" apply.

- [ ] The converter's unit and property tests show, on texts drawn from `a`, `😀`, `é`, `\r`, and
      `\n`:
  - [ ] each client column equals the UTF-16 length of the line's text before the internal column,
        computed independently of the converter
  - [ ] converting a position to the client and back returns it, for every position not inside an
        astral character, past the line end included
  - [ ] a client column inside an astral character converts to the internal column of that character
  - [ ] a column past the line end keeps its excess in both directions, and the internal column
        `u32::MAX` converts to the client column `u32::MAX`
  - [ ] lines are split at `\n`, `\r\n`, and a lone `\r`, as `LineIndex` splits them; a line past
        the last line and an empty text convert positions unchanged
- [ ] For a text without astral characters, building the converter scans the text once and
      allocates nothing, and every conversion returns its input
- [ ] The boundary trait converts every position inside each response type it covers, nested values
      included, and each implementation names every field of its struct, so a new field in
      `lsp-types` fails to compile until it is handled
- [ ] Through the server, the `unusedAnchor` diagnostic published for `😀: &a v\n` covers (0,4)-(0,6)
- [ ] Through the server, "Delete unused anchor" is offered for `😀: &a v\n`, and its edit, applied
      with UTF-16 columns, gives `😀: v\n`; the diagnostic attached to the action equals the
      published one
- [ ] Through the server, for every action kind that code actions offer, on a document with an
      astral character before the action's node on its line, the action's edit applied with UTF-16
      columns gives the text that the same action gives for the document with `x` in place of the
      astral character, with the astral character back in place of `x`
- [ ] Through the server, a code-action request whose range starts after an astral character on its
      line gets the actions for the node at that client position
- [ ] The diagnostics the server stores for code actions keep internal positions, and the doc
      comment of the stored diagnostics says so
- [ ] The block-scalar action computes its positions in code points, as the other actions do, and the
      server test of Task 1 for its edit passes unchanged
- [ ] `tests/unused_anchor_fix.rs` checks the fix on lines with an astral character too
- [ ] The test helpers in `src/` and `tests/` that build, apply, or check the positions, edits, or
      ranges that features take or answer, outside the server, count internal columns

### Task 4: Requests that take a position read and answer client positions

Hover, completion, definition, references, prepare rename, rename, selection ranges, on-type
formatting, formatting, and range formatting convert the position or range they read and the
positions they answer. Each handler converts both directions in this task, because selection ranges
and on-type formatting answer with the client's own column. LSP 3.17 section "Position" applies.

- [ ] Through the server, hover, completion, definition, references, prepare rename, rename,
      selection ranges, and on-type formatting at a position after an astral character on its line
      answer for the node at that client position, and every range they answer is in client
      positions
- [ ] Through the server, a selection-range request at a position without a range answers the empty
      range at the exact client position it was sent
- [ ] Through the server, a range-formatting request whose range lies on lines with an astral
      character formats the same lines as before this task
- [ ] The end of the full-format edit is computed in code points inside the server, and the server
      tests of Task 1 for formatting pass unchanged
- [ ] On its 2 MiB stack, `tests/lsp_lifecycle/stack_depth.rs` answers the selection-range request on
      a deep document whose lines hold an astral character, with every range in client positions,
      since the boundary trait walks the nested parent ranges

### Task 5: Whole-document responses use client positions

Document symbols, folding ranges, document links, document colors, and semantic tokens answer client
positions. Semantic tokens are converted with absolute positions, before the delta encoding, so start
and length both count UTF-16 units. LSP 3.17 sections "Position" and "Semantic Tokens" apply.

- [ ] Through the server, for a document with an astral character before them on their lines, the
      ranges of document symbols, document links, and document colors are in client positions
- [ ] Through the server, the semantic token after an astral character starts at its client column
      and its length counts UTF-16 units, as does the delta start of the token after it
- [ ] Document links compute their columns in code points; the server tests of Task 1 for document
      links pass unchanged
- [ ] On its 2 MiB stack, `tests/lsp_lifecycle/stack_depth.rs` answers the document-symbol request on
      a deep document whose lines hold an astral character, with every range in client positions,
      since the boundary trait walks the nested child symbols
- [ ] Every document handler converts the positions it reads and answers through the boundary trait,
      and `tests/lsp_lifecycle/utf16_positions.rs` holds a case for each document handler that reads
      or answers a position
- [ ] `len_utf16` and `encode_utf16` appear in `src/` only in `src/position_encoding.rs`
- [ ] The comment on the color test cases in `src/decorators/color.rs` states that features count
      columns in code points and the handler converts them, and announces no later change
- [ ] The Conventions section of the root `CLAUDE.md` tells an agent that features count columns in
      code points and that handlers convert through `src/position_encoding.rs`

## Decisions

- **Boundary shape:** the handlers convert, and the features count code points only — one place
  per handler to check, and no feature can mix units. The converter records only the astral
  characters and one trait covers every request and response type; the test file with a case per
  handler, the source check, and the `CLAUDE.md` convention keep a new handler from skipping the
  boundary.
- **Code points inside:** `rlsp-yaml-parser` is a published crate whose public `Pos` and `LineIndex`
  count code points and change only for parser bugs (root `CLAUDE.md` Crate Boundaries), and Rust
  slices text by bytes and iterates it by code points; UTF-16 inside would convert at every column
  computation instead of at one boundary.
- **No encoding negotiation:** the server keeps the UTF-16 default and does not accept `utf-32` or
  `utf-8` from clients that offer them — the VS Code client offers only UTF-16, every other mode
  would double the handler tests, and `utf-8` would still need a conversion.
- **One document read per request:** a refactor ahead of the conversion gives every document handler
  what it reads from one function, so the converter is built in one place. The handlers that do not
  copy the text today copy it from then on; measured with release builds on a 1 MB document, that
  copy costs under 1 % of the AST copy those handlers already make.
- **Per request, no cache:** the scan for astral characters costs under 3 % of a request in the same
  measurement; a converter cached per document version adds state to keep in sync for no measurable
  gain.
- **Only astral characters recorded:** a position converts in time logarithmic in the astral
  characters of its line; a converter that walks the line text for each position takes quadratic
  time for the semantic tokens of one long line.
- **Every document handler converts:** range formatting, whose range only gives lines, converts too,
  so the rule holds for every document handler without exceptions to check.
- **Diagnostics:** the server stores diagnostics with internal positions, because code actions
  compare them with spans; publishing them and attaching them to a code action converts them.
- **Edge positions:** a client column inside an astral character converts to the start of that
  character, so a request there finds the character. A column past the line end keeps its excess,
  so round trips are exact, and `u32::MAX` stays `u32::MAX` on the way out, since syntax diagnostics
  end at that column (`src/parser.rs`). LSP 3.17 "Position" leaves a column inside a surrogate pair
  undefined and lets a client read a column past the line end as the line end.
- **Byte order mark:** U+FEFF lies in the BMP, so the converter maps a column after it on line 1
  unchanged; whether a line-1 column counts the mark stays as it is today.
- **Escaped document links:** LSP 3.17 "Document Link Request" makes a link's `range` the range the
  link applies to, but in a single-line quoted scalar with an escape sequence before the URL the
  range starts and ends one column early for each character the escapes save, because the URL's
  offset is measured in the scalar's value, not in its source text. This plan keeps it: the fix maps
  value offsets back through escapes, a change to the document-link feature that the position
  encoding does not touch.
- **Gate demands on Task 1:** the never-opened-URI tests form one table that asserts each handler's
  exact answer, since "no item" would weaken the tests that asserted `null` before; the server-test
  helpers `open` and `apply_action` live once in `tests/lsp_lifecycle/helpers.rs`, as later tasks
  add cases that use them.

## Non-Goals

- Sharing the stored AST between requests instead of copying it for each one (separate plan after
  this one)
- The document-link range of an escaped quoted scalar (Decisions, "Escaped document links";
  `.ai/backlog.md`)
