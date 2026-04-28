**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-28

## Goal

Add a corpus invariant that asserts: for every YAML file
in `rlsp-yaml/tests/corpus/`, parsing the file and parsing
the formatter's output of that file produce semantically
equivalent ASTs. The invariant catches the class of
formatter bugs that drop, alter, or restructure data
between parse and re-parse — which the existing
fixture-based formatter tests cover only for hand-written
inputs, leaving real-world corpus files unchecked. If the
invariant surfaces failures on the current 4-file corpus,
fix them in-plan via the harness's Surprise Failure
Protocol (developer reports each failure to the lead;
lead decides per failure whether to fix in-plan or file
a follow-up).

This invariant lands in slot **I10** of the existing
`rlsp-yaml/tests/corpus_invariants.rs` harness. It was
originally named "I6" in the corpus-invariants scaffold
plan (`.ai/plans/2026-04-18-corpus-invariants-scaffold.md`,
completed) when only I1–I4 existed; slots I5 and I6 have
since been reassigned to the AST anchor_loc and tag_loc
invariants. The next free slot is I10. The follow-up
queue's "Corpus invariant: formatter round-trip" entry
is the work this plan implements; that entry will be
removed when this plan completes.

## Context

### What the invariant does and does not catch

**Catches:**

- Formatter drops a scalar value, anchor name, or user
  tag that was present in the pre-format AST.
- Formatter changes the resolved tag of a node (e.g.
  forces a quoted string output that re-parses with a
  different tag than the input).
- Formatter changes mapping entry count, sequence item
  count, or document count.
- Formatter produces output that re-parses into a
  different structural shape (mapping↔sequence,
  scalar↔collection).

**Does not catch (deliberately documented to keep
expectations honest):**

- Parser bugs that mis-parse consistently. If the parser
  produces the same wrong AST for both the original text
  and the formatted text, round-trip equivalence holds
  even though the AST is wrong vs. source semantics. The
  follow-up queue entry "Parser mis-attaches or drops
  properties on block mapping with combined `&anchor`
  and user `!tag`" is exactly this class — both pre- and
  post-format ASTs are equally wrong, so the
  formatter-round-trip invariant cannot surface it. That
  bug needs a separate plan with parser AST unit tests.
- Comment preservation, blank-line preservation, or
  any other non-AST whitespace concern. Comments are
  metadata and are not part of the AST equivalence the
  invariant asserts.

### Existing harness

`rlsp-yaml/tests/corpus_invariants.rs` already registers
nine invariants (I1–I9) and contains the
infrastructure new invariants extend:

- `INVARIANTS` array: `&[Invariant { id, description,
  check }]`. Add a new entry with `id: "I10"`.
- `SKIP_LIST` constant: `&[(corpus_file_name,
  invariant_id, followup_plan_reference)]`. Add entries
  here only for failures the lead directs to follow-up
  rather than fix in-plan; each entry must reference a
  filed plan path (no ad-hoc TODO markers, per the
  shrink-only discipline documented in the file's
  module-level comment).
- Surprise Failure Protocol (module-level comment, also
  used by every prior task that added an invariant): the
  developer reports any failure not on the skip-list to
  the lead via SendMessage; the lead either files a
  follow-up plan (whose path the developer references in
  a new skip-list entry) or directs in-scope handling.
  No skip-list entry without a filed plan reference.
- Per-entry skip-list verification (precedent from prior
  tasks): for any new skip-list entry, temporarily
  remove it and run the harness — confirm it fails
  citing that exact (file, invariant) pair. Restore.
  Record the verification in the commit message.

### Existing AST helpers

The harness already has `collect_scalar_values` and
`collect_node_scalars` that walk every document's node
tree (used by I4). Those helpers are scalar-only and
order-insensitive (they flatten to a multiset), which is
the wrong shape for AST equivalence — equivalence must
be order-sensitive and cover anchor/tag/structure,
not just scalar values. The new helper introduced in
Task 1 is structurally distinct from those.

### AST equivalence rule

Two `Vec<Document<Span>>` are equivalent iff:

