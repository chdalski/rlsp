**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-07

## Goal

Rewrite the YAML parser as a streaming/incremental
state machine that yields events as they are produced,
delivering O(1) first-event latency for LSP responsiveness.
Currently first-event latency is O(n) — the parser must
buffer the entire input before yielding the first token.
For a 1MB document this means ~3.5 seconds before any
event is available, which makes the LSP feel unresponsive
on large files.

The rewrite is built as a new sibling crate
`rlsp-yaml-parser-temp` that develops in parallel with
the existing parser, then replaces it once all integration
tests pass.

## Context

### Why a from-scratch rewrite

The investigation in the prior session showed the existing
parser (`rlsp-yaml-parser`) is built on a `Box<dyn Fn>` PEG
combinator framework that is **fundamentally
batch-oriented**:

- Every combinator (`alt`, `opt`, `many0`, `many1`,
  `lookahead`) clones `State` for backtracking
- `Reply::Success` carries a `Vec<Token>` of all tokens
  matched so far
- The whole parser tree is rebuilt as `Box<dyn Fn>`
  closures on every parse call
- `tokenize()` produces `Vec<Token>` covering the entire
  input before any event can be emitted
- Two post-parse validation passes (`validate_tokens` in
  `stream.rs`, `validate_input` in `event.rs`) require
  the full token stream

A streaming parser cannot reuse this framework. The
combinator boxes, the State cloning, the Vec<Token>
accumulation, and the post-parse validation all assume
batch operation. Replacing them piecewise would leave the
code in a half-broken state for many task slices.

### Why a separate crate

User decision after weighing trade-offs:

- **Clean slate** — no temptation to subconsciously
  preserve flawed assumptions from the existing
  implementation. The developer writes from references and
  spec, not from "what's already there."
- **Side-by-side testing** — both implementations can run
  against the same integration test suite, including the
  YAML test suite. The new one's correctness is measured
  against the spec AND against the existing parser's
  known-passing tests.
- **Existing parser stays functional** — `rlsp-yaml` (the
  LSP server) keeps working unchanged throughout
  development. No broken intermediate state in CI or
  local dev.
- **Reference-driven development is more natural** —
  When writing fresh code, looking up `s_l_block_node`
  in libfyaml/HsYAML is the obvious first step. When
  "fixing" existing code, the temptation is to grep your
  own codebase first.
- **Cleaner git history** — One commit series adds the
  new crate, one swaps it in.

The migration cost (rename, swap dependency in
`rlsp-yaml`, delete old crate) is small compared to those
benefits.

### Reference implementations are mandatory

Before implementing any YAML production, the developer
**must** consult reference implementations in this order:

1. **`/workspace/rlsp-yaml-parser/src/`** (local) — grep
   for the production name; the existing implementation is
   already in the workspace, has been validated against
   the test suite, and knows about edge cases. Check FIRST.
2. **HsYAML:** `https://raw.githubusercontent.com/haskell-hvr/HsYAML/master/src/Data/YAML/Token.hs`
3. **libfyaml:** `https://raw.githubusercontent.com/pantoniou/libfyaml/master/src/lib/fy-parse.c`

This is a long-standing project rule (the user has
emphasized it 7+ times). The cost of an unnecessary fetch
is seconds; the cost of skipping is hours of trial-and-
error debugging. Each grammar task in this plan includes
this consultation as the first step.

### Architectural approach: line-at-a-time streaming

Per the investigation: YAML's indentation-based grammar
requires knowing the next line's indent to parse the
current line. Most productions need **one line of
lookahead**. The exception is **block scalars with
auto-detected indentation** (`|` or `>` with no explicit
indent digit), which need to scan forward to the first
content line.

The streaming parser uses a **line buffer** abstraction:

- Reads input one line at a time
- Buffers exactly one line ahead so the current line's
  parser can see the next indent
- For block scalars, the line buffer expands locally to
  capture content lines until the indent drops below the
  block scalar's base indent

This bounds lookahead to "one line under normal
conditions, one block-scalar's worth in scalar mode."
First-event latency becomes proportional to the first
line, not the whole document — O(1) for typical use.

### API change: scalars use `Cow<'input, str>`

The existing `Event::Scalar { value: String, ... }` always
allocates. The new parser uses `Cow<'input, str>`:

- Plain scalars, single-quoted scalars without escapes,
  and contiguous double-quoted scalars without escapes →
  `Cow::Borrowed(&'input str)` (zero allocation)
- Double-quoted scalars with escape sequences, folded
  block scalars where line breaks become spaces, and
  literal block scalars where lines are concatenated →
  `Cow::Owned(String)` (one allocation per scalar, not per
  fragment)

Anchors, tags, and aliases always borrow from input as
`&'input str` — they cannot contain escapes.

This is a **breaking API change** but the crate is pre-1.0
with no external consumers beyond `rlsp-yaml` (this repo's
LSP server). `rlsp-yaml` will be updated as part of the
migration task.

### Test acceptance gate: integration tests, not unit tests

Distinction (per user feedback during planning):

- **`src/*.rs` unit tests (945 in existing parser)** — tied
  to internal implementation details (specific combinator
  function names, intermediate state, token codes). The
  new crate gets its own unit tests written fresh as part
  of the implementation work. **Not held to any "must
  match" standard.**
- **`tests/*.rs` integration tests** — implementation-
  agnostic, use only the public API. **These are the
  acceptance gate.** Currently:
  - `conformance.rs` — 351 YAML test suite files
  - `encoding.rs` — 24 byte encoding tests
  - `error_reporting.rs` — 48 error format tests
  - `loader_spans.rs` — 3 span correctness tests
  - `robustness.rs` — large input stress tests
  - `round_trip.rs` — emit/parse roundtrip tests

These tests must pass against the new crate with **only
import path adaptation** — no logic changes. If the new
parser produces different errors or different events for
inputs the old parser handled, that's a bug in the new
parser (modulo intentional error message improvements,
which would be discussed at review time).

### Acceptance criteria

1. **All `tests/*.rs` integration tests pass** against the
   new crate. Specifically:
   - `tests/encoding.rs` — 24 tests pass
   - `tests/error_reporting.rs` — 48 tests pass
   - `tests/loader_spans.rs` — 3 tests pass
   - `tests/robustness.rs` — all tests pass
   - `tests/round_trip.rs` — all tests pass
2. **Conformance pass rate** — at least the same number of
   YAML test suite files pass as the existing parser
   (currently 351/351). The exact list of passing files
   should match.
