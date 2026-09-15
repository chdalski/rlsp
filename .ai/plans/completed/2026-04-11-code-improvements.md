**Repository:** root
**Status:** Completed (2026-04-11)
**Created:** 2026-04-11

# Code quality improvements for rlsp-yaml-parser

## Goal

Address the eight items catalogued in `rlsp-yaml-parser/code-improvements.md`: remove dead code and duplication in the character-predicates module; colocate tests with the lexer submodules they cover and add missing coverage for `comment.rs`; split the monolithic `lib.rs` (4,628 lines) and `loader.rs` (985 lines) into cohesive smaller files; consolidate `EventIter`'s boolean state into enums so invalid states are unrepresentable; rewrite the stale parser README and retrofit an AI-authorship note across all crate READMEs; and strip orphaned historical content from `docs/benchmarks.md`. The work is decomposed into 30 small, independently-committable tasks ordered so later tasks build on earlier ones.

## Context

### Origin

The user supplied an initial code-improvements checklist as a scratchpad during the 2026-04-11 clarification session. This plan records the verification findings, decomposition decisions, and execution ordering that came out of that discussion — the original checklist is not committed to the repo (user preference; the plan is the single source of truth).

### Specifications and reference implementations

- [YAML 1.2 specification](https://yaml.org/spec/1.2.2/) — authoritative grammar. Productions [2], [3], [22]-[27], [31]-[40], [102] are the character predicates touched by Task 1. §6.8.1 (production [38] `ns-uri-char`) is relevant to Task 27 (spec tightening).
- [libfyaml](https://github.com/pantoniou/libfyaml) — reference C parser used as the performance baseline in `docs/benchmarks.md` and the only comparison retained after Task 27.
- [clippy::struct_excessive_bools](https://rust-lang.github.io/rust-clippy/master/index.html#struct_excessive_bools) — the lint Task 5 removes the allow for. Default threshold is 3 bools per struct; `EventIter` currently has 5. Clippy's recommended fix is refactoring boolean state into enums so invalid states are unrepresentable.
- [YAML Test Suite](https://github.com/yaml/yaml-test-suite) — conformance suite used to verify parser correctness; exercised by `tests/conformance.rs`.

### Key verification findings from clarification

**chars.rs (improvement #1):** 17 of 22 character-class predicates are unused outside their own tests. Analysis revealed five distinct reasons:

- **Architecture-redundant** (5 items) — `is_b_line_feed`, `is_b_carriage_return`, `is_b_char`, `is_nb_char`, `is_c_byte_order_mark`. Always constant inside a `Line<'_>` because `lines.rs` splits input at line terminators and strips the UTF-8 BOM at `lines.rs:116-117` before the lexer sees anything; UTF-16/32 BOM handling lives in `encoding.rs`.
- **Stdlib wrappers** (4 items) — `is_ns_dec_digit`, `is_ns_hex_digit`, `is_ns_ascii_letter`, `is_ns_word_char`. Each is a one-line forward to a `char::is_ascii_*` stdlib method.
- **Trivially inline** (3 items) — `is_s_space`, `is_s_tab`, `is_s_white`. Replaced by `ch == ' '` / `matches!(ch, ' ' | '\t')` at call sites; `plain.rs:556` already inlines the equivalent.
- **Genuinely unused** (1 item) — `is_nb_json`. JSON-compatible subset used in spec productions [107]-[110] for flow scalars; our flow scalar path doesn't carve out a JSON-compat subset.
- **Duplicated** (3 items) — `is_c_indicator`, `is_ns_char`, `is_ns_tag_char_single`. Copy-pasted as local predicates:
  - `is_c_indicator` duplicated at `plain.rs:530-552`
  - `is_ns_char` duplicated at `plain.rs:555-564`
  - `is_ns_tag_char_single` duplicated at `lib.rs:1444-1466` as `is_tag_char`, called by `scan_tag_suffix` and `scan_tag`
  
  All three copies are byte-for-byte identical to the chars.rs definitions. Task 1 keeps the chars.rs versions as the single source and deletes the duplicates, re-routing the three call sites to import from chars.rs.

**Spec-conformance gap found during analysis:** `is_ns_uri_char_single` (production [38]) is genuinely unused, but its absence reveals a spec gap in `scan_tag` (`lib.rs:1355-1363`): the verbatim-tag URI path only rejects control characters (`< '\x20' || == '\x7F'`) and accepts spaces, flow indicators like `{` `}`, non-ASCII characters, and other non-URI symbols that YAML 1.2 §6.8.1 forbids. This is a trust-boundary issue because verbatim tag URIs come from untrusted input and flow to the loader as `tag_handles` keys. Task 27 (Task C2) wires `is_ns_uri_char_single` into `scan_tag` to close the gap; it is deferred to the end of the plan because it's a behaviour change that can reject input other parsers accept and needs security + test advisor consultation.

**lexer/comment.rs has zero unit-test coverage.** Single-method file: `Lexer::try_consume_comment`. No `#[cfg(test)] mod tests`. Called only from the event iterator at `lib.rs:1584` and `lib.rs:1782`; `tests/smoke.rs` exercises it indirectly but no dedicated unit-level contract tests exist. Task 3d creates the missing test module.

**lexer.rs test groups map cleanly to submodules.** All `try_consume_*` scalar methods live in their respective submodules:

- `try_consume_plain_scalar` — `lexer/plain.rs:30`
- `try_consume_single_quoted` — `lexer/quoted.rs:26`
- `try_consume_double_quoted` — `lexer/quoted.rs:189`
- `try_consume_literal_block_scalar` — `lexer/block.rs:37`
- `try_consume_folded_block_scalar` — `lexer/block.rs:260`

The lexer.rs test module (lines 530-2354) currently contains ~1,500 lines of tests targeting these submodule methods. The driver tests (Groups A-F: `is_directives_end`, `is_document_end`, `skip_empty_lines`, `consume_marker_line`, `has_content`, `is_blank_or_comment`) test helpers defined in lexer.rs itself and stay put. Everything else moves.

**lib.rs is 4,628 lines.** Dominated by two methods: `handle_flow_collection` (~1,310 lines) and `step_in_document` (~740 lines). **`handle_flow_collection` has an in-code design note at `lib.rs:3245-3253`** explicitly stating the function is deliberately not broken up:

> Four sites below repeat the same `if let Some(frame) = flow_stack.last_mut() { ... }` shape. Extracting a helper function would require moving `FlowFrame` and `FlowMappingPhase` to module scope — adding module-level types whose sole purpose is to enable this refactor adds more complexity than the duplication costs. Each site is 6-8 lines and clearly labelled by its comment; **the repetition is intentional and stable.**

The function declares a local `FlowFrame` enum at `lib.rs:3204-3243` that would need to be promoted to module scope to support any helper-extraction refactor. **This plan keeps `event_iter/flow.rs` as one file at ~1,310 lines** (the largest file in the final layout). Any future attempt to split `handle_flow_collection` must override this design note explicitly and goes through its own separate plan.

**`EventIter` has 5 booleans**, triggering `#[allow(clippy::struct_excessive_bools)]` at `lib.rs:315`. Three are structural invalid-state smells:

- `pending_anchor` + `pending_anchor_for_collection`: the `_for_collection` flag is undefined when `pending_anchor` is `None`. An enum `PendingAnchor<'input>` with `Standalone(&'input str)` and `Inline(&'input str)` variants makes the disposition inseparable from the anchor.
- `pending_tag` + `pending_tag_for_collection`: byte-for-byte parallel to the anchor case.
- `failed` + `IterState::Done`: both terminate iteration. Folding `failed` into `state = IterState::Done` after an error yield eliminates the redundancy.

After Tasks 5a-5c the struct has 2 booleans (`root_node_emitted`, `explicit_key_pending`) which are below clippy's default threshold of 3. The allow attribute is removed in Task 5c.

**loader.rs is 985 lines** with clear section headers. `LoadState` and its methods (`parse_node`, `register_anchor`, `resolve_alias`, `expand_node`) are tightly coupled and will not be fragmented. Only three self-contained helper sections — stream helpers, `reloc`, comment attachment — are extracted in Tasks 23-25.

**`rlsp-yaml-parser/README.md` does not exist.** It was created on 2026-04-05 (commit `7a32bac`) and removed during the streaming rewrite (commit `cc5c9a5`) because its content — parser combinator architecture, 100% conformance claim tied to the PEG parser, `emitter` / `schema` / `stream` module references — no longer applied. The removed README is preserved at commit `560230d` and serves as the structural starting point for Task 26. The current streaming parser's public API is verified as of this plan's creation date: `parse_events`, `load`, `Loader`, `LoaderBuilder`, `LoaderOptions`, `LoadMode`, and the `encoding`, `loader`, `node` modules are re-exported from `lib.rs`. The `emitter`, `schema`, and `stream` modules are gone.

**`docs/benchmarks.md` has three historical layers**, all originating from completed plans:

- PEG-parser comparisons (Task 22 of `2026-04-07-streaming-parser-rewrite.md`, commit `2ca0ba4`) — acceptance-criterion proof for the O(1) first-event latency target
- "Lazy Pos optimization results" (from `2026-04-10-unicode-position-safety-and-lazy-pos.md`, commit `ea47bb9`) — frozen before/after tables for the char_offset removal
- "Byte-level scanning optimization results" (from `2026-04-10-byte-level-scanning-and-memchr.md`, commits `c6c56ba` / `815d7c5` / `cf772a9`) — frozen before/after tables for memchr scanning

All three layers are preserved in their respective plan files and commit messages. The live benchmarks doc will become a snapshot of current streaming-parser performance vs libfyaml only.

### Execution protocol — pause between tasks

**Per user directive: pause after every task is reviewer-approved and wait for user confirmation before dispatching the next.** The lead cycles the team between tasks and reports task results (commit SHA, relevant findings), but does not auto-dispatch the next task. The user may opt out of pausing for the remainder of the plan with "auto go on" or equivalent; see `feedback_pause_between_tasks.md`.

## Steps

- [x] #1 — chars.rs dead-code removal + de-duplication + spec tightening (Tasks 1, 27) — Task 1 done (17abda2), Task 27 done (ad790db)
- [x] #2 — lexer.rs `is_directive_or_blank_or_comment` test-helper move (Task 2) — 4c9428f
- [x] #3 — lexer.rs test migration to submodules + comment.rs test creation (Tasks 3-6) — Task 3 done (2e49640), Task 4 done (cd8937c), Task 5 done (4b37665), Task 6 done (082c565)
- [x] #6 — loader.rs helper extraction + unit tests (Tasks 7-9, 7b-9b) — Task 7 done (2ac29d0), Task 8 done (3e1ff8a), Task 9 done (c835896), Task 7b done (617519a), Task 8b done (a330847), Task 9b done (84d789d)
- [x] #4a — lib.rs support module extraction (Tasks 10-14) — Task 10 done (769b1dc), Task 11 done (7a7127e), Task 12 done (7b04cd0), Task 13 done (b171ce1), Task 14 done (69596e2)
- [x] #5 — EventIter boolean consolidation (Tasks 15-17) — Task 15 done (c5913f1), Task 16 done (5b316fc), Task 17 done (fd183ab)
- [x] #4b — lib.rs `event_iter/` submodule split (Tasks 18-23) — Task 18 done (9555145), Task 19 done (56a603a), Task 20 done (d6170c4), Task 21 done (d1f0e10), Task 22 done (a7657ab), Task 23 done (8705972)
- [x] #4c — relocate single-consumer support modules into `event_iter/` (Tasks 23b-23g) — added 2026-04-11 after Task 23, see Decisions; Task 23b done (4316828), Task 23c done (94721e4), Task 23d done (01a4f3d), Task 23e done (5ad49a1), Task 23f done (b66b26a), Task 23g done (9a48d38)
- [x] #8 — docs/benchmarks.md historical cleanup (Task 24) — 73c3371
- [x] #7 — parser README rewrite + cross-crate AI Note retrofit (Tasks 25-26) — Task 25 done (cdd1a18), Task 26 done (0bf9706)
- [x] #27 — chars.rs verbatim-tag URI validation tightening (Task 27) — ad790db

## Tasks

### Task 1: chars.rs dead-code removal and de-duplication (#1-C1) — 17abda2

Delete 13 unused YAML 1.2 character predicates from `src/chars.rs`, consolidate three that are duplicated across files back into a single chars.rs home, and remove the crate-level `#![allow(dead_code)]` attribute. `is_ns_uri_char_single` is kept (Task 27 wires it into `scan_tag`). Pure refactor, no behaviour change.

**Files:** `src/chars.rs`, `src/lexer/plain.rs`, `src/lib.rs`

- [x] Delete from `src/chars.rs`: `is_nb_json`, `is_c_byte_order_mark`, `is_b_line_feed`, `is_b_carriage_return`, `is_b_char`, `is_nb_char`, `is_s_space`, `is_s_tab`, `is_s_white`, `is_ns_dec_digit`, `is_ns_hex_digit`, `is_ns_ascii_letter`, `is_ns_word_char` (13 predicates)
- [x] Delete the `#[cfg(test)]` tests for each removed predicate
- [x] Remove the local copy of `is_c_indicator` from `src/lexer/plain.rs:530-552`; import `crate::chars::is_c_indicator` instead
- [x] Remove the local copy of `is_ns_char` from `src/lexer/plain.rs:555-564`; export `chars::is_ns_char` as `pub use plain::is_ns_char` at `lexer.rs:20` remains intact — update to re-export from `chars` instead, OR have `plain.rs` re-export its imported version
- [x] Remove the local copy of `is_tag_char` from `src/lib.rs:1444-1466`; update `scan_tag_suffix` and `scan_tag` call sites to call `crate::chars::is_ns_tag_char_single` instead
- [x] Remove `// Functions defined here will be used by scanner/lexer in later tasks.` comment and `#![allow(dead_code)]` attribute from top of `src/chars.rs`
- [x] `cargo fmt`, `cargo clippy --all-targets` zero warnings, `cargo test` all green
- [x] **Advisors:** none — pure deletion/inlining, low risk, low uncertainty

### Task 2: move `is_directive_or_blank_or_comment` into lexer.rs test module (#2) — 4c9428f

The helper `is_directive_or_blank_or_comment` at `src/lexer.rs:469-476` is gated with `#[cfg(test)]` and defined outside the test module but is only called from one test at line 724 inside `mod tests`. Move it into the test module and drop the `#[cfg(test)]` attribute (implicit inside the test module).

**Files:** `src/lexer.rs`

- [x] Move lines 461-476 (doc comment + `#[cfg(test)]` + function body) into `mod tests` at line 530
- [x] Remove the `#[cfg(test)]` attribute (now implicit)
- [x] Update the doc comment — the "Used only in tests..." line becomes self-evident from location and can be trimmed
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors:** none — mechanical move

### Task 3: migrate plain-scalar tests from lexer.rs to lexer/plain.rs (#3a) — 2e49640

Migrate all test groups that exercise `try_consume_plain_scalar`, `scan_plain_line_block`, and `scan_plain_line_flow` into `lexer/plain.rs`'s existing `mod tests` at line 599. Pure test move; no logic changes.

**Files:** `src/lexer.rs`, `src/lexer/plain.rs`

- [x] Move Group G (`try_consume_plain_scalar`, Task 6) from `src/lexer.rs:729` onwards through the TE-addition subgroups targeting plain-scalar behaviour (colon termination, hash with tab, multi-line folding, c-forbidden disambiguation, indicator chars, span byte offsets)
- [x] Move the "TE required" groups at `src/lexer.rs:1106`, `1122`, `1140` that target plain-scalar behaviour
- [x] Move Group SPF (`scan_plain_line_flow`, 14 tests) from `src/lexer.rs:2268-2353`
- [x] Append to `lexer/plain.rs:599 mod tests` — verify `use super::*;` covers the test dependencies
- [x] If any test uses a helper that's private to `lexer.rs`, either promote the helper to `pub(super)` or keep that specific test in `lexer.rs` with a comment explaining why
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` — same test count as before, just relocated
- [x] **Advisors:** none — pure test move, existing tests unchanged

### Task 4: migrate quoted-scalar tests from lexer.rs to lexer/quoted.rs (#3b) — cd8937c

Migrate all test groups for `try_consume_single_quoted` and `try_consume_double_quoted` into `lexer/quoted.rs`'s existing `mod tests` at line 756. Pure test move.

**Files:** `src/lexer.rs`, `src/lexer/quoted.rs`

- [x] Move Group H (`try_consume_single_quoted`, Task 7) from `src/lexer.rs:1184` through subgroups H-A (happy path), H-B (Cow allocation), H-C (multi-line folding), H-D (error cases)
- [x] Move Group I (`try_consume_double_quoted`, Task 7) from `src/lexer.rs:1343` through subgroups I-E (happy path), I-F (hex/unicode escapes), I-G (line continuation and folding), I-H (Cow allocation), I-I (security controls I-22 through I-25)
- [x] Append to `lexer/quoted.rs:756 mod tests` — verify `use super::*;` covers dependencies
- [x] Same helper-visibility check as Task 3
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors:** none — pure test move

### Task 5: migrate block-scalar tests and create test module in lexer/block.rs (#3c) — 4b37665

Migrate all test groups for `try_consume_literal_block_scalar` (Task 8) from `src/lexer.rs:1676` through subgroups H-A through H-H into `lexer/block.rs`. **The block.rs file currently has no `mod tests`** — this task creates it.

**Files:** `src/lexer.rs`, `src/lexer/block.rs`

- [x] Create `#[cfg(test)] mod tests { use super::*; ... }` at the bottom of `lexer/block.rs`
- [x] Move Group H (literal block scalar, Task 8) and its subgroups H-A through H-H (header parsing happy path/errors, clip content collection, strip/keep chomping, explicit indent indicator, termination/boundary, tab handling, UTF-8 and special content)
- [x] Helper-visibility check
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors:** none — pure test move + new test module scaffolding

### Task 6: add unit tests for lexer/comment.rs (#3d — new) — 082c565

`lexer/comment.rs` is a single-method file (`Lexer::try_consume_comment`) with no unit-test coverage. This task adds a `#[cfg(test)] mod tests` module with unit tests covering the method's documented contract. This is NOT a test migration — these are new tests for previously-untested code.

**Files:** `src/lexer/comment.rs`

- [x] Create `#[cfg(test)] mod tests { use super::*; ... }` in `lexer/comment.rs`
- [x] Happy-path coverage: simple `# hello` comment → text + span; indented comment with leading spaces/tabs; empty comment body (`#` alone); comment with leading whitespace after `#` preserved per doc; multi-byte UTF-8 in comment body
- [x] `None` cases: empty input; blank line; content line like `key: value`; directive line `%YAML 1.2`
- [x] Span correctness: `hash_pos` byte offset, line, column accurate including after leading whitespace; span end is after the last content char, not after newline
- [x] Error path: body exceeds `max_comment_len` → `Err` with `hash_pos`
- [x] State effect: successful consume advances `current_pos` past the line (verify via subsequent `peek_next()`)
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors required:**
  - **test-engineer input gate:** consult before implementing — task introduces a new test file for a previously-untested module (triggers the risk-assessment rule on both "new test file establishes testing pattern" and "modified code has no existing test coverage")
  - **test-engineer output gate:** get sign-off on the completed test list before submitting to reviewer

### Task 7: extract loader/stream.rs (#6) — 2ac29d0

Move the four stream-helper functions from `src/loader.rs` into a new `src/loader/stream.rs` submodule. None of these functions touch `LoadState` — they take `EventStream` references. Pure move.

**Files:** `src/loader.rs`, `src/loader/stream.rs` (new)

- [x] Create `src/loader/stream.rs`
- [x] Move `next_from` (`loader.rs:666`), `consume_leading_doc_comments` (`loader.rs:689`), `consume_leading_comments` (`loader.rs:711`), `peek_trailing_comment` (`loader.rs:730`)
- [x] Add `mod stream;` declaration in `loader.rs`; update internal call sites to use `stream::fn_name` or bring into scope with `use`
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors:** none — pure move

### Task 8: extract loader/reloc.rs (#6) — 3e1ff8a

Move the `reloc` function from `src/loader.rs:778` into a new `src/loader/reloc.rs` submodule. Takes `Node<Span>` and `Span`, no `LoadState` dependency. Pure move.

**Files:** `src/loader.rs`, `src/loader/reloc.rs` (new)

- [x] Create `src/loader/reloc.rs`
- [x] Move `reloc` (`loader.rs:778-840`)
- [x] Add `mod reloc;` declaration in `loader.rs`; update call sites
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors:** none

### Task 9: extract loader/comments.rs (#6) — c835896

Move the comment-attachment helpers from `src/loader.rs` into a new `src/loader/comments.rs` submodule. Both functions take `&mut Node<Span>` and `String`/`Vec<String>`; no `LoadState` dependency. Pure move.

**Files:** `src/loader.rs`, `src/loader/comments.rs` (new)

- [x] Create `src/loader/comments.rs`
- [x] Move `attach_leading_comments` (`loader.rs:846`), `attach_trailing_comment` (`loader.rs:869`)
- [x] Add `mod comments;` declaration in `loader.rs`; update call sites
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors:** none

### Task 7b: add unit tests for loader/stream.rs (#6 — follow-up) — 617519a

Create `#[cfg(test)] mod tests` in `src/loader/stream.rs` with unit-level coverage for the four functions extracted in Task 7 (`next_from`, `consume_leading_doc_comments`, `consume_leading_comments`, `peek_trailing_comment`). Mirrors the Task 6 precedent (first unit tests for `lexer/comment.rs`). Added mid-plan after the user observed that Task 7 extracted functions whose direct unit-test coverage was nil — they were only exercised indirectly through `tests/loader.rs` and `tests/smoke.rs`.

**Files:** `src/loader/stream.rs`

- [x] Create `#[cfg(test)] mod tests { use super::*; ... }` at the bottom of `src/loader/stream.rs`
- [x] Coverage for `next_from`: forwards `Some(Ok(event, span))`, propagates `Some(Err(e))` as `LoadError::Parse`, returns `Ok(None)` on `None`
- [x] Coverage for `consume_leading_doc_comments`: accumulates comment-text events into the caller's Vec until a non-comment event is peeked (without consuming the non-comment); empty case (first event is non-comment)
- [x] Coverage for `consume_leading_comments`: same accumulation pattern for in-doc context
- [x] Coverage for `peek_trailing_comment`: respects the `value_end_line` cutoff — returns `Some(text)` when the next comment is on the same line as `value_end_line`, returns `None` when the comment is on a later line, returns `None` when the next event is not a comment
- [x] Test helper: build a mock `EventStream` from a `Vec<Result<(Event, Span), Error>>` or parse a small YAML string to get a real stream — the TE will recommend the pattern at the input gate
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` — new tests green, existing tests unaffected
- [x] **Advisors required:**
  - **test-engineer input gate:** consult before implementing — this is a new test file for a previously-untested module, triggering both "new test file establishes testing pattern" and "modified code has no existing test coverage" in the risk-assessment rule
  - **test-engineer output gate:** get explicit sign-off on the completed test list before submitting to reviewer
- [x] **Submission:** developer's handoff message to reviewer must cite both advisor gates explicitly; reviewer rejects if either citation is missing (per Task 6 precedent)

### Task 8b: add unit tests for loader/reloc.rs (#6 — follow-up) — a330847

Create `#[cfg(test)] mod tests` in `src/loader/reloc.rs` with unit-level coverage for `reloc`. Easiest of the three follow-up tasks because `reloc` is a pure function over `Node<Span>`.

**Note during review:** the plan's original description claimed `reloc` recursively rewrites child spans. This was incorrect — `reloc` is shallow: it only replaces the top-level `loc` (see `loader.rs:619` — the single call site re-stamps an expanded alias target with the alias-site's location while leaving descendant spans at their original definition sites). The developer tested actual behavior and added `reloc_mapping_with_entries_only_replaces_top_loc` to document the shallow semantics.

**Files:** `src/loader/reloc.rs`

- [x] Create `#[cfg(test)] mod tests { use super::*; ... }` at the bottom of `src/loader/reloc.rs`
- [x] Coverage for scalar relocation: `reloc(Node::Scalar { span, ... }, new_span)` replaces `span` with `new_span`
- [x] Coverage for sequence relocation: top-level sequence span rewritten (shallow — child spans preserved, per actual semantics)
- [x] Coverage for mapping relocation: top-level mapping span rewritten (shallow — key/value spans preserved, per actual semantics)
- [x] Coverage for shallow semantics: mapping with scalar entries — top-level `loc` rewritten, child `loc` unchanged (`reloc_mapping_with_entries_only_replaces_top_loc`)
- [x] Coverage for alias relocation: `Node::Alias` span rewritten
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` — new tests green (432 → 439 unit tests)
- [x] **Advisors required:** test-engineer input + output gates (same rationale as 7b; new test file, no existing coverage) — both gates satisfied

### Task 9b: add unit tests for loader/comments.rs (#6 — follow-up) — 84d789d

Create `#[cfg(test)] mod tests` in `src/loader/comments.rs` (file created in Task 9) with unit-level coverage for `attach_leading_comments` and `attach_trailing_comment`. Must execute after Task 9 because the file does not exist yet.

**Note during review:** the plan's original description said `attach_leading_comments` should "append" to existing comments. This was incorrect — the actual implementation *overwrites* on assign (`*leading_comments = comments;` at `loader/comments.rs:24`). Per plan directive ("verify against current behaviour, don't change it"), the developer tested the real overwrite semantics — same correction pattern as Task 8b's `reloc` shallow-semantics note.

**Files:** `src/loader/comments.rs`

- [x] Create `#[cfg(test)] mod tests { use super::*; ... }` at the bottom of `src/loader/comments.rs`
- [x] Coverage for `attach_leading_comments`: empty input list is a no-op; non-empty list overwrites the node's existing leading comments (actual semantics, not append); verify ordering is preserved
- [x] Coverage for `attach_trailing_comment`: attaches a single trailing comment to the node; overwrite semantics verified against current behaviour
- [x] Verify the helpers reach into the correct `Node` variants — all four (Scalar/Sequence/Mapping/Alias) are supported and covered
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` — new tests green (439 → 450 unit, 368/368 conformance)
- [x] **Advisors required:** test-engineer input + output gates — both gates satisfied

### Task 10: extract lib.rs security-limit constants into limits.rs (#4a-i) — 769b1dc

Move all `MAX_*` constants and their doc comments from the "Security Limits" section of `src/lib.rs` (lines 44-129) into a new `src/limits.rs` module. Update lib.rs imports to re-export or bring them into scope.

**Files:** `src/lib.rs`, `src/limits.rs` (new)

- [x] Create `src/limits.rs`
- [x] Move `MAX_COLLECTION_DEPTH`, `MAX_ANCHOR_NAME_BYTES`, `MAX_TAG_LEN`, `MAX_COMMENT_LEN`, `MAX_DIRECTIVES_PER_DOC`, `MAX_TAG_HANDLE_BYTES`, `MAX_RESOLVED_TAG_LEN` (verified: code uses `MAX_TAG_HANDLE_BYTES`, not `MAX_TAG_HANDLE_LEN` as plan originally said) with their full doc comments; intra-doc links rewritten to `crate::` paths inside the new module
- [x] Add `pub mod limits;` declaration in `lib.rs` and `pub use limits::{...}` re-export to preserve the public API; no internal references needed changes because the re-export keeps all identifiers visible at the crate root
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` — all green; conformance 368/368
- [x] **Advisors:** none — pure move

### Task 11: extract DirectiveScope into directive_scope.rs (#4a-ii) — 7a7127e

Move the `DirectiveScope` struct (`src/lib.rs:142`) and its impl block (`lib.rs:153-236`, containing `resolve_tag` and `tag_directives`) into a new `src/directive_scope.rs` module.

**Files:** `src/lib.rs`, `src/directive_scope.rs` (new)

- [x] Create `src/directive_scope.rs`
- [x] Move `DirectiveScope` struct definition + full `impl DirectiveScope` block
- [x] Add `mod directive_scope;` declaration in `lib.rs`; update `EventIter`'s field declaration to use `directive_scope::DirectiveScope`
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors:** none

### Task 12: extract state-machine enums into state.rs (#4a-iii) — 7b04cd0

Move the state-machine type definitions from `src/lib.rs:242-401` into a new `src/state.rs` module: `StepResult`, `IterState`, `MappingPhase`, `CollectionEntry` + `impl CollectionEntry`, `FlowMappingPhase`, and the `ConsumedMapping` enum at `lib.rs:1061-1088`.

**Files:** `src/lib.rs`, `src/state.rs` (new)

- [x] Create `src/state.rs`
- [x] Move `StepResult`, `IterState`, `MappingPhase`, `CollectionEntry`, `impl CollectionEntry`, `FlowMappingPhase`, `ConsumedMapping`
- [x] Add `mod state;` declaration in `lib.rs`; update all internal references (there will be many — the `impl EventIter` blocks match these enums frequently)
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors:** none — pure move but wide touch surface; careful with imports

### Task 13: extract tag and anchor scanning into properties.rs (#4a-iv) — b171ce1

Move node-property scanning functions from `src/lib.rs` into a new `src/properties.rs` module. YAML 1.2 §6.9 calls these "node properties" (anchors + tags), which gives the module its name.

**Files:** `src/lib.rs`, `src/properties.rs` (new)

- [x] Create `src/properties.rs`
- [x] Move `scan_anchor_name` (`lib.rs:1274`), `scan_tag` (`lib.rs:1330`), `scan_tag_suffix` (`lib.rs:1473`), `is_valid_tag_handle` (`lib.rs:1543`). `is_ns_tag_char_single` from chars.rs is now the single source — no need to move.
- [x] Add `mod properties;` declaration in `lib.rs`; update call sites in `impl EventIter` blocks
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors:** none — pure move (Task 1 already normalised the tag-char predicate)

### Task 14: extract mapping-key line helpers into mapping.rs (#4a-v) — 69596e2

Move line-level mapping-key detection helpers from `src/lib.rs:1089-1259` into a new `src/mapping.rs` module. Also migrates the one lib.rs unit test at `lib.rs:4585-4627` which covers these helpers' contract.

**Files:** `src/lib.rs`, `src/mapping.rs` (new)

- [x] Create `src/mapping.rs`
- [x] Move `is_implicit_mapping_line` (`lib.rs:1091`), `is_tab_indented_block_indicator` (`lib.rs:1101`), `inline_contains_mapping_key` (`lib.rs:1113`), `find_value_indicator_offset` (`lib.rs:1154`)
- [x] Move the `#[cfg(test)] mod tests` block at `lib.rs:4571-4628` (single test `find_value_indicator_agrees_with_is_implicit_mapping_line`) into the new `mapping.rs`
- [x] Add `mod mapping;` declaration in `lib.rs`; update call sites in `impl EventIter` blocks
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors:** none — pure move including the test

### Task 15: EventIter — pending_anchor enum consolidation (#5a) — c5913f1

Replace the pair `pending_anchor: Option<&'input str>` + `pending_anchor_for_collection: bool` with a single `pending_anchor: Option<PendingAnchor<'input>>` field backed by an enum. Eliminates the invalid-state representation where `_for_collection` is undefined when `pending_anchor` is `None`. Pure refactor; no behaviour change.

**Files:** `src/lib.rs` (or `src/state.rs` if Task 12 has landed the enum module)

- [x] Define `PendingAnchor<'input>` enum with variants `Standalone(&'input str)` and `Inline(&'input str)` — place in `state.rs` alongside other state types
- [x] Update `EventIter` struct: remove `pending_anchor_for_collection: bool`, change `pending_anchor` type to `Option<PendingAnchor<'input>>`
- [x] Audit every call site that reads or writes `pending_anchor` or `pending_anchor_for_collection` (~20+ sites across `consume_mapping_entry`, `try_consume_scalar`, `handle_sequence_entry`, `handle_mapping_entry`, `handle_flow_collection`, anchor-scanning sites, and collection-open sites). Convert each:
  - Read access: match on `Option<PendingAnchor>` variants instead of reading two fields
  - Write access: construct `Some(PendingAnchor::Standalone(...))` or `Some(PendingAnchor::Inline(...))`
  - Clear: `None` (both old fields cleared at once)
- [x] Verify no call site inadvertently reads `pending_anchor_for_collection` without first checking `pending_anchor.is_some()` — the disjoint encoding made this latent; the refactor eliminates the possibility
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, `cargo test --test conformance` — 368/368 conformance, 503 smoke, 1313 unit, zero warnings
- [x] **Advisors required:**
  - **test-engineer input gate:** test list embedded in dispatch message from prior-session TE (scenarios A-1 through A-11) — satisfied
  - **test-engineer output gate:** TE gave conditional sign-off, developer added B-8/B-10/B-11 error-path tests, TE gave full sign-off — satisfied

### Task 16: EventIter — pending_tag enum consolidation (#5b) — 5b316fc

Parallel refactor to Task 15 for the tag state. Replace `pending_tag: Option<Cow<'input, str>>` + `pending_tag_for_collection: bool` with `pending_tag: Option<PendingTag<'input>>`. Same structure, same advisor requirements.

**Files:** `src/lib.rs` and/or `src/state.rs`

- [x] Define `PendingTag<'input>` enum with variants `Standalone(Cow<'input, str>)` and `Inline(Cow<'input, str>)` — place alongside `PendingAnchor` in `state.rs`
- [x] Update `EventIter` struct: remove `pending_tag_for_collection: bool`, change `pending_tag` type to `Option<PendingTag<'input>>`
- [x] Audit every call site (~20+ — symmetric with Task 15 since tags and anchors flow through the same state-machine sites)
- [x] Same latent-bug check as Task 15
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, `cargo test --test conformance` — 368/368 conformance, 513 smoke, 455 unit, zero warnings
- [x] **Advisors required:** test-engineer input + output gates — both satisfied (T-1/T-2 inline enum tests, T-3 through T-12 smoke tests covering standalone/inline propagation, Cow::Owned via %TAG directive, tag clearing between items, tag+anchor pairing)

### Task 17: EventIter — fold `failed` into IterState::Done + remove allow (#5c)

Eliminate the `failed: bool` field by folding its semantics into `IterState::Done`. Both already mean "iterator is finished, stop yielding". Smallest of the three refactors. At this point the struct has 2 booleans (`root_node_emitted`, `explicit_key_pending`), below clippy's default threshold of 3, so the `#[allow(clippy::struct_excessive_bools)]` attribute at `lib.rs:315` is removed.

**Files:** `src/lib.rs` (and `src/state.rs` if IterState lives there after Task 12)

- [x] Remove `failed: bool` field from `EventIter`
- [x] Remove `failed: false` from the constructor
- [x] At the error-yield site, replace `self.failed = true;` with `self.state = IterState::Done;`
- [x] In `Iterator::next`, the early-return guard that checks `self.failed` becomes a check for `matches!(self.state, IterState::Done)` (may already exist; verify no duplication) — removed as redundant; the `IterState::Done` arm in the dispatch match already returns `None`
- [x] Remove `#[allow(clippy::struct_excessive_bools)]` from the `EventIter` struct definition
- [x] `cargo fmt`, `cargo clippy --all-targets` — zero warnings including `struct_excessive_bools`, `cargo test`, `cargo test --test conformance` — 1313 unit, 455 + 368 parser, 368 conformance, zero clippy warnings
- [x] **Advisors:** none — small refactor, low risk. The clippy warning check is itself the acceptance criterion for removing the allow.

### Task 18: create event_iter/base.rs (#4b-i) — 9555145

Create the `src/event_iter/` submodule and populate `base.rs` with mode-independent `EventIter` infrastructure: construction, scalar consumption, stack management, and the `Iterator` glue. These are called by every other `event_iter/` fragment, so this task must land first.

**Files:** `src/lib.rs`, `src/event_iter/base.rs` (new), `src/event_iter.rs` (new — declares submodules)

- [x] Create `src/event_iter.rs` with `mod base;` (the parent module file per project convention — no `mod.rs`; the plan originally said `pub(crate) mod base;` but `mod base;` is equivalent because `event_iter` itself is crate-private)
- [x] Create `src/event_iter/base.rs`
- [x] Move from `impl EventIter`: `new` (`lib.rs:403`), `try_consume_scalar` (`lib.rs:586`), `close_collections_at_or_above` (`lib.rs:432`), `close_all_collections` (`lib.rs:468`), `drain_trailing_comment` (`lib.rs:1013`), `min_standalone_property_indent` (`lib.rs:1048`), and the full `impl<'input> Iterator for EventIter<'input>` block (`lib.rs:4532`)
- [x] Moved methods use `pub(crate) fn` visibility — the plan's original directive (`pub(in crate::event_iter)`) is infeasible during this transient state because 14 call sites in `lib.rs` (inside the surviving `impl EventIter` blocks slated for Tasks 19-23) need to reach them. `pub(crate)` is the narrowest visibility that compiles. Tasks 19-23 drain those call sites out of `lib.rs`, after which a follow-up can tighten the visibility to `pub(in crate::event_iter)`. `zero_span` is also promoted to `pub(crate)` for the same reason.
- [x] Add `mod event_iter;` declaration in `lib.rs`
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` — all green; 1313 unit, 368 conformance, 455 + 513 parser, zero warnings
- [x] **Advisors:** none — pure move + visibility adjustment

### Task 19: create event_iter/directives.rs (#4b-ii) — 56a603a

Move directive-parsing methods and the `BetweenDocs` stepper into `src/event_iter/directives.rs`.

**Files:** `src/lib.rs`, `src/event_iter.rs`, `src/event_iter/directives.rs` (new)

- [x] Create `src/event_iter/directives.rs`
- [x] Add `pub(crate) mod directives;` to `src/event_iter.rs`
- [x] Move from `impl EventIter`: `consume_preamble_between_docs` (`lib.rs:1577`), `parse_directive` (`lib.rs:1614`), `parse_yaml_directive` (`lib.rs:1647`), `parse_tag_directive` (`lib.rs:1699`), `skip_and_collect_comments_in_doc` (`lib.rs:1773`), `step_between_docs` (`lib.rs:1795`)
- [x] Convert to `pub(in crate::event_iter) fn` — used `pub(crate)` as transient (same Task 18 precedent; 3 call sites in lib.rs — 2 for `marker_span`, and step dispatch — reach directly until Tasks 20-23 drain them). `marker_span` also promoted to `pub(crate)` for the same reason.
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` — all green; 455 unit, 368 conformance, 513 smoke, 21 unicode, 3 doc; zero warnings
- [x] **Advisors:** none — pure move

### Task 20: create event_iter/flow.rs (#4b-iii) — d6170c4

Move `handle_flow_collection` (~1,310 lines) into `src/event_iter/flow.rs` as a single atom. Do NOT attempt to split the function — the in-code design note at `lib.rs:3245-3253` deliberately preserves the repetition and the function declares a local `FlowFrame` enum that would require module-scope promotion to break up.

**Files:** `src/lib.rs`, `src/event_iter.rs`, `src/event_iter/flow.rs` (new)

- [x] Create `src/event_iter/flow.rs`
- [x] Add `pub(crate) mod flow;` to `src/event_iter.rs`
- [x] Move `handle_flow_collection` (`lib.rs:3196-4505`) in its entirety, including the inner `FlowFrame` local enum
- [x] Convert to `pub(in crate::event_iter) fn`
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, `cargo test --test conformance` — flow-collection tests are sensitive; verify full pass
- [x] **Advisors:** none (pure move). The reviewer should spot-check that the byte-for-byte function moved unchanged — no accidental edits inside the 1,310-line body.

### Task 21: create event_iter/step.rs (#4b-iv) — d1f0e10

Move the main `step_in_document` dispatcher (~740 lines) into `src/event_iter/step.rs`. This is the document-mode entry point that `Iterator::next` delegates to when `state == InDocument`.

**Files:** `src/lib.rs`, `src/event_iter.rs`, `src/event_iter/step.rs` (new)

- [x] Create `src/event_iter/step.rs`
- [x] Add `pub(crate) mod step;` to `src/event_iter.rs`
- [x] Move `step_in_document` (`lib.rs:1896-2636`)
- [x] Convert to `pub(in crate::event_iter) fn`
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] **Advisors:** none — pure move

### Task 22: create event_iter/block.rs + event_iter/block/sequence.rs (#4b-v) — a7657ab

Create the `event_iter/block/` sub-submodule hierarchy and populate it with block-sequence handling. `event_iter/block.rs` is the module file that declares `mod sequence; mod mapping;` (the mapping submodule arrives in Task 23). Task 22 creates the scaffolding plus the sequence handlers.

**Files:** `src/lib.rs`, `src/event_iter.rs`, `src/event_iter/block.rs` (new), `src/event_iter/block/sequence.rs` (new)

- [x] Create `src/event_iter/block.rs` containing `pub(in crate::event_iter) mod sequence;` (the mapping submodule is added in Task 23)
- [x] Add `pub(crate) mod block;` to `src/event_iter.rs` — declared as `mod block;` (crate-private by default, equivalent to `pub(crate)` because `event_iter` itself is crate-private; matches the Task 18 precedent for `mod base;`)
- [x] Create `src/event_iter/block/sequence.rs`
- [x] Move from `impl EventIter`: `handle_sequence_entry` (`lib.rs:2637`), `consume_sequence_dash` (`lib.rs:703`), `peek_sequence_entry` (`lib.rs:506`)
- [x] Convert to `pub(in crate::event_iter) fn` — used `pub(crate)` as transient (Task 18-21 precedent; lib.rs still has `impl EventIter` callers in the block-mapping handlers slated for Task 23, which require crate-wide visibility. A follow-up tightening pass after Task 23 will narrow visibility to `pub(in crate::event_iter)`)
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` — 1313 unit, 455 parser, 529 smoke, 368/368 conformance, zero warnings
- [x] **Advisors:** none — pure move + scaffolding

### Task 23: create event_iter/block/mapping.rs (#4b-vi) — 8705972

Move the block-mapping handlers into `src/event_iter/block/mapping.rs`. This is the final `impl EventIter` split task — after this, `lib.rs` is reduced to 165 lines (not ~80 as originally estimated; see note below).

**Files:** `src/lib.rs`, `src/event_iter/block.rs`, `src/event_iter/block/mapping.rs` (new)

- [x] Add `pub(in crate::event_iter) mod mapping;` to `src/event_iter/block.rs`
- [x] Create `src/event_iter/block/mapping.rs`
- [x] Move from `impl EventIter`: `handle_mapping_entry`, `consume_mapping_entry`, `consume_explicit_value_line`, `peek_mapping_entry`, `advance_mapping_to_value`, `advance_mapping_to_key`, `tick_mapping_phase_after_scalar`, `is_value_indicator_line` — all 8 methods moved byte-for-byte, no logic changes
- [x] Convert to `pub(in crate::event_iter) fn` — no `pub(crate)` fallback needed (cleaner than Tasks 18-22's transient visibility). Minor incidental changes noted during review: (a) `impl<'input> EventIter<'input>` → `impl EventIter<'_>` for the remaining `collection_depth` method, since `'input` is now unused; (b) `empty_scalar_event` tightened from `const fn` (implicitly crate-visible because child modules see private items in ancestor modules) to `pub(crate) const fn` for stylistic consistency with the sibling `marker_span`/`zero_span`
- [x] lib.rs is now 165 lines (not ~80 as the plan estimated). The plan's target didn't account for what must remain: the `EventIter` struct definition itself (~65 lines with doc comments for each of 12 fields), three `pub(crate) const fn` helpers (`empty_scalar_event`, `marker_span`, `zero_span`) shared across event_iter submodules, and the `collection_depth` method. Moving any of these would constitute scope creep — the Task 23 checkboxes specify only the 8 method moves, which are complete.
- [x] `cargo fmt`, `cargo clippy --all-targets` — zero warnings; `cargo test -p rlsp-yaml-parser` — 455 unit + 529 smoke + 21 unicode + 3 doc tests + supporting suites green; `cargo test --test conformance` — 368/368
- [x] **Advisors:** none consulted — pure move, no logic changes, no new code paths, no behaviour change

### Task 23b: relocate `src/properties.rs` → `src/event_iter/properties.rs` (#4c) — 4316828

Pure relocation of the `properties` module into the `event_iter/` subtree. The four functions (`scan_anchor_name`, `scan_tag`, `scan_tag_suffix`, `is_valid_tag_handle`) were extracted from `lib.rs` in Task 13 before `event_iter/` existed as a directory, so the module sits at crate root despite every caller living in `event_iter/`. Audit confirms: only `event_iter/step.rs`, `event_iter/flow.rs`, `event_iter/directives.rs` import from `crate::properties`.

**Files:** `src/properties.rs` → `src/event_iter/properties.rs` (move), `src/lib.rs`, `src/event_iter.rs`, `src/event_iter/step.rs`, `src/event_iter/flow.rs`, `src/event_iter/directives.rs`

- [x] `git mv src/properties.rs src/event_iter/properties.rs`
- [x] Remove `mod properties;` from `src/lib.rs`
- [x] Add `pub(crate) mod properties;` to `src/event_iter.rs` — used plain `mod properties;` (clippy flags `pub(crate)` as redundant inside the already-crate-private `event_iter` module; matches the Task 18 / Task 22 precedent for `mod base;` and `mod block;`)
- [x] Update `use crate::properties::...` → `use super::properties::...` in `event_iter/step.rs`, `event_iter/flow.rs`, `event_iter/directives.rs`
- [x] Narrow the four functions' visibility to `pub(in crate::event_iter) fn` — they have no callers outside `event_iter/`, so the tightening is safe at this point
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, `cargo test --test conformance` — 368/368 confirmed
- [x] **Advisors:** none — pure relocation, low risk, low uncertainty

### Task 23c: add unit tests for `event_iter/properties.rs` (#4c — follow-up) — 94721e4

Create `#[cfg(test)] mod tests` in `src/event_iter/properties.rs` with unit-level coverage for the four functions. Follows the Task 6/7b/8b/9b precedent — the module currently has no direct unit tests; coverage is only via `tests/smoke.rs` integration tests (including Task 27's 16 `verbatim_tag_uri_*` cases). `scan_tag` is a trust boundary (verbatim-tag URI parsing against untrusted input), so test-engineer may route to security-engineer at the input gate.

**Files:** `src/event_iter/properties.rs`

- [x] Create `#[cfg(test)] mod tests { use super::*; ... }` at the bottom of the file
- [x] Coverage for `scan_anchor_name`: valid anchor names; empty anchor indicator; anchors longer than `MAX_ANCHOR_NAME_BYTES`; anchor terminators (space, tab, flow indicators, line break)
- [x] Coverage for `scan_tag`: shorthand `!foo`, `!!foo`, verbatim `!<uri>`; percent-encoded sequences; rejection cases aligned with Task 27's §6.8.1 tightening (spaces, non-URI chars, bare `%`, control chars); length limit; embedded close-delimiter behaviour
- [x] Coverage for `scan_tag_suffix`: scan length calculation for valid tag-char sequences
- [x] Coverage for `is_valid_tag_handle`: `!`, `!named!`, `!!`; invalid forms (missing trailing `!`, non-word-char content)
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` — 530 unit (+75), 368/368 conformance, zero clippy warnings
- [x] **Advisors required:**
  - **test-engineer input gate:** 56-test spec + SE referral — satisfied
  - **test-engineer output gate:** sign-off granted on 75 tests (56 spec + 19 additive security strengthening) — satisfied
  - **security-engineer input gate:** 30-scenario adversarial spec — satisfied
  - **security-engineer output gate:** sign-off granted — satisfied
- [x] **Submission:** all four advisor gates cited in handoff; reviewer verified completeness. One review round-trip: reviewer rejected first submission for two Medium findings (missing embedded close-delimiter test + stale loop comment at properties.rs:89-97; duplicate test `scan_tag_verbatim_rejects_percent_one_digit_at_buffer_end`); developer resolved both on resubmission

### Task 23d: relocate `src/directive_scope.rs` → `src/event_iter/directive_scope.rs` (#4c) — 01a4f3d

Pure relocation. `DirectiveScope` was extracted from `lib.rs` in Task 11 before `event_iter/` existed. Audit confirms only `event_iter/base.rs` and `event_iter/step.rs` import `crate::directive_scope::DirectiveScope`.

**Files:** `src/directive_scope.rs` → `src/event_iter/directive_scope.rs` (move), `src/lib.rs`, `src/event_iter.rs`, `src/event_iter/base.rs`, `src/event_iter/step.rs`

- [x] `git mv src/directive_scope.rs src/event_iter/directive_scope.rs`
- [x] Remove `mod directive_scope;` from `src/lib.rs`
- [x] Add `pub(crate) mod directive_scope;` to `src/event_iter.rs` — used plain `mod directive_scope;` plus `pub use directive_scope::DirectiveScope;` (clippy rejects `pub(super)` inside the already-crate-private `event_iter` module as redundant; matches the Task 23b precedent for `mod properties;`)
- [x] Update `use crate::directive_scope::DirectiveScope` → `use super::directive_scope::DirectiveScope` in `event_iter/base.rs` and `event_iter/step.rs`; `lib.rs` updated to `use event_iter::DirectiveScope`
- [x] Narrow the `DirectiveScope` type + methods to `pub(in crate::event_iter)` where possible — fields (`version`, `tag_handles`, `directive_count`) and methods (`resolve_tag`, `tag_directives`) narrowed to `pub(in crate::event_iter)`. The struct itself remains `pub` because `EventIter` in `lib.rs:103` still declares a `directive_scope: DirectiveScope` field; the plan's original assumption that "the struct is not referenced outside event_iter after this move" was inaccurate. The `pub use` in the private `event_iter` module keeps the type reachable at `event_iter::DirectiveScope` without exposing it beyond the crate.
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, `cargo test --test conformance` — all green; 368/368 conformance, zero clippy warnings
- [x] **Advisors:** none — pure relocation

### Task 23e: add unit tests for `event_iter/directive_scope.rs` (#4c — follow-up) — 5ad49a1

Create `#[cfg(test)] mod tests` in `src/event_iter/directive_scope.rs`. The module currently has no unit tests — same coverage gap as properties.rs. Follows the Task 6/7b/8b/9b/23c precedent.

**Files:** `src/event_iter/directive_scope.rs`

- [x] Create `#[cfg(test)] mod tests { use super::*; ... }` at the bottom of the file
- [x] Coverage for `resolve_tag`: primary handle `!`, named handles `!foo!`, secondary handle `!!`, verbatim tags, unknown handles, empty suffix, oversized resolved tag against `MAX_RESOLVED_TAG_LEN`
- [x] Coverage for `tag_directives`: returns the directive map; interaction with `%TAG` handle registration (if the struct exposes a registration method — confirm against current API). Note during review: `DirectiveScope` exposes no public registration method — handles are inserted via `parse_tag_directive` in `event_iter/directives.rs` writing to the `pub(in crate::event_iter) tag_handles` field. The tests are in the same visibility scope and write directly to `tag_handles`, which is the only feasible API surface at the unit-test level.
- [x] Coverage for scope lifecycle: fresh scope has no handles; registering a handle and resolving against it; scope reset between documents. Note during review: production code "resets" the scope by reassignment of `DirectiveScope::default()` (`event_iter/step.rs:82,103`) — there is no `reset()` method. The lifecycle tests model this by asserting fresh-default invariants, which is what the production reset path relies on.
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` — 553 unit tests pass (up from 530, +23 new), 368/368 conformance, zero clippy warnings
- [x] **Advisors required:** test-engineer input + output gates — both satisfied (23 tests, TE sign-off granted); security-engineer not consulted (TE confirmed MAX_RESOLVED_TAG_LEN is a simple numeric boundary with no trust-boundary crossing, already reviewed when limit was established)

### Task 23f: relocate `src/state.rs` → `src/event_iter/state.rs` (#4c) — b66b26a

Pure relocation. The state-machine types (`StepResult`, `IterState`, `MappingPhase`, `CollectionEntry`, `FlowMappingPhase`, `ConsumedMapping`, `PendingAnchor`, `PendingTag`) were extracted in Task 12 before `event_iter/` existed. Audit confirms consumers are all event_iter submodules: `base.rs`, `directives.rs`, `flow.rs`, `step.rs`, `block/sequence.rs`, `block/mapping.rs`. **The existing `#[cfg(test)] mod tests` block at `state.rs:158` migrates alongside the code.**

**Files:** `src/state.rs` → `src/event_iter/state.rs` (move), `src/lib.rs`, `src/event_iter.rs`, all `event_iter/*.rs` that import `crate::state::...`

- [x] `git mv src/state.rs src/event_iter/state.rs`
- [x] Remove `mod state;` from `src/lib.rs`
- [x] Add `pub(crate) mod state;` to `src/event_iter.rs` — used plain `mod state;` (clippy flags `pub(crate)` as redundant inside the already-crate-private `event_iter` module; matches the Task 23b/23d precedent for `mod properties;` / `mod directive_scope;`)
- [x] Update `use crate::state::...` → `use super::state::...` (or equivalent) in all six event_iter consumers — four use `super::state::` (base, directives, flow, step), two use `crate::event_iter::state::` (block/sequence, block/mapping — one level deeper)
- [x] Narrow types to `pub(in crate::event_iter)` where the surface is not referenced outside event_iter — types left as `pub` inside the private `event_iter::state` module (effective crate-private visibility; same precedent as Task 23b/23d). `CollectionEntry`, `IterState`, `PendingAnchor`, `PendingTag` re-exported via `pub use state::{...}` in `event_iter.rs` because `EventIter`'s field declarations in `lib.rs` reference them; `MappingPhase` dropped from the re-export since `lib.rs` does not reference it directly
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, `cargo test --test conformance` — 553 unit + 368 conformance + 529 smoke + 21 unicode + 3 doc tests, zero clippy warnings
- [x] **Advisors:** none — pure relocation; existing tests migrate unchanged

### Task 23g: relocate and rename `src/mapping.rs` → `src/event_iter/line_mapping.rs` (#4c) — 9a48d38

Pure relocation combined with a rename. The line-level mapping-key helpers (`is_implicit_mapping_line`, `is_tab_indented_block_indicator`, `inline_contains_mapping_key`, `find_value_indicator_offset`) were extracted in Task 14 before `event_iter/` existed. Audit confirms only event_iter submodules import `crate::mapping`: `step.rs`, `block/sequence.rs`, `block/mapping.rs`. **The existing `#[cfg(test)] mod tests` block at `mapping.rs:174` migrates alongside.**

**Rename rationale:** the plan's original decision *"Two `mapping.rs` files coexist — renaming rejected as cosmetic churn"* was made before the single-consumer audit. The new context (module is exclusive to `event_iter/`) reverses that decision: placing the module under `event_iter/` while named `mapping.rs` would create `event_iter/mapping.rs` + `event_iter/block/mapping.rs` side-by-side, which is worse than the current split, not better. Renaming to `line_mapping.rs` disambiguates explicitly — the module contains *line-level* mapping-key detection, whereas `block/mapping.rs` contains the *block-mapping state machine*. See superseding Decisions entry.

**Files:** `src/mapping.rs` → `src/event_iter/line_mapping.rs` (move + rename), `src/lib.rs`, `src/event_iter.rs`, `src/event_iter/step.rs`, `src/event_iter/block/sequence.rs`, `src/event_iter/block/mapping.rs`

- [x] `git mv src/mapping.rs src/event_iter/line_mapping.rs` — git detected 96% similarity rename
- [x] Remove `mod mapping;` from `src/lib.rs`
- [x] Add `pub(crate) mod line_mapping;` to `src/event_iter.rs` — used plain `mod line_mapping;` (clippy flags `pub(crate)` as redundant inside the already-crate-private `event_iter` module; matches the Task 23b/23d/23f precedent)
- [x] Update `use crate::mapping::...` → `use super::line_mapping::...` (or equivalent absolute path) in the three event_iter consumers — `step.rs` uses `super::line_mapping::`, `block/sequence.rs` and `block/mapping.rs` use `crate::event_iter::line_mapping::` (one level deeper)
- [x] Narrow functions to `pub(in crate::event_iter)` where possible — all four functions narrowed
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, `cargo test --test conformance` — 553 unit + 529 smoke + 368/368 conformance + 21 unicode + 3 doc tests, zero clippy warnings
- [x] **Advisors:** none — pure relocation + rename; existing tests migrate unchanged

### Task 24: clean docs/benchmarks.md to current-state-only snapshot (#8) — 73c3371

Remove all historical content from `rlsp-yaml-parser/docs/benchmarks.md`: PEG-parser comparisons (acceptance proof for completed Task 22 of `2026-04-07-streaming-parser-rewrite.md`), Lazy Pos before/after tables (acceptance proof for completed `2026-04-10-unicode-position-safety-and-lazy-pos.md`), byte-level scanning before/after tables (acceptance proof for completed `2026-04-10-byte-level-scanning-and-memchr.md`), and the "Latest update" framing that treats the doc as a diff-over-time artifact. Result: a snapshot of current rlsp-yaml-parser vs libfyaml performance with environment, methodology, fixtures, current measurements, and forward-looking analysis of current behaviour.

**Files:** `rlsp-yaml-parser/docs/benchmarks.md`

- [x] Rewrite intro paragraph to remove "previous PEG-based parser" framing (line 3) — keep comparison to libfyaml
- [x] Remove "Latest update" note (line 18)
- [x] Trim header at line 61 — "Streaming parser" qualifier is meaningless without contrast
- [x] Delete entire "Side-by-side comparison" subsection (lines 71-84) and its table
- [x] Delete quoted comparison note at lines 120-121
- [x] Rewrite analysis paragraphs at lines 225-237 — describe streaming latency on its own merits, not vs. PEG
- [x] Rewrite heading at line 233 to drop the "137× faster than old parser" framing
- [x] Trim phrases at lines 241-242, 253 that contrast with old parser
- [x] Delete entire "Lazy Pos optimization results" subsection (lines 263-298)
- [x] Delete entire "Byte-level scanning optimization results" subsection (lines 300-343)
- [x] Delete entire "## Comparison: old parser vs streaming parser" final section (lines 345-354)
- [x] Result: 243 lines (from 354) — close to the ~227 estimate; covers environment, methodology, fixtures, current rlsp vs libfyaml numbers, current-behaviour analysis
- [x] `cargo fmt` (markdown unaffected but consistency), no other verification needed
- [x] **Advisors:** none — docs-only cleanup

### Task 25: write rlsp-yaml-parser/README.md (#7a) — cdd1a18

Create `rlsp-yaml-parser/README.md` using the old README at commit `560230d` as structural template but rewriting all content against the current streaming parser. The file was deleted during the streaming rewrite (commit `cc5c9a5`) and its old content no longer applies. Must include a Short AI Note section and a link to `docs/benchmarks.md` instead of an inline performance table.

**Files:** `rlsp-yaml-parser/README.md` (new), `.ai/memory/project_followup_plans.md` (memory update)

- [x] Section: title + one-line description (spec-faithful streaming YAML 1.2 parser)
- [x] Section: Overview — describe streaming state-machine architecture, line-oriented lexing, zero-copy event iterator, separate loader. Do NOT reference parser combinators or "211 productions" — that was the PEG parser.
- [x] Section: Features — verify each claim against current code; adjust as needed. Spec-faithful, conformance pass rate (measure — see below), first-class comments, lossless spans, alias preservation (`LoadMode::Lossless` still exists), security controls
- [x] **Measure current conformance:** run `cargo test -p rlsp-yaml-parser --test conformance` and record the pass rate. Include the number in the Features section and Conformance section. — 368/368 measured and recorded
- [x] Section: Quick Start — verify examples compile against current API
  - `parse_events` iterator example
  - `load` top-level example
  - `LoaderBuilder` example (`new().resolved().max_nesting_depth(128).build().load(...)`). All methods verified present at `loader.rs:140-187`.
  - DO NOT include the old "Emit YAML" example — no `emitter` module exists
- [x] Section: API Overview — table of current public modules
  - `parse_events` top-level fn
  - `loader` — `load`, `Loader`, `LoaderBuilder`, `LoaderOptions`, `LoadMode`, `LoadError`
  - `node` — `Document`, `Node`
  - `event` — `Event`, `ScalarStyle`, `Chomp`, `CollectionStyle`
  - `encoding` — brief note (UTF-8/16/32 + BOM handling, typically internal-use)
  - `lines` — brief note (`Line`, `LineBuffer`, `BreakType`, typically internal-use)
  - DO NOT include `stream`, `emitter`, `schema` — those modules don't exist
- [x] DELETE the old "Schemas" section entirely — no `schema` module in the current parser crate
- [x] Section: Security Limits — verify defaults against `loader.rs:110-118` (`max_nesting_depth=512`, `max_anchors=10_000`, `max_expanded_nodes=1_000_000`). These match the old README; keep the table.
- [x] Section: Performance — one-sentence summary + link to `docs/benchmarks.md` (no inline duplicate numbers)
- [x] Section: Building — `cargo build/test/clippy/bench -p rlsp-yaml-parser`
- [x] Section: License — `[MIT](../LICENSE) — Christoph Dalski`
- [x] Section: AI Note — use the **Short** variant agreed during planning:

  ```markdown
  ## AI Note

  Every line of source in this crate was authored, reviewed, and committed by AI agents
  working through a multi-agent pipeline (planning, implementation, independent review,
  and test/security advisors for high-risk tasks). The human role is designing the
  architecture, rules, and review process; agents execute them. Conformance against the
  YAML Test Suite is a measured acceptance criterion — not an aspiration — and any change
  touching parser behaviour or untrusted input passes through formal test and security
  advisor review before being merged.
  ```

- [x] Update `.ai/memory/project_followup_plans.md` — the stale "Write rlsp-yaml-parser/README.md — DONE" entry has already been cleaned; verify no follow-up action needed, or add a note referencing this plan if historical continuity would help future agents — verified no action needed
- [x] **Advisors:** none — docs only. Reviewer verified Quick Start examples against current API signatures (`parse_events` at `lib.rs:47`, `load` at `loader.rs:249`, `LoaderBuilder::{new, resolved, max_nesting_depth, build, load}` at `loader.rs:147-197`) and confirmed security-limit defaults against `loader.rs:122-127`.

### Task 26: retrofit AI Note across rlsp-yaml, rlsp-fmt, root README + add missing crates to root Crates table (#7b) — 0bf9706

Add the Short AI Note section to `rlsp-yaml/README.md` and `rlsp-fmt/README.md`. Replace the existing one-liner AI Note in `/workspace/README.md` with the Short variant so all four READMEs share identical wording. Update the root README's Crates table to include `rlsp-yaml-parser` and `rlsp-fmt` (currently only lists `rlsp-yaml`).

**Files:** `README.md`, `rlsp-yaml/README.md`, `rlsp-fmt/README.md`

- [x] Add Short AI Note section to `rlsp-yaml/README.md` (currently has none — append before License or after License, matching convention)
- [x] Add Short AI Note section to `rlsp-fmt/README.md` (currently has none)
- [x] Replace the existing AI Note in `/workspace/README.md` — old text is one sentence; replace with the Short variant
- [x] Update the Crates table in `/workspace/README.md` to include rows for `rlsp-yaml-parser` (link to `rlsp-yaml-parser/README.md`, description: "Spec-faithful streaming YAML 1.2 parser") and `rlsp-fmt` (link to `rlsp-fmt/README.md`, description: "Generic Wadler-Lindig pretty-printing engine")
- [x] **Advisors:** none — docs only

### Task 27: tighten verbatim-tag URI validation using is_ns_uri_char_single (#1-C2) — ad790db

Close the spec-conformance gap in `scan_tag` (`lib.rs:1355-1363` — will move during Task 13 to `properties.rs`). Currently the verbatim-tag URI path only rejects control characters (`< '\x20' || == '\x7F'`), accepting non-URI characters that violate YAML 1.2 §6.8.1 production [38]. This task wires `is_ns_uri_char_single` (which was kept in chars.rs during Task 1 for this purpose) into `scan_tag` so each character of a verbatim-tag URI must be either a member of `ns-uri-char` or a `%HH` percent-encoded sequence.

**This is a behaviour change** — input that other YAML parsers accept (e.g., non-ASCII characters or spaces inside `!<...>`) will start returning `Err`. Task is deferred to the end of the plan so the rest of the codebase is stable first.

**Files:** `src/chars.rs` (no change; `is_ns_uri_char_single` already present), `src/properties.rs` (`scan_tag` after Task 13)

- [x] Replace the control-character-only loop at the verbatim-tag URI path with character-by-character validation against `is_ns_uri_char_single` or `%HH` sequences
- [x] Error message: cite YAML 1.2 §6.8.1 production [38]; include the position. **Trade-off (per security-engineer [High] finding):** offending character is NOT included in the error message — only the byte offset. Character reflection in error output was rejected as a reflection-injection risk. The plan's "include the offending character" requirement is overridden by the security advisor's specification.
- [x] Update any existing test cases that used verbatim tags with non-URI characters — none required updating; existing tests at `smoke.rs:8178-8234` use only inputs still accepted/rejected identically under the new predicate (verified)
- [x] Add new test cases for the rejection path: space, `{`, `}`, non-ASCII like `中`, unescaped `<`/`>`/`|` — done (16 tests in Group A2). `>` is a special case: it cannot be rejected because it is the URI close delimiter; instead `verbatim_tag_uri_embedded_close_delimiter_terminates_uri` documents the spec-correct behaviour where `!<foo>bar>` parses as `tag=foo` with `bar>` as scalar content
- [x] Add test cases for the accept path that should still work: alphanumeric + `-_.~*'()[]#;/?:@&=+$,`, percent-encoded `%20` (space), `%2F` (slash) — done
- [x] Run the full YAML Test Suite — conformance remains 368/368, no regression
- [x] `cargo fmt`, `cargo clippy --all-targets`, `cargo test`, `cargo test --test conformance` — 455 unit, 368/368 conformance, 529 smoke, zero clippy warnings
- [x] **Advisors required:**
  - **security-engineer input gate:** consulted before implementing; full assessment received — three issues identified: [High] no character reflection, [Medium] bare `%` handling, [Low] embedded `>` behavior — satisfied
  - **security-engineer output gate:** signed off on completed implementation — all three issues resolved — satisfied
  - **test-engineer input gate:** **NOT consulted before implementing** — plan violation. The developer proceeded directly using the security assessment's test scenarios as the test-design baseline. The deviation was disclosed transparently to the test-engineer at the output gate.
  - **test-engineer output gate:** signed off after the developer added two TE-required tests (`verbatim_tag_uri_less_than_rejected` and `verbatim_tag_uri_embedded_close_delimiter_terminates_uri`). Full sign-off granted — satisfied

## Decisions

- **chars.rs cleanup split into two tasks (C1 and C2).** C1 is a pure refactor (delete unused, de-duplicate, remove allow); C2 is a behaviour change that closes a spec gap. Bundling them would conflate refactoring with a trust-boundary change. C2 is deferred to end-of-plan so advisor consultation happens when the rest of the codebase is stable.
- **`flow.rs` stays as one 1,310-line file.** The in-code design note at `lib.rs:3245-3253` explicitly rejects helper-extraction refactors for `handle_flow_collection`. Overriding that note is out of scope for this plan.
- **`block` submodule uses nested folder structure.** `event_iter/block.rs` + `event_iter/block/sequence.rs` + `event_iter/block/mapping.rs` rather than `event_iter/block_sequence.rs` + `event_iter/block_mapping.rs`. The path hierarchy disambiguates the two `mapping.rs` files (`src/mapping.rs` at crate root for line-scanning helpers, `src/event_iter/block/mapping.rs` for block-mapping state-machine methods).
- **Two `mapping.rs` files coexist** — Rust paths disambiguate them and the hierarchy makes the distinction obvious. Renaming either was considered and rejected as cosmetic churn. **Superseded 2026-04-11 by the Task 23g decision below.**
- **Task 23g — `src/mapping.rs` renamed to `line_mapping.rs` and relocated to `event_iter/` (supersedes the two-mapping-rs decision above).** After Task 23 completed, an audit of crate-root modules for "used only by event_iter" affiliation identified four candidates (properties, directive_scope, state, mapping). For `mapping.rs` specifically, simply moving it under `event_iter/` would produce `event_iter/mapping.rs` + `event_iter/block/mapping.rs` side-by-side — worse than the current crate-root/submodule split, not better. Renaming to `line_mapping.rs` disambiguates by what the module actually contains: line-level mapping-key detection vs. block-mapping state machine. The original "cosmetic churn" concern no longer applies because the single-consumer context makes the relocation valuable in its own right and the rename is a logical corollary of the move, not a standalone cosmetic change.
- **Tasks 23b/23c/23d/23e/23f/23g added mid-plan (2026-04-11).** After Task 23 completed, the user asked whether `src/properties.rs` should be part of `event_iter/` (since it's only referenced from there) and whether it had unit tests. Audit of all crate-root support modules revealed four "used only by event_iter" candidates: `properties.rs`, `directive_scope.rs`, `state.rs`, `mapping.rs`. The first two have no direct unit tests (same gap as `lexer/comment.rs` before Task 6, and `stream.rs`/`reloc.rs`/`comments.rs` before Tasks 7b/8b/9b); the latter two already have unit tests migrated in their originating tasks. Following the Task 6/7b/8b/9b precedent, each of the four gets a relocation task, and the two with coverage gaps get a follow-up test-creation task with test-engineer input + output gates. Plan grew from 30 → 36 tasks. Execution ordering: after Task 23 (event_iter/ split complete), before Task 24 (benchmarks cleanup): 23b → 23c → 23d → 23e → 23f → 23g → 24.
- **loader.rs keeps `LoadState` intact.** The internal cohort of `LoadState` methods (`parse_node`, `register_anchor`, `resolve_alias`, `expand_node`) are tightly coupled; splitting them into sibling files would require `pub(super)` on every method and add file-hop friction without improving clarity. Only the three self-contained helper sections are extracted.
- **EventIter Fix 4 (fold `root_node_emitted` into `IterState::InDocument`) declined.** After Fixes 1-3, the struct has 2 booleans (below clippy threshold); no warning to silence. Folding `root_node_emitted` into the state variant would require helper methods or `matches!` contamination at every read/write site. The philosophical win doesn't justify the practical noise, and it contradicts the project's demonstrated preference for "bool-with-a-comment" over "new-type-for-invariant" (`flow.rs` design note).
- **Clippy allow is removed in Task 17.** Once `EventIter` drops to 2 booleans, `#[allow(clippy::struct_excessive_bools)]` at `lib.rs:315` becomes a no-op and is deleted. The removal is bundled with Task 17 rather than its own task because it is a mechanical consequence of the bool-count drop.
- **Short AI Note chosen over Medium/Minimal** for consistency across all four crate READMEs. The medium-length version was considered for the parser specifically but the user chose to standardize on Short across all READMEs.
- **Benchmarks doc becomes current-state-only** (not a running log of optimizations). Each historical layer — PEG comparison, Lazy Pos, byte-level scanning — is preserved permanently in its corresponding plan file and commits. The live doc focuses on current rlsp-yaml-parser vs libfyaml only.
- **Ordering: #5 between #4a and #4b.** The EventIter bool consolidation touches 40+ call sites. Doing it before the `event_iter/` split means the refactor lives inside a single file (`lib.rs`), which is much easier to reason about than a refactor spread across `event_iter/base.rs`, `flow.rs`, `step.rs`, `block/sequence.rs`, `block/mapping.rs`. #5 is orthogonal to #4a (none of the support-module extractions touch `EventIter` fields), so #4a can execute before #5.
- **Ordering: #7 after #4 and #8.** The parser README's Architecture section references the final folder layout (including `event_iter/`, `limits.rs`, `directive_scope.rs`, etc.), so it must be written after the splits land. The README's Performance section links to `docs/benchmarks.md`, so that doc should be in its cleaned-up form first.
- **Execution protocol: pause between tasks.** Per user directive — the lead waits for user confirmation after every reviewer-approved task before dispatching the next. Opt-out phrase: "auto go on". (User opted out mid-plan after Task 1; auto-advance mode since.)
- **Tasks 7b/8b/9b added mid-plan (2026-04-11).** After Task 7 (stream.rs extraction) was committed, the user observed that the extracted helper functions had no direct unit-test coverage and asked whether tests should be migrated alongside the functions. Verified: the four `stream.rs` functions, the `reloc` function, and the two `attach_*_comment` helpers had no direct unit tests anywhere in the codebase — they were only exercised indirectly through integration tests in `tests/loader.rs` and `tests/smoke.rs`. Same situation as `lexer/comment.rs` before Task 6. Following the Task 6 precedent, Tasks 7b/8b/9b were added to create direct unit tests for each extracted helper module. Plan grew from 27 → 30 tasks. Execution ordering: at-end-of-improvement-6 (8 → 9 → 7b → 8b → 9b → 10…). Each new task requires test-engineer input AND output gate consultation.