1. Same number of documents.
2. For each pair of corresponding documents, the root
   nodes are equivalent under these rules (recursive,
   order-sensitive):
   - Same `Node` variant (`Scalar`, `Mapping`,
     `Sequence`, `Alias`).
   - `node.anchor()` returns the same `Option<&str>`
     value (anchor names equal).
   - `node.tag()` (the resolved tag string) returns the
     same `Option<&str>` value.
   - For `Scalar`: same `value` string. Style is not
     compared (Plain vs DoubleQuoted vs Literal can all
     represent the same value).
   - For `Mapping`: same entry count; entries pairwise
     equivalent in their stored order (key node
     equivalent, value node equivalent).
   - For `Sequence`: same item count; items pairwise
     equivalent in their stored order.
   - For `Alias`: same alias name.

The `tag()` comparison covers user-authored vs
resolver-injected tags correctly: if the formatter
preserves a user tag, both pre and post have the same
tag string; if the formatter strips a user-authored
core-schema tag (per `formatter.rs:540-573`'s
intentional design), both pre and post resolve to the
same tag string via the resolver, so equivalence still
holds. The invariant fires only when the round-trip
actually changes the tag — which is the bug class we
want to catch.

`tag_loc.is_some()` is intentionally NOT part of the
equivalence check — see the previous paragraph for why
formatter-stripped explicit `!!str` is correct behavior
that must not flag the invariant.

`Span` positions are not compared (round-trip is allowed
to shift positions due to whitespace differences).

NodeMeta `leading_comments` and `trailing_comment` are
not compared.

### Key implementation anchors

- Format entry point: `rlsp_yaml::editing::formatter::
  format_yaml(text: &str, opts: &YamlFormatOptions)
  -> String`.
- Parse entry point: `rlsp_yaml::parser::parse_yaml(text)`
  returns a `ParseResult` whose `documents` field is
  `Vec<Document<Span>>`. Used in I1, I3, I4 already.
- `Node<Span>` accessors: `anchor()`, `tag()`,
  `tag_loc()`, `anchor_loc()`. Defined in
  `rlsp-yaml-parser/src/node.rs`.
- Existing `Invariant` struct: see lines 60–69 of
  `corpus_invariants.rs`.

### References

- Parent program plan (completed):
  `.ai/plans/2026-04-18-corpus-invariants-scaffold.md`
- Harness file:
  `rlsp-yaml/tests/corpus_invariants.rs`
- Corpus directory:
  `rlsp-yaml/tests/corpus/`
- Worklist mirror:
  `rlsp-yaml/tests/corpus/WORKLIST.md`
- YAML 1.2 specification:
  https://yaml.org/spec/1.2.2/
- Follow-up queue (parser bug entry the invariant does
  NOT catch):
  `.ai/memory/project_followup_plans.md` —
  "Parser mis-attaches or drops properties on block
  mapping with combined `&anchor` and user `!tag`"

## Steps

- [ ] Implement AST equivalence helper with unit tests
- [ ] Register I10, run on corpus, address failures via
      Surprise Failure Protocol

## Tasks

### Task 1: AST equivalence helper with unit tests

Add a private helper `documents_equivalent(a:
&[Document<Span>], b: &[Document<Span>]) -> Result<(),
String>` to `rlsp-yaml/tests/corpus_invariants.rs`. The
helper returns `Ok(())` when the two sets of documents
are equivalent under the rule documented in the plan's
Context section, and `Err(path_description)` otherwise
where `path_description` identifies the location of the
first mismatch (e.g. `"documents[0]/mapping/entries[2]/
value: scalar value differs: 'foo' vs 'bar'"`).

The path-description format must allow a reader to
locate the mismatch without re-running the test:
- Document index: `documents[N]`
- Mapping descent: `mapping/entries[N]/key` or
  `mapping/entries[N]/value`
- Sequence descent: `sequence/items[N]`
- Specific mismatch suffixes: `: scalar value differs:
  'X' vs 'Y'`, `: anchor differs: Some("a") vs None`,
  `: tag differs: Some("X") vs Some("Y")`,
  `: kind differs: Scalar vs Mapping`,
  `: entry count differs: 3 vs 2`, etc.