3. **First-event latency O(1)** — measured by latency
   benchmarks. Target: first-event time for huge_1MB
   fixture < 1 ms (currently 3.498 s, ~3,500,000× speedup).
4. **Migration successful** — `rlsp-yaml-parser-temp`
   replaces `rlsp-yaml-parser`. The `rlsp-yaml` LSP server
   compiles and its existing tests pass.

### Files involved

The new crate `rlsp-yaml-parser-temp/` is created from
scratch. The existing `rlsp-yaml-parser/` is **not
modified during development** — only at migration time.

New files (estimated):
- `rlsp-yaml-parser-temp/Cargo.toml`
- `rlsp-yaml-parser-temp/src/lib.rs`
- `rlsp-yaml-parser-temp/src/pos.rs` — Pos and Span (port)
- `rlsp-yaml-parser-temp/src/chars.rs` — character
  predicates (port)
- `rlsp-yaml-parser-temp/src/lines.rs` — line buffer with
  lookahead (new)
- `rlsp-yaml-parser-temp/src/scanner.rs` — character cursor
  (new)
- `rlsp-yaml-parser-temp/src/lexer.rs` — token state
  machine (new)
- `rlsp-yaml-parser-temp/src/event.rs` — Event types,
  token-to-event conversion (new + ported types)
- `rlsp-yaml-parser-temp/src/loader.rs` — event-to-AST
  loader (port with minimal changes)
- `rlsp-yaml-parser-temp/src/error.rs` — error types
- `rlsp-yaml-parser-temp/tests/` — copied from existing
  crate, import path updated

### Plan size warning

This is the largest plan in the project to date — ~25
tasks across roughly 4× the volume of any prior plan.
Execution will span many developer-reviewer cycles. The
user has explicitly approved this scope.

## Steps

- [x] Investigate combinator architecture and streaming
  feasibility (done in planning)
- [x] Confirm scope and approach with user
- [x] Bootstrap new crate (Task 1) — `8531e28`
- [x] Build line buffer and scanner foundations (Tasks 2-3) — Task 2 `63ea25c`, Task 3 `562b133`
- [x] Implement empty stream and document boundaries (Tasks 4-5) — Task 4 `6d1d315`, Task 5 `494286e`
- [x] Implement plain, quoted, and literal block scalars (Tasks 6-8) — Task 6 `e624786`, Task 7 `c06c0b2`, Task 8 `ddc3038`
- [x] Enable `clippy::panic` and clean up panic sites (Task 9) — `cb55273`
- [x] Implement folded block scalars (Task 10) — `f107749`
- [x] Implement block collections (Tasks 11-13) — Task 11 `93c66e0`, Task 12 `9f4ecb0`, Task 13 `09b5b10`
- [x] Implement flow collections (Tasks 14-15) — Task 14 `82ad832`, Task 15 `a87ed9f`
- [ ] Implement anchors, tags, aliases, comments (Tasks 16-18) — Task 16 `88210c3`, Task 17 `dc01d7f`
- [ ] Implement directives and multi-document (Task 19)
- [ ] Port loader and run integration tests (Tasks 20-21)
- [ ] Run benchmarks, verify O(1) latency (Task 22)
- [ ] Migrate: replace rlsp-yaml-parser (Task 23)

## Tasks

### Task 1: Bootstrap rlsp-yaml-parser-temp crate

Create the new crate skeleton in the workspace.

**Status:** Completed in commit `8531e28`.

- [x] Create `rlsp-yaml-parser-temp/Cargo.toml` with same
  workspace integration as `rlsp-yaml-parser`
- [x] Add `rlsp-yaml-parser-temp` to workspace members in
  root `Cargo.toml`
- [x] Create `rlsp-yaml-parser-temp/src/lib.rs` with empty
  module declarations and a placeholder `parse_events()`
  that returns an empty iterator
- [x] Create empty source files for: `pos.rs`, `chars.rs`,
  `lines.rs`, `scanner.rs`, `lexer.rs`, `event.rs`,
  `loader.rs`, `error.rs`
- [x] `cargo build -p rlsp-yaml-parser-temp` succeeds
- [x] `cargo clippy -p rlsp-yaml-parser-temp --all-targets`
  passes with zero warnings
- [x] Commit message: `feat(parser-temp): bootstrap streaming parser crate skeleton`

**Reference impl consultation:** Not applicable (skeleton only).
**Advisors:** None.

### Task 2: Port Pos, Span, and chars predicates

Port the foundational types from `rlsp-yaml-parser` that
do not depend on the parser architecture.

**Status:** Completed in commit `63ea25c`.

- [x] Port `Pos` struct (byte_offset, char_offset, line,
  column) and its operations to `pos.rs`
- [x] Port `Span` struct to `pos.rs`
- [x] Port character predicate functions from
  `rlsp-yaml-parser/src/chars.rs` to `chars.rs`
- [x] Add unit tests for non-trivial predicates
- [x] Build and clippy clean
- [x] Commit: `feat(parser-temp): port Pos, Span, and character predicates`

**Reference impl consultation:** Local
`rlsp-yaml-parser/src/pos.rs` and `chars.rs`. These are
data types and pure functions — no streaming considerations.
**Advisors:** None (pure refactoring of pure code).

### Task 3: Line buffer with one-line lookahead

Build the streaming line reader. This is the foundation of
the streaming architecture.

**Status:** Completed in commit `562b133`.

- [x] Implement `LineBuffer` struct that wraps an input
  `&str` and yields lines on demand
- [x] Each `Line` carries: byte offset of line start, byte
  range of content (excluding terminator), the line break
  type (`\n`, `\r\n`, `\r`, EOF), and the indent (count of
  leading spaces)
- [x] LineBuffer always has the *next* line buffered if it
  exists, so callers can check the next line's indent
  without consuming it
- [x] Provide `peek_next()`, `peek_next_indent()`,
  `consume_next()`, and `at_eof()` operations.

  Note: the implementation uses a **one-line buffer model**
  rather than a [current, next] two-line model. The buffer
  holds only the *upcoming* line; the lexer will hold the
  "current" line in a local variable after `consume_next()`
  and use `peek_next_indent()` to look at the line after.
  This deviates from the original plan wording
  (`consume_current()`) but is simpler and semantically
  equivalent for the lexer pattern. Lead approved during
  Task 3 review.
- [x] Provide a "scalar mode" or block-scalar peek
  (`peek_until_dedent(base_indent)`) that expands the
  buffer locally to scan forward until a line with indent
  ≤ base — used by block scalar auto-indentation. Blank
  lines are transparent to the scan; trailing blank lines
  are included in the result and the consumer (Task 8
  lexer) is responsible for chomping-based trimming.
- [x] Unit tests covering: empty input, single line, multi
  line, mixed line endings, BOM handling, trailing newline,
  indent calculation, and `pos.line`/`pos.column`
  tracking after each terminator type (regression tests
  for the bare-CR pos bug found in review)
- [x] Build, clippy, tests pass (85/85)
- [x] Commit: `feat(parser-temp): add streaming line buffer with one-line lookahead`

**Reference impl consultation:**
1. Local: check how `structure.rs` and `block.rs` handle
   line breaks and indent counting
2. libfyaml: `fy_reader_*` functions for the streaming
   line reader pattern
3. HsYAML: line handling in `Token.hs`

**Advisors:** test-engineer (new abstraction with no
existing pattern to follow).

### Task 4: Stream events (empty input + EOF)

Wire the line buffer into a minimal event iterator that
yields just `StreamStart` and `StreamEnd`.

**Status:** Completed in commit `6d1d315`.

- [x] Define `Event` enum in `event.rs`. Variants for this
  task: `StreamStart`, `StreamEnd`. Other variants will be
  added in subsequent tasks. Use `Cow<'input, str>` for
  scalar values, `&'input str` for anchors/tags/aliases
  per the API decision in Context.
- [x] Define `Span` carrying input bytes
- [x] Define `Error` type (using `thiserror`)
- [x] Implement `parse_events(input: &str) -> impl Iterator<Item = Result<(Event<'_>, Span), Error>>`
- [x] For empty/whitespace-only input: emit `StreamStart`
  then `StreamEnd`
- [x] Update `lib.rs` exports
- [x] Add an integration test in `tests/smoke.rs`:
  parses empty string, expects two events
- [x] Build, clippy, tests pass
- [x] Commit: `feat(parser-temp): emit StreamStart/StreamEnd for empty input`

**Reference impl consultation:**
1. Local: `rlsp-yaml-parser/src/event.rs` `Event` enum and
   `parse_events()` function
2. HsYAML: `tokenize` function and `Token` data type
3. libfyaml: `fy_parser_parse` and event types

**Advisors:** test-engineer (new public API; defines the
test pattern that subsequent tasks follow).

### Task 5: Document boundaries and bare documents

Add `DocumentStart`/`DocumentEnd` events and handle the
`---`/`...` markers and bare (no-marker) documents.

**Status:** Completed in commit `494286e`.

- [x] Tokenizer recognizes `---` at column 0 followed by
  whitespace/EOL/EOF as a document start marker
- [x] Tokenizer recognizes `...` at column 0 followed by
  whitespace/EOL/EOF as a document end marker
- [x] Emit `DocumentStart { explicit: bool }` and
  `DocumentEnd { explicit: bool }` events
- [x] Handle multi-document streams (e.g.,
  `---\n---\n---\n`)
- [x] Handle bare documents (no markers — explicit=false)
- [x] Handle the `directives + ---` document start
  (directives covered in Task 19, but the parser must not
  reject them here)
- [x] Conformance tests for empty docs and bare doc
  markers must pass; identify which YAML test suite files
  this covers and ensure they pass
- [x] Add unit tests for the new tokenizer states
- [x] Build, clippy, tests pass
- [x] Commit: `feat(parser-temp): document boundaries and bare documents`

**Reference impl consultation:**
1. Local: `rlsp-yaml-parser/src/stream.rs`
   `c_directives_end()`, `c_document_end()`,
   `l_yaml_stream()`, and the corresponding event handling
   in `event.rs`
2. HsYAML and libfyaml document boundary handling

**Advisors:** test-engineer (defines tokenizer state
machine pattern subsequent tasks build on).

### Task 6: Plain scalars

Implement plain scalar tokenization and the `Scalar` event
with `style: ScalarStyle::Plain`.

**Status:** Completed in commit `e624786`.

- [x] Define `ScalarStyle` enum: Plain, SingleQuoted,
  DoubleQuoted, Literal(Chomp), Folded(Chomp)
  (Plain only — other variants deferred to their tasks)
- [x] Tokenizer recognizes plain scalars per YAML 1.2
  productions (`ns-plain-first`, `ns-plain-safe`,
  `ns-plain-char`)
- [x] Distinguish plain scalars from indicators (`:`, `-`,
  `?`, `&`, `*`, `!`, `|`, `>`, `[`, `]`, `{`, `}`, `,`,
  `#`)
- [x] Multi-line plain scalars (line folding rules)
- [x] Plain scalars borrow from input where possible
  (`Cow::Borrowed`); only owned when line folding requires
  building a new string
- [x] Emit `Scalar { value, style: Plain, anchor: None,
  tag: None }` events
- [x] Conformance tests covering plain scalars must pass
- [x] Unit tests for the tokenizer plain-scalar state
- [x] Build, clippy, tests pass
- [x] Commit: `feat(parser-temp): plain scalars`

**Note:** The `Chomp` enum (Strip/Clip/Keep) was originally
listed as a Task 6 sub-task but was deferred to Task 8 with
lead approval — it has no consumer in Task 6 (only
`Literal(Chomp)` and `Folded(Chomp)` use it), so defining
it now would be dead code per YAGNI. Task 8 will introduce
it naturally as part of literal block scalar parsing.

**Reference impl consultation:**
1. Local: `block.rs` and `flow.rs` plain scalar productions
   (`ns_plain`, `ns_plain_one_line`, `ns_plain_multi_line`,
   `ns_plain_safe`, `ns_plain_char`)
2. HsYAML and libfyaml plain scalar handling
3. The local impl has known plain-scalar edge cases —
   read it carefully, especially around `:` and `#` boundaries

**Advisors:** test-engineer (first scalar implementation;
plain scalars have many spec edge cases).

### Task 7: Single-quoted and double-quoted scalars

**Status:** Completed in commit `c06c0b2`.

- [x] Single-quoted scalar tokenization (escape: `''` →
  `'`, otherwise verbatim)
- [x] Double-quoted scalar tokenization with escape
  sequences (`\n`, `\t`, `\\`, `\"`, `\xHH`, `\uHHHH`,
  `\UHHHHHHHH`, etc.)
- [x] Multi-line quoted scalars (line folding rules differ
  between single and double quotes)