This task adds **only the helper and its unit tests**.
No changes to `INVARIANTS` array, no I10 wiring. The
helper is unused after this task lands; Task 2 wires it
in. This split exists because the helper has independent
test surface area (eight unit-test cases below) that is
easier to land cleanly when not bundled with corpus
integration; the reviewer can verify correctness of the
comparison logic in isolation before it influences
production-corpus pass/fail.

- [ ] Add `documents_equivalent` to
      `corpus_invariants.rs` (visibility: file-private;
      not referenced from outside this test file).
- [ ] Apply the AST equivalence rule from the plan's
      Context section: same document count; recursive
      structural comparison; `node.anchor()`,
      `node.tag()`, scalar `value`, sequence/mapping
      counts and ordered children compared; styles,
      spans, NodeMeta comments NOT compared.
- [ ] On mismatch, return a descriptive
      `Err(path_description)` per the format above.
- [ ] Unit tests under `#[cfg(test)] mod tests` in the
      same file (existing test module already exists
      from prior tasks). Each test uses
      `rlsp_yaml_parser::loader::load(text)` to build
      input documents, calls `documents_equivalent`, and
      asserts the result. The test names below follow
      the existing module's naming convention
      (descriptive `should_*` style).

  The eight cases below are required (one assertion
  shape per case):

  - `should_return_ok_when_inputs_are_byte_identical`
    — `load("a: 1\n")` compared to itself returns
    `Ok(())`.
  - `should_return_err_when_document_counts_differ` —
    one-document input vs two-document input differ;
    the `Err` message starts with `"documents:"` or
    similar (exact prefix decided during
    implementation, but must identify the
    document-count mismatch).
  - `should_return_err_when_scalar_value_differs` —
    `"a: foo\n"` vs `"a: bar\n"`; `Err` message
    contains both `'foo'` and `'bar'`.
  - `should_return_err_when_anchor_name_differs` —
    `"a: &x 1\n"` vs `"a: &y 1\n"`; `Err` message
    mentions anchor difference.
  - `should_return_err_when_tag_differs` — for
    example, `"a: !custom 1\n"` vs `"a: 1\n"`; `Err`
    message mentions tag difference.
  - `should_return_err_when_mapping_entry_count_differs`
    — `"a: 1\nb: 2\n"` vs `"a: 1\n"`; `Err` message
    mentions entry-count difference.
  - `should_return_err_when_sequence_item_count_differs`
    — `"- 1\n- 2\n"` vs `"- 1\n"`; `Err` message
    mentions item-count difference.
  - `should_return_ok_when_only_styles_differ` —
    `"a: foo\n"` vs `"a: \"foo\"\n"`; both have the
    same scalar `value` `"foo"`; equivalence holds
    despite the style difference.

- [ ] `cargo test --test corpus_invariants` exits 0
      (compilation + new helper tests + existing
      harness tests all pass).
- [ ] `cargo clippy --all-targets` exits 0 with no
      warnings.
- [ ] `cargo fmt` applied.

Acceptance: `documents_equivalent` is defined in
`corpus_invariants.rs` with the equivalence rule from
the Context section. The eight unit tests above all
exist with the specified test names and all pass. The
existing harness tests still pass. The helper is not
yet wired into `INVARIANTS` (Task 2 does that). The
existing `INVARIANTS` array, `SKIP_LIST` constant, and
existing invariant check functions are unchanged.

### Task 2: Register I10 and run on the corpus

Wire `documents_equivalent` into a new I10 invariant,
register it, run the harness on the existing 4-file
corpus, and address every failure under the harness's
Surprise Failure Protocol.