- [x] Single-quoted scalars without folding/escapes →
  `Cow::Borrowed`. Otherwise `Cow::Owned`.
- [x] Double-quoted scalars without escapes/folding →
  `Cow::Borrowed`. Otherwise `Cow::Owned`.
- [x] Emit `Scalar` events with appropriate `style`
- [x] Conformance tests for quoted scalars must pass
- [x] Build, clippy, tests pass
- [x] Commit: `feat(parser-temp): single- and double-quoted scalars`

**Reference impl consultation:**
1. Local: `flow.rs` single-quoted (`c_single_quoted`,
   `nb_single_text`) and double-quoted (`c_double_quoted`,
   `nb_double_text`) productions
2. HsYAML and libfyaml escape sequence handling — known
   edge cases around line folding and escaped line breaks

**Advisors:** test-engineer (escape handling has many
edge cases); security-engineer (escape sequence parsing
is a trust boundary — `\xHH`, `\uHHHH`, `\UHHHHHHHH`
need careful bounds checking).

### Task 8: Literal block scalars

**Status:** Completed in commit `ddc3038`.

- [x] Literal block scalar header: `|` followed by
  optional chomp indicator (`+`/`-`) and optional explicit
  indent digit
- [x] Auto-detect indentation when no explicit digit:
  scan forward (using the line buffer's block-scalar peek
  mode) to find the first content line
- [x] Collect content lines preserving newlines
- [x] Apply chomping rules (Strip/Clip/Keep) at end
- [x] Emit `Scalar { value: Cow::Owned(String), style: Literal(chomp), ... }`
- [x] Handle empty block scalars
- [x] Handle block scalars at end of input
- [x] Conformance tests for literal block scalars must pass
- [x] Build, clippy, tests pass
- [x] Commit: `feat(parser-temp): literal block scalars`

**Reference impl consultation:**
1. Local: `block.rs` `c_l_literal()`,
   `s_l_block_literal()`, and the chomping/indent
   detection helpers (`detect_scalar_indentation()` is
   the function the investigation flagged as the
   unbounded-lookahead case)
2. HsYAML and libfyaml block scalar implementation
3. The chomping rules in spec §8.1.1.2 are subtle — read
   the reference impls carefully

**Advisors:** test-engineer (block scalars have many
subtle edge cases — chomping, indent detection, blank
line handling); the local impl has WW2P, M9B4, S98Z and
other test fixture cases that exercise edge cases.

### Task 9: Enable `clippy::panic` and clean up panic sites

**Status:** Completed in commit `cb55273`.