- [ ] Add `check_i10_formatter_round_trip(path: &Path,
      text: &str) -> Result<(), String>`:
  1. `let parse_pre = parse_yaml(text);` — collect
     pre-format documents.
  2. If `parse_pre.documents` is empty (parse error
     produced no docs), return `Ok(())` — invalid YAML
     has no AST to round-trip.
  3. `let formatted = format_yaml(text,
     &YamlFormatOptions::default());` — format the
     original source.
  4. `let parse_post = parse_yaml(&formatted);` —
     re-parse the formatter's output.
  5. If `parse_post.documents` is empty, return
     `Err("formatter output failed to parse")` —
     formatter producing unparseable output is itself
     a failure mode this invariant catches.
  6. Call `documents_equivalent(&parse_pre.documents,
     &parse_post.documents)` and return its result.
- [ ] Register the invariant by adding an entry at the
      end of the `INVARIANTS` array:
      `Invariant { id: "I10", description: "Formatter
      round-trip: parsing format(text) produces an AST
      semantically equivalent to parsing text", check:
      check_i10_formatter_round_trip }`.
- [ ] Run `cargo test --test corpus_invariants`. Three
      possible outcomes:

  **(a) All 4 corpus files pass I10.** Skip-list
  remains empty; no further work required for failures.

  **(b) One or more corpus files fail I10 with a
  failure that is in-scope for this plan.** "In-scope"
  means: the lead, after reviewing the failure detail
  the developer sends via SendMessage, decides to fix
  it in this plan rather than file a follow-up. The
  developer fixes the underlying bug as additional
  sub-task work in this Task 2; once the fix lands,
  re-run the harness and confirm the previously-failing
  file now passes I10. No skip-list entry is added for
  in-plan-fixed failures.

  **(c) One or more corpus files fail I10 with a
  failure the lead directs to follow-up.** The lead
  files a follow-up plan, sends its file path back to
  the developer, and the developer adds a skip-list
  entry of the form `("filename.yml", "I10",
  "<filed_followup_plan_path> — <one-line
  justification>")`.

  In every case, the developer never adds a skip-list
  entry without a filed plan reference, and never
  resolves a failure silently. The Surprise Failure
  Protocol from the harness's module-level comment
  is the gate.

- [ ] Per-entry skip-list verification: for each
      skip-list entry added (if any), temporarily
      remove it, run the harness, confirm the test
      fails citing that specific (file, invariant)
      pair, restore. Record verification in the commit
      message — name each verified entry explicitly. If
      the skip-list is empty, state "no skip-list
      entries added; per-entry verification not
      applicable" in the commit message.
- [ ] If the skip-list changed (entries added):
  - Update `rlsp-yaml/tests/corpus/WORKLIST.md` to
    mirror the new `SKIP_LIST` entries 1:1 — each
    entry appears in WORKLIST.md grouped under its
    follow-up plan reference, with a one-line
    explanation.
  - Confirm `WORKLIST.md` and `SKIP_LIST` content
    match exactly (every constant entry has a
    WORKLIST.md line; every WORKLIST.md line traces
    to a constant entry).
- [ ] If the skip-list did not change:
  - `WORKLIST.md` remains as-is. State this in the
    commit message.
- [ ] `cargo test --test corpus_invariants` exits 0
      (every (file, invariant) pair either passes or
      has a verified skip-list entry).
- [ ] `cargo clippy --all-targets` exits 0 with no
      warnings.
- [ ] `cargo fmt` applied.

Acceptance: I10 is registered in the `INVARIANTS` array
with the specified id, description, and check function.
The corpus harness exits 0. Every I10 failure (if any)
either has been fixed in-plan with verifiable green
test, or has a verified skip-list entry referencing a
filed follow-up plan. WORKLIST.md mirrors SKIP_LIST
exactly.

### Cleanup task wrapper

Both tasks above include `cargo fmt` and `cargo clippy
--all-targets` as acceptance gates — these are not
separate tasks. The follow-up queue's existing entry
"Corpus invariant: formatter round-trip" is removed
when this plan reaches `Status: Completed`; the lead
handles that removal during the plan-completion commit
(per the convention that follow-up entries track
unplanned work; once a plan exists the queue marker
becomes redundant).

## Decisions