Task 8's review surfaced ~60 `unwrap_or_else(|| panic!("..."))`
call sites in `rlsp-yaml-parser-temp/src/lexer.rs`. These
express invariant assertions in a form that bypasses the
existing `unwrap_used = "deny"` / `expect_used = "deny"`
clippy lints, making them hard to audit: a reviewer can't
tell a legitimate invariant ("peek returned Some but
consume returned None" — a LineBuffer API guarantee) from
a developer dodging the lint without tracing the call site.

**Approach:** add `#![deny(clippy::panic)]` at the top of
`rlsp-yaml-parser-temp/src/lib.rs`. This is a crate-level
Rust attribute that stacks additively with the workspace
`[lints]` inheritance — the existing lint configuration
is preserved, and the new lint is scoped to this crate
only. Other workspace crates (`rlsp-yaml-parser`,
`rlsp-yaml`, `rlsp-fmt`) are unaffected.

The lint catches every literal `panic!` macro invocation,
including `unwrap_or_else(|| panic!(...))` since the
closure body contains `panic!`. The lint becomes the
audit: `cargo clippy` lists exactly which sites need to
be fixed, and the list can never grow undetected.

The crate-level attribute covers both `src/` and the
crate's `tests/` integration tests, so test sites need
conversion too.

**Execution order:** Inserted between Task 8 and the
scalar work of Task 10 so the lint is active before
Tasks 10-21 add more code. This bounds the cleanup scope
to what Tasks 5-8 introduced (~60 sites) rather than
letting the pattern spread.

**Conversion rules:**
- **Invariant assertions** (the majority): `unwrap_or_else(|| panic!("..."))` → `let Some(x) = ... else { unreachable!("...") }`. `unreachable!` is a distinct macro, not caught by `clippy::panic`, and signals "I've proved this can't happen" at the call site.
- **Real error cases:** convert to a `Result` return. No exceptions.
- **Test assertions:** `unwrap_or_else(|| panic!("should parse"))` in unit tests → either `unreachable!("should parse")` or `#[expect(clippy::panic, reason = "test assertion")]` on the test function. Prefer the former.

**Acceptance target:** zero `panic!` macro violations in
`rlsp-yaml-parser-temp` (src and tests) when the
`#![deny(clippy::panic)]` attribute is active.

- [x] Add `#![deny(clippy::panic)]` at the top of `rlsp-yaml-parser-temp/src/lib.rs` (below the SPDX header, above the `mod` declarations)
- [x] Run `cargo clippy -p rlsp-yaml-parser-temp --all-targets` and collect the full violation list
- [x] Convert each violation following the rules above
- [x] Verify `cargo clippy -p rlsp-yaml-parser-temp --all-targets` is clean
- [x] Verify `cargo test -p rlsp-yaml-parser-temp` still passes (all 392+ tests)
- [x] Verify `cargo clippy --workspace --all-targets` is still clean (other crates should be unaffected)
- [x] Verify `cargo test --workspace` passes
- [x] Verify `cargo fmt --check` is clean
- [x] Grep verification: `grep -rn 'unwrap_or_else(|| panic!' rlsp-yaml-parser-temp/` returns zero results

**Reviewer note (resubmission):** Initial submission was rejected for three High findings:
(1) production sites used closure-form `unwrap_or_else(|| unreachable!)` instead of let-else;
(2) test panics in lexer.rs/lines.rs were silenced via module-level `#[allow(clippy::panic)]` rather than converted; (3) the plan's claim that the crate-level attribute reaches integration tests was factually wrong — `tests/smoke.rs` is a separate compilation unit and needed its own `#![deny(clippy::panic)]`. All three findings addressed in the resubmission and committed in `cb55273`. Both `#[allow]` attributes removed; `tests/smoke.rs` now carries `#![deny(clippy::panic)]`; all 17 production sites use let-else form. Test-helper closure sites inside `mod tests` (45 in lexer.rs) were left in `unwrap_or_else(|| unreachable!)` form since they were not flagged by the rejection scope.

**Reference impl consultation:** Not applicable — this is
internal refactoring driven by a lint.

**Advisors:** test-engineer (verify no test regressions
after restructuring); security-engineer (confirm the
remaining panics — which are all `unreachable!` or tests —
cannot be triggered by user input).

### Task 10: Folded block scalars

**Status:** Completed in commit `f107749`.

- [x] Folded block scalar header: `>` with chomp/indent
  modifiers (same as literal)
- [x] Same auto-detect and collection as literal, but
  apply line folding (single newlines become spaces,
  blank lines become newlines, more-indented lines stay
  literal)
- [x] Emit `Scalar { style: Folded(chomp), ... }`
- [x] Conformance tests for folded scalars must pass
- [x] Build, clippy, tests pass
- [x] Commit: `feat(parser-temp): folded block scalars`

**Reference impl consultation:**
1. Local: `block.rs` `c_l_folded()`, `s_l_block_folded()`,
   `b_l_folded()`
2. HsYAML and libfyaml folded scalar implementation

**Advisors:** test-engineer (folding rules are subtle).

### Task 11: Block sequences

**Status:** Completed in commit `93c66e0`.

Implement `- item` block sequences and `SequenceStart`/
`SequenceEnd` events.

- [x] Tokenizer recognizes `-` followed by space/EOL as a
  block sequence entry indicator
- [x] Track indent levels: a block sequence's items must
  have consistent indent
- [x] Emit `SequenceStart`/`SequenceEnd` around the
  sequence's items
- [x] Each sequence item can be any node (scalar, nested
  collection) — scalar and nested-sequence items supported;
  sequence-containing-mapping deferred to Task 13 per plan
  scope
- [x] Empty sequence items (just `-` followed by EOL)
  produce `Scalar { value: "", style: Plain, ... }` (null
  representation per YAML spec)
- [x] Conformance tests for block sequences must pass
- [x] Build, clippy, tests pass
- [x] Commit: `feat(parser-temp): block sequences`

**Reference impl consultation:**
1. Local: `block.rs` `l_block_sequence()`,
   `s_l_block_seq_entry()`, and indent handling
2. HsYAML and libfyaml block sequence implementation

**Advisors:** test-engineer (collections are where
indent-tracking complexity lives).

**Reviewer note (resubmission):** Initial submission was rejected
for a High span-correctness finding: `consume_sequence_dash`
computed synthetic inline-line positions from `line.pos +
offset_from_dash`, omitting the leading-space count that
`trim_start_matches(' ')` skipped to reach the dash. Indented
sequences like `"  - foo\n"` reported the `foo` scalar at byte
2..5 instead of 4..7. Not caught by the initial Group A-I tests,
which asserted event order and scalar values but no spans. A
Medium finding (SequenceStart span also zero-based from
`current_pos` instead of the dash column) and two Lows (unused
return value in `consume_sequence_dash`, handoff factual errors)
were in the same rejection. Resubmission fixed all four:
`peek_sequence_entry` now returns `(dash_indent, dash_pos)` with
the dash's absolute position computed from `line.pos +
leading_spaces`; `consume_sequence_dash` uses `leading_spaces +
offset_from_dash` for the synthetic line's byte/char/column;
`SequenceStart` anchors at `dash_pos`; the function signature
returns `bool`. Group J added 8 span tests locking in correctness
for zero-indent, indented, nested-indented, inline-nested, and
multiline cases for both scalar and SequenceStart spans. All 177
smoke tests pass; committed in `93c66e0`.

### Task 12: Block mappings

**Status:** Completed in commit `9f4ecb0`.

Implement `key: value` block mappings and `MappingStart`/
`MappingEnd` events.

- [x] Tokenizer recognizes the implicit key-value pair
  pattern: a scalar followed by `:` followed by space/EOL
- [x] Tokenizer recognizes the explicit key indicator `?`
  followed by space/EOL (less common but spec-required)
- [x] Emit `MappingStart`/`MappingEnd` events around the
  mapping's entries
- [x] Key-value pairs alternate as Scalar/event pairs
  inside the mapping
- [x] Handle complex keys (any node type as key) — literal
  block scalar keys (`? |\n  ...`) verified
- [x] Handle empty values (`key:` followed by EOL)
- [x] Conformance tests for block mappings must pass —
  deferred to Task 21 (integration test suite); 34 smoke
  tests in `mod mappings` cover the scope documented here
- [x] Build, clippy, tests pass (211 integration + 277
  unit, zero clippy warnings)
- [x] Commit: `feat(parser-temp): block mappings`

**Reference impl consultation:**
1. Local: `block.rs` `l_block_mapping()`,
   `ns_l_block_map_entry()`, `c_l_block_map_explicit_entry()`,
   `ns_l_block_map_implicit_entry()`
2. HsYAML and libfyaml block mapping implementation
3. Complex keys (`?`) are rare but the local impl handles
   them — verify against test fixtures like 6BCT, 6FWR

**Advisors:** test-engineer.

**Reviewer note (resubmission):** Initial submission was rejected
for a High correctness finding: `find_value_indicator_offset` did
not handle the `#`-starts-comment rule (YAML 1.2 §6.6). Inputs
like `"key # comment: value\n"` were parsed as a mapping
{`"key # comment"`: `"value"`} instead of a plain scalar
`"key"` followed by a comment — diverging from the reference
parser. A Medium finding (three phase-management functions
`advance_mapping_to_value` / `advance_mapping_to_key` /
`tick_mapping_phase_after_scalar` with undocumented asymmetric
sequence-handling semantics) and two Lows (dead `let _ = col;`
bindings; pre-existing CLAUDE.md drift flagged for future
cleanup) were in the same rejection. Resubmission fixed all
four: the byte loop in `find_value_indicator_offset` now tracks
`prev_was_space` and returns `None` at an unquoted `#` preceded
by whitespace; `advance_mapping_to_*` both gained doc comments
stating the call-site invariant and `debug_assert!`s enforcing
it; the dead `col` bindings became `_`. Two new tests added to
Group G (`hash_after_space_in_key_terminates_at_comment`,
`hash_immediately_after_word_is_part_of_plain_scalar`) lock in
the disambiguation. Verified against an additional 9-case
probe including tab-prefix, quoted-`#`, and `foo: # comment`
variants. All 211 smoke tests pass; committed in `9f4ecb0`.

### Task 13: Nested block collections

**Status:** Completed in commit `09b5b10`.

- [x] Block sequences can contain block mappings and vice
  versa
- [x] Indent rules across nesting boundaries
- [x] Compact in-line forms (e.g., `- key: value` where
  the mapping starts on the sequence entry line)
- [x] Conformance tests for nested block collections must
  pass — 37 smoke tests in `mod nested_collections` cover
  the scope; yaml-test-suite conformance is wired to
  `rlsp-yaml-parser` (not parser-temp) and is deferred to
  Task 21
- [x] Build, clippy, tests pass (248 smoke + 278 unit,
  zero clippy warnings)
- [x] Commit: `feat(parser-temp): nested block collections`

**Reference impl consultation:**
1. Local: `block.rs` `s_l_block_indented()`,
   `s_l_block_node()`, the recursion between block
   sequence/mapping productions
2. HsYAML and libfyaml nesting handling

**Advisors:** test-engineer (nesting is where most
indent-related conformance failures hide).

**Reviewer note (resubmission):** Task 13 was planned as an
audit-and-strengthen exercise — Task 12's unified `coll_stack`
pre-delivered most nesting combinations as a natural consequence of
its design. The initial submission added 34 `nested_collections`
smoke tests (Groups A–H) covering every cross-type combination from
the test-engineer spec plus three targeted parser fixes for the
YAML §8.2.1 seq-spaces rule: `handle_sequence_entry` opens a new
sequence when the top of the stack is `Mapping(col, Value)` and
`dash_indent >= col` (not just `> col`); `handle_mapping_entry`
closes a same-indent sequence whose immediate parent is a mapping
at that column before processing the new key, transitioning the
parent mapping back to `Key` phase; and a bare `-` followed by a
next line at a deeper indent defers empty-scalar emission so the
indented content becomes the item's block value. A Gap 2
table-driven unit test enforces the
`is_implicit_mapping_line` ↔ `find_value_indicator_offset`
contract at the `consume_mapping_entry` `unreachable!` site. The
initial submission was rejected for a High test-coverage gap: the
lead's review directive required span regression-guards on three
cases "at least" (`- key: value`, `- - key: value`, `key:\n  - item`),
and 4 of the 12 mandated span assertions were present. The
resubmission added three span tests (`sequence_start_span_in_compact_seq_map`,
`seq_of_seq_of_map_spans`, `mapping_and_scalar_spans_with_seq_value`)
locking in the outer `SequenceStart` at `(0,0,1)` for
`- key: value`, all five positions for `- - key: val` (outer
`SeqStart (0,0,1)`, inner `SeqStart (2,2,1)`, `MappingStart (4,4)`,
key `(4,4)`, val `(9,9)`), and the outer `MappingStart (0,0,1)` +
item scalar `(9,4,2)` for `key:\n  - item`. The parser was
empirically correct on all these cases at first submission — the
gap was a regression-guard gap, not a correctness bug. All 248
smoke tests pass; committed in `09b5b10`.

### Task 14: Flow sequences and mappings

**Status:** Completed in commit `82ad832`.

- [x] Flow sequence: `[a, b, c]`
- [x] Flow mapping: `{a: b, c: d}`
- [x] Empty flow collections
- [x] Multi-line flow collections
- [x] Trailing commas (allowed)
- [x] Flow scalars inside flow collections (plain scalars
  have stricter rules in flow context)
- [x] Emit Sequence/Mapping events with nested events for
  items
- [x] Conformance tests for flow collections must pass —
  46 smoke tests in `mod flow_collections` cover the
  scope; yaml-test-suite conformance is wired to
  `rlsp-yaml-parser` (not parser-temp) and is deferred
  to Task 21
- [x] Build, clippy, tests pass (295 smoke + 292 unit,
  zero clippy warnings)
- [x] Commit: `feat(parser-temp): flow sequences and mappings`

**Reference impl consultation:**
1. Local: `flow.rs` — the entire file is flow productions.
   Pay attention to `ns_flow_node()`, `c_flow_sequence()`,
   `c_flow_mapping()`, `ns_flow_seq_entry()`,
   `ns_flow_map_entry()`
2. HsYAML and libfyaml flow handling

**Advisors:** test-engineer.

**Reviewer note (re-resubmission):** Task 14 went through three review
rounds. Round 1 rejected for a High silent-scope-deferral finding: the
plain-scalar fallback silently skipped the reserved c-indicators (`&`,
`*`, `!`, `%`, `@`, backtick) inside flow collections, so `[&x]` parsed
as `SequenceStart, Scalar("x"), SequenceEnd` instead of erroring — a
data-loss bug on valid YAML. Also flagged two Mediums (no multi-line
flow span regression-guard; `handle_flow_collection` is 579 lines with
4× duplicated phase-advance blocks). Round 2 rejected for a new High
introduced by the round-1 fix: the replacement code used
`unreachable!("unhandled flow-context character")` as its fallback, but
the `is_plain_first` gate and the downstream `ns_plain_safe_block`
check in `scan_plain_line_flow` are not the same predicate — any
non-`ns-char` character (C0 controls, DEL, C1 controls, surrogates,
`\uFFFE`/`\uFFFF`) would reach the unreachable and panic the parser.
Proven via an ad-hoc test of `"[\x01]\n"`. Round 3 replaced the
unreachable with an explicit `Err` and added a regression test
(`control_character_in_flow_collection_returns_error`). The round-1
Medium on span regression-guard was addressed by
`scalar_on_continuation_line_has_correct_span` (Group P), which pins
down byte_offset=9, line=3, column=2 for scalar `b` in the multi-line
input `"[\n  a,\n  b\n]\n"` — confirming the `peek_next_line()` natural
Pos path is span-correct across line boundaries. The round-1 Medium on
function size was accepted as a documented trade-off: `FlowFrame` and
`FlowMappingPhase` are local to `handle_flow_collection` and extracting
helper functions would require promoting them to module scope, adding
more surface area than the 4× 6-8 line duplication costs. The flow
parser emits `CollectionStyle::Flow` (not the default `Block`) through
all open/close paths; the unified depth limit is
`coll_stack.len() + flow_stack.len() >= MAX_COLLECTION_DEPTH` with no
separate flow-depth constant, verified by `flow_depth_limit_is_enforced`
and `mixed_block_and_flow_depth_limit_is_enforced`. Block context is
untouched — `scan_plain_line_block` received 0 deletions and all
Task 11/12/13 sequences/mappings/nested_collections tests still pass.
All 295 smoke tests pass; committed in `82ad832`.

### Task 15: Nested flow and block-flow mixing

**Status:** Completed in commit `a87ed9f`.

- [x] Flow collections nested inside flow collections
- [x] Flow collections nested inside block collections
  (block context contains flow nodes as values)
- [x] Block collections cannot appear inside flow context
  (per spec)
- [x] Conformance tests for mixed/nested flow must pass
- [x] Build, clippy, tests pass
- [x] Commit: `feat(parser-temp): nested and mixed flow/block`

**Reference impl consultation:**
1. Local: `flow.rs` and `block.rs` — how they call into
   each other
2. HsYAML and libfyaml context handling

**Advisors:** test-engineer.

### Task 16: Anchors and aliases

**Status:** Completed in commit `88210c3`.

- [x] Anchor token: `&name` before any node
- [x] Alias token: `*name` as a node reference
- [x] Emit `Alias { name }` for alias nodes
- [x] Attach `anchor` field to MappingStart/SequenceStart/
  Scalar events that have an anchor
- [x] Anchor names borrow from input as `&'input str`
- [x] Conformance tests for anchors/aliases must pass
- [x] Build, clippy, tests pass
- [x] Commit: `feat(parser-temp): anchors and aliases`

**Reference impl consultation:**
1. Local: `flow.rs` and `block.rs` anchor/alias handling
   (`c_ns_anchor_property`, `c_ns_alias_node`)
2. HsYAML and libfyaml anchor/alias handling

**Advisors:** test-engineer; security-engineer (alias
expansion is a known DoS vector — billion laughs attack;
but expansion happens in the loader, not the parser, so
this is mostly a reminder for Task 20).

### Task 17: Tags

**Status:** Completed in commit `dc01d7f`.

- [x] Verbatim tag: `!<tag:yaml.org,2002:str>`
- [x] Shorthand tags: `!!str`, `!handle!suffix`
- [x] Non-specific tag: `!`
- [x] Tag attached to node like anchor
- [x] Emit MappingStart/SequenceStart/Scalar events with
  `tag: Some(&'input str)` (or maybe `Cow` if tag handle
  expansion requires it — investigate against the local
  impl)
- [x] Conformance tests for tags must pass
- [x] Build, clippy, tests pass
- [x] Commit: `feat(parser-temp): tags`

**Reference impl consultation:**
1. Local: `flow.rs` tag productions, `event.rs`
   `collect_tag()`
2. HsYAML and libfyaml tag handling

**Advisors:** test-engineer.

### Task 18: Comments

- [ ] Tokenize `#` to end of line as a comment
- [ ] Emit `Comment { text: &'input str }` events at the
  positions where comments appear
- [ ] Comments can appear: between any two events, on
  blank lines, after node values
- [ ] Comments inside flow collections
- [ ] Conformance tests with comments must pass
- [ ] `tests/loader_spans.rs` cares about comment positions
- [ ] Build, clippy, tests pass
- [ ] Commit: `feat(parser-temp): comments`

**Reference impl consultation:**
1. Local: how comments are emitted in `event.rs` and
   handled in `loader.rs` — comment placement matters for
   round-trip and span correctness
2. HsYAML and libfyaml comment handling

**Advisors:** test-engineer (comments are easy to misplace
in the event stream).

### Task 19: Directives and multi-document streams

- [ ] `%YAML 1.2` directive parsing
- [ ] `%TAG !handle! prefix` directive parsing
- [ ] Directive scope per document (resets on `...` or
  end of stream)
- [ ] Tag handle resolution against directive prefixes
- [ ] Multi-document streams with directives between
  documents
- [ ] DocumentStart events carry the directive info
  (version, tag pairs)
- [ ] Conformance tests for directives must pass
- [ ] Build, clippy, tests pass
- [ ] Commit: `feat(parser-temp): directives and tag handles`

**Reference impl consultation:**
1. Local: `structure.rs` `l_directive()`, `event.rs`
   `collect_directives()`
2. HsYAML and libfyaml directive handling

**Advisors:** test-engineer.

### Task 20: Port the loader (event → AST)

The existing loader is already sequential. Port it with
minimal changes — only what's needed to consume the new
crate's `Event` type.

- [ ] Port `node.rs` (Node types, Document) — these are
  data types, port verbatim
- [ ] Port `loader.rs` `LoadState`, `LoaderBuilder`,
  `load()` function
- [ ] Adapt the loader to consume `Iterator<Item = Result<(Event<'_>, Span), Error>>` instead of materializing all events
- [ ] Cow scalar values need to be converted to owned
  strings inside Node (or kept as Cow if we want to
  preserve the borrow further — investigate the right call
  with the local impl)
- [ ] `tests/loader_spans.rs` integration tests pass (3 tests)
- [ ] `tests/round_trip.rs` integration tests pass
- [ ] Build, clippy, tests pass
- [ ] Commit: `feat(parser-temp): port loader and Node types`

**Reference impl consultation:**
1. Local: `loader.rs` and `node.rs` — verbatim port for
   most of it
2. HsYAML loader for sequential consumption pattern

**Advisors:** test-engineer; security-engineer (alias
expansion limits, anchor count limits, nesting depth
limits — the loader enforces resource limits to prevent
DoS attacks like billion laughs; verify these are ported
correctly).

### Task 21: Run full integration test suite

Copy all `tests/*.rs` files from `rlsp-yaml-parser` to
`rlsp-yaml-parser-temp/tests/`, update import paths, and
make them pass.

- [ ] Copy `tests/conformance.rs`,
  `tests/yaml-test-suite/`, `tests/encoding.rs`,
  `tests/error_reporting.rs`, `tests/loader_spans.rs`,
  `tests/robustness.rs`, `tests/round_trip.rs`
- [ ] Update import paths from `rlsp_yaml_parser::` to
  `rlsp_yaml_parser_temp::`
- [ ] Run the suite, get the pass/fail report
- [ ] Fix any failing tests by addressing the underlying
  parser bug (NOT by changing the test)
- [ ] All `tests/encoding.rs` tests pass (24)
- [ ] All `tests/error_reporting.rs` tests pass (48)
- [ ] All `tests/loader_spans.rs` tests pass (3)
- [ ] All `tests/robustness.rs` tests pass
- [ ] All `tests/round_trip.rs` tests pass
- [ ] Conformance pass rate: 351/351 (or matches existing
  parser if it's not 351/351)
- [ ] Build, clippy, tests pass
- [ ] Commit: `feat(parser-temp): full integration test suite passes`

This task may surface gaps in earlier tasks. Each fix
goes into a sub-commit referencing the failing test.

**Reference impl consultation:** Per failure, consult
the local impl for the production that's failing.

**Advisors:** test-engineer (this is the broad
integration test pass).

### Task 22: Run benchmarks and verify O(1) latency

Copy benchmarks from `rlsp-yaml-parser/benches/` to
`rlsp-yaml-parser-temp/benches/`, update them to call the
new crate, and run.

- [ ] Copy `benches/throughput.rs`, `benches/latency.rs`,
  `benches/memory.rs`, `benches/fixtures.rs`
- [ ] Update to call `rlsp_yaml_parser_temp::parse_events`
- [ ] Run all benchmarks
- [ ] Verify acceptance: huge_1MB first-event latency
  < 1 ms (target: O(1), currently 3.498 s in old parser)
- [ ] Document results in
  `rlsp-yaml-parser-temp/docs/benchmarks.md`
- [ ] Build, clippy, tests pass
- [ ] Commit: `docs(parser-temp): benchmark results for streaming parser`

**Reference impl consultation:** Not applicable.
**Advisors:** None.

### Task 23: Migration — replace rlsp-yaml-parser

Final task. Atomic migration in one commit (or one PR)
so CI never sees a broken state.

- [ ] Verify all integration tests still pass on temp crate
- [ ] Verify benchmarks still meet acceptance criteria
- [ ] Delete `rlsp-yaml-parser/` directory entirely
- [ ] Rename `rlsp-yaml-parser-temp/` to `rlsp-yaml-parser/`
- [ ] In the new `rlsp-yaml-parser/Cargo.toml`, change
  `name = "rlsp-yaml-parser-temp"` to
  `name = "rlsp-yaml-parser"`
- [ ] Update workspace `members` in root `Cargo.toml`:
  remove `rlsp-yaml-parser-temp`, keep `rlsp-yaml-parser`
- [ ] Update `rlsp-yaml/Cargo.toml` if it pinned to the
  old version (it depends on `rlsp-yaml-parser`)
- [ ] Update `rlsp-yaml/src/*.rs` for the new public API
  (Cow scalar values, &str anchors/tags). Most callers
  will need minor adjustments.
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets` passes
- [ ] `cargo bench --workspace` runs
- [ ] Commit: `feat(parser): replace PEG parser with streaming implementation`
- [ ] Update `rlsp-yaml-parser/docs/benchmarks.md` to
  reflect the migration

**Reference impl consultation:** Not applicable.
**Advisors:** test-engineer (verify migration completeness);
security-engineer (verify resource limits and DoS
protections survived the rewrite).

## Decisions

- **New sibling crate, not in-place rewrite:** User
  decision after weighing trade-offs. See Context section.
  Clean slate, side-by-side testing, no broken intermediate
  state, naturally enforces reference-driven development.

- **Crate name `rlsp-yaml-parser-temp`:** Clearly
  temporary, descriptive of intent. Renamed at migration
  time.

- **Line-at-a-time streaming with one-line lookahead:**
  Per investigation, YAML's indentation grammar requires
  knowing the next line's indent. One-line lookahead is
  the minimum for general parsing. Block scalars expand
  the buffer locally for auto-indent detection.

- **Cow<'input, str> for scalars, &'input str for
  anchors/tags:** Borrow when possible (zero allocation),
  own only when escapes/folding require building a new
  string. Anchors and tags can never contain escapes, so
  they always borrow.

- **Breaking API change is acceptable:** The crate is
  pre-1.0 with no external consumers. The LSP server
  (`rlsp-yaml`) is updated as part of the migration task.

- **Integration tests in `tests/*.rs` are the acceptance
  gate, not unit tests in `src/`:** User clarification
  during planning. Unit tests are tied to internal
  structure and will be written fresh; integration tests
  use the public API and should pass with only import
  path adaptation.

- **Reference implementation consultation is mandatory
  for every grammar task:** Local first
  (`rlsp-yaml-parser/src/`), then HsYAML, then libfyaml.
  This is a long-standing project rule (user repeated 7+
  times). Each grammar task explicitly lists the
  references to consult.

- **No advisor consultation for non-grammar tasks (1-3):**
  Bootstrap, type ports, and infrastructure are pure
  refactoring or new pattern-establishing work where TE
  consultation has limited value. Tasks 4 onwards (where
  the public API and tokenizer state machine emerge)
  consult the test-engineer for design guidance.

- **Security advisor consulted for:** Task 7 (escape
  sequence parsing — `\xHH`/`\uHHHH`/`\UHHHHHHHH` need
  bounds checking), Task 9 (audit panic sites after
  Task 8 surfaced the `unwrap_or_else(|| panic!)` pattern),
  Task 20 (loader resource limits and DoS protections),
  Task 23 (verify limits survived migration). The actual
  parser is pure and free of trust boundaries — the
  security concerns are around resource exhaustion
  attacks at the loader and escape sequence validation in
  the lexer.

- **Migration is one atomic commit:** Task 23 swaps the
  crate name and deletes the old one in one go. This
  avoids any window where CI sees both crates or a half-
  migrated state.

- **Panic-site cleanup executed early (Task 9):** Task 8
  surfaced ~60 `unwrap_or_else(|| panic!("..."))` sites in
  the lexer that express invariant assertions in a clippy-
  lint-bypass form. Originally planned as a pre-migration
  cleanup, this was moved to Task 9 (immediately after
  Task 8) so the lint is active before Tasks 10-22 add
  more code — bounding the cleanup scope and preventing
  regression. The approach is a crate-scoped
  `#![deny(clippy::panic)]` attribute at the top of
  `rlsp-yaml-parser-temp/src/lib.rs`, which stacks
  additively with workspace lint inheritance without
  affecting other crates. Conversion rules: invariant
  assertions → `let...else { unreachable!() }`, real
  errors → `Result` return, test assertions →
  `unreachable!` or `#[expect(clippy::panic, reason)]`.

- **Inline-after-marker scalar gap:** Task 5's
  `inline_scalar` slot only supports plain scalars.
  Quoted (Task 7), literal/folded (Tasks 8/10), and flow
  (Task 14) scalars after `--- ` currently fall back to
  the plain scalar path, producing wrong events. Tracked
  for Task 13 or a dedicated cleanup task; Task 21's
  conformance run will flag any regressions. Filed as a
  TODO in `consume_marker_line`.