- **Slot I10, not I6.** The original Move 0 plan
  reserved "I6" for this invariant when only I1–I4
  existed. Slots I5 and I6 have since been reassigned
  to AST anchor_loc and tag_loc invariants in
  `corpus_invariants.rs`. The next free slot is I10.
  This decision is recorded in the follow-up queue's
  cleanup commit (`e4fe438`) and in this plan's Goal.
- **AST equivalence ignores style, spans, and
  NodeMeta comments.** The invariant asserts data and
  structural fidelity, not whitespace fidelity. Style
  differences (Plain ↔ DoubleQuoted ↔ Literal) on the
  same scalar value are not failures because they
  represent the same value. Span position differences
  are expected from any whitespace change. Comment
  preservation is a separate concern with its own
  potential invariant.
- **`tag_loc.is_some()` is not part of the
  equivalence check.** The formatter intentionally
  strips user-authored core-schema tags from
  non-empty scalars (per `formatter.rs:540-573`); the
  resolved tag string is the same on both sides, but
  `tag_loc.is_some()` flips between pre and post.
  Including `tag_loc.is_some()` in equivalence would
  flag this intentional behavior as a bug. The plan
  compares `node.tag()` (resolved tag string) only.
- **Helper is file-private, not pub.** The helper
  lives in the test file and is only used by the
  test. Exporting it for reuse is YAGNI; a future
  invariant that wants AST comparison can either reuse
  it from the test file or extract a shared helper at
  that time.
- **Two tasks, not one.** Splitting the helper from
  the integration is justified by independent test
  surface area: the helper has eight named unit tests
  whose correctness is verifiable in isolation, and
  bundling them with corpus-integration changes
  produces a larger commit that is harder to review.
  Each task is independently committable.
- **Surprise Failure Protocol governs every I10
  failure.** No skip-list entry without a filed plan
  reference; no silent in-plan fix without lead
  authorization. This matches the discipline every
  prior corpus-invariant task followed and is the
  harness's only enforcement surface against rubber
  stamping.
- **Out of scope: parser bug fixes.** Bug A
  (anchor migrates) and Bug B (tag dropped) on
  combined-property block mappings are tracked in the
  follow-up queue and explicitly noted as a class I10
  cannot surface. Fixing them requires parser AST
  unit tests, which is a separate plan.
- **No consolidation pass on `corpus_invariants.rs` in
  this plan.** This is the third additive pass on the
  file (after the corpus-invariants-scaffold plan that
  added I1–I4 and the destructive-flow-to-block-fix
  plan that updated the skip-list). The new
  `documents_equivalent` helper has a structurally
  distinct shape from the existing helpers
  (`collect_scalar_values`/`collect_node_scalars`
  flatten to a multiset, `apply_text_edits` operates
  on text not AST, `check_diagnostic_ranges` and
  `check_utf8_boundary` validate diagnostic ranges) —
  there is no overlapping or duplicated code to merge.
  The file's existing helper set is well-organized
  (each helper has a clear single purpose, sections are
  delimited by header comments), and no dead code is
  visible from the existing-passes' diff history.
  Adding a consolidation sub-task with no concrete
  cleanup target would be mechanical busywork that the
  user's memory rule against mechanical splits
  explicitly forbids. Recorded here so future plan
  reviewers know consolidation was considered and
  consciously rejected for this pass; the next plan
  that touches this file should re-evaluate.

## Non-Goals

- Implementing the validator-stability invariant
  (originally "I5" in Move 0; would land as I11).
  Tracked separately in the follow-up queue.
- Expanding the corpus beyond the existing 4 seed
  files. Tracked separately in the follow-up queue.
- Comment-preservation testing. Comments are
  metadata, not part of AST equivalence; if a
  comment-preservation invariant is wanted later,
  it will land as its own invariant (probably I11
  or later) and will not be folded into I10.
- Fixing the parser combined-property bugs (Bug A
  and Bug B in the follow-up queue). I10 cannot
  surface them; they need a parser AST test sweep
  in a separate plan.
- Removing the existing follow-up queue entry for
  this invariant. The lead removes it as part of
  the plan-completion commit, not as part of any
  task in this plan.
