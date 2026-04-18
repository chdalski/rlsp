**Repository:** root
**Status:** InProgress
**Created:** 2026-04-18

## Goal

Build a small real-world corpus and a broad invariant
test harness for `rlsp-yaml`'s LSP pipeline, so that
subsequent retrofit plans land against a stable,
TDD-style acceptance signal rather than only synthetic
unit tests. The plan also produces a shrink-only
skip-list of currently-failing (file, invariant) pairs
with follow-up-plan references, and a human-readable
`WORKLIST.md` mirror, so subsequent plans have a
concrete failure worklist to execute against. This
plan is sequenced first in the broader program (see
"Architectural program" in Context below) so retrofit
plans have a visible worklist and concrete regression
protection over files users actually edit.

## Context

### Architectural program

This plan is the first of four sequenced initiatives to
harden rlsp-yaml against real-world YAML and close the
class of bugs where LSP features re-implement YAML
parsing by hand. The sequence exists because the
user-reported bug (a false flow-mapping diagnostic on
`${{ … }}` inside a GitHub Actions workflow, plus a
destructive "convert to block" quick fix) passed every
existing test yet broke on first contact with the
user's own workflow file.

- **This plan (the first move)** — invariant harness +
  seed corpus + shrink-only skip-list. No production-
  code changes; surfaces the failure worklist.
- **Next plan (`.ai/plans/2026-04-18-one-parser-one-ast.md`)**
  — "one parser, one AST" rule added to root CLAUDE.md,
  `validate_flow_style` retrofitted from text-scanning
  to AST traversal, mechanical audit `#[test]`
  preventing new text-scan violators, release-plz
  gated on CI.
- **Fixture-pattern plan (later)** — extend the
  formatter's human-readable markdown fixture format
  to diagnostics, code actions, hover, completion, etc.
  Each bug becomes a fixture before it becomes a fix.
- **Corpus-and-invariant expansion (ongoing)** — grow
  this plan's corpus and invariant set over time;
  broader invariants (refactor AST equivalence,
  validator stability under whitespace re-emit,
  formatter round-trip) land as the surface area
  stabilizes.

Each move answers a distinct question: "does it
survive real YAML?" (this plan), "are we using the
parser?" (next plan), "does this specific input
produce the right output?" (fixtures), "does the
feature hold up across a growing real corpus?"
(ongoing).

### Why Move 0 exists

The user-reported GHA-expression bug passed every
existing unit test — the validator, the code action,
clippy, the formatter fixtures, and the yaml-test-suite
conformance suite — yet destroyed a real
`.github/workflows/release-plz.yml` on first contact.
The gap is that current tests exercise narrow inputs
and assert narrow outputs; nothing runs the full LSP
pipeline over representative real files and asserts
*general invariants* (no panics, valid ranges, code
actions produce parser-accepted text). Move 0 closes
that gap.

The user asked specifically for a TDD framing: land
the invariants first, surface the failure worklist,
then fix. That sequencing prevents a retrofit from
being declared "done" because its unit tests pass
while something else quietly broke.

### What this plan delivers

- `rlsp-yaml/tests/corpus/` — a seed corpus of 4
  real-world YAML files
- `rlsp-yaml/tests/corpus_invariants.rs` — a harness
  that runs every registered invariant over every
  corpus file and reports failures per (file,
  invariant) pair
- Four foundational invariants:
  - **I1 — No panics.** The full LSP pipeline (parser,
    every validator, formatter, code-action
    enumeration) must not panic on any corpus file.
  - **I2 — Diagnostic range validity.** Every emitted
    diagnostic has `range.start <= range.end`,
    positions within file bounds, positions aligned to
    character boundaries (UTF-16 code units per LSP
    spec for `.character`, UTF-8 byte boundaries when
    indexing into source text).
  - **I3 — Code-action output parses.** For every
    diagnostic that has an available code action,
    applying the text edit produces text whose parse
    introduces no new error-level
    (`DiagnosticSeverity::Error`) diagnostics
    compared to the pre-application parse. (This is
    the operational definition used in Task 3;
    "parses without errors" and "no new Error
    diagnostics after edit" are equivalent only when
    the original parse had no Error diagnostics, so
    the diagnostic-delta definition is what the
    harness actually checks.)
  - **I4 — Refactor code actions preserve scalar
    content.** For every code action with
    `kind == CodeActionKind::REFACTOR_REWRITE`, every
    scalar value (both keys and values) present in the
    pre-edit AST must still be present in the
    post-edit AST. New scalars may appear (e.g.
    explicit null markers from a structural rewrite);
    no pre-existing scalar may disappear. Catches the
    class of bugs where an edit produces syntactically
    valid but semantically destructive output — the
    parseable-but-data-losing failure mode that
    motivated the originating GHA-expression bug
    report. Added after Task 3 surfaced that I3 was
    too narrow to catch this class on its own.
- A shrink-only skip-list mechanism for currently-failing
  (file, invariant) pairs where every entry references a
  concrete follow-up plan file path (same discipline as
  Move 1's audit allow-list; ad-hoc markers without a
  filed plan are forbidden by the Surprise Failure
  Protocol)
- `rlsp-yaml/tests/corpus/WORKLIST.md` — a human-readable
  worklist listing every skip-list entry grouped by
  follow-up plan

### What this plan does NOT do

- **No production-code changes.** All deliverables live
  in `rlsp-yaml/tests/`. Validators, code actions,
  formatter, parser are untouched.
- **No fixing of invariant failures.** Failures surface
  onto the skip-list with follow-up-plan markers;
  fixing each one is the job of subsequent plans
  (Move 1 retrofit, destructive code-action fix, other
  validator retrofits).
- **No additional invariants beyond I1, I2, I3, I4.**
  I5 (validator stability under whitespace re-emit)
  and I6 (formatter round-trip) are explicitly
  deferred to later expansions under Move 3.
- **No corpus expansion beyond 4 files.** Expansion is
  follow-up work under Move 3.

### Relationship to existing tests

- `rlsp-yaml/tests/ecosystem_fixtures.rs` already tests
  narrow behaviors against K8s snippets (e.g. "no
  false-positives of diagnostic X on fixture Y"). Those
  tests remain as-is. Move 0's invariants are
  complementary — broad properties over a corpus,
  rather than narrow assertions per fixture.
- `rlsp-yaml/tests/fixtures/formatter/` — formatter
  markdown fixtures remain the precedent Move 2 will
  extend. Move 0 does not touch them.
- `rlsp-yaml/benches/hot_path.rs` and
  `benches/insight.rs` run validators for timing, not
  correctness. Unaffected.

### Lead pre-execution step (completed)

The destructive `flow_map_to_block` / `flow_seq_to_block`
quick-fix bug is known to produce I3 and I4 failures on
legitimate flow maps in the corpus (traced during the
investigation that motivated this plan). A follow-up
plan to fix that bug needed to exist before any task
that would create skip-list entries citing it, so those
entries could reference a concrete plan file rather
than unfiled TODO markers.

The lead filed the stub plan at
`.ai/plans/2026-04-18-fix-destructive-flow-to-block-code-action.md`
before Task 3 dispatched. The stub carries Repository,
Status NotStarted, Created, a Goal referencing this
plan and the Move 1 plan, and a single Step "file
proper plan content when Move 1 completes." Task 3 and
Task 4 skip-list entries for destructive-action
failures reference that exact path.

Task 3 landed with an empty skip-list because I3's
definition doesn't catch the destructive output (see
Task 4 for the motivation to add I4). Task 4 is
expected to populate the skip-list with entries against
the stub plan's path.

### Key implementation anchors

- Code-action entry point: `rlsp-yaml/src/editing/code_actions.rs`
  exposes `code_actions(text, cursor_range, diagnostics,
  uri)` (inspected during investigation). Test harness
  calls this directly rather than going through LSP
  JSON-RPC.
- Validator list (all to be invoked for I1):
  `validate_unused_anchors`, `validate_flow_style`,
  `validate_custom_tags`, `validate_key_ordering`,
  `validate_duplicate_keys`, `validate_yaml11_compat`.
- LSP `Position` uses UTF-16 code units for
  `.character` per LSP spec
  (https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocuments).
  I2 must check UTF-16-code-unit validity, not UTF-8
  byte validity.
- Panic catching: `std::panic::catch_unwind` with
  `std::panic::AssertUnwindSafe` for closures that
  borrow non-UnwindSafe values. This works across
  `catch_unwind` boundaries.
- LSP `TextEdit` application: apply edits in
  reverse-start-position order so earlier offsets
  remain valid as later edits are applied. Multiple
  edits per action are common; the harness must
  handle that.

### References

- Successor plan (queued behind this plan):
  `.ai/plans/2026-04-18-one-parser-one-ast.md`
- Existing ecosystem fixture harness (narrower
  precedent): `rlsp-yaml/tests/ecosystem_fixtures.rs`
- Formatter fixture pattern (the later fixture-pattern
  plan's precedent): `rlsp-yaml/tests/fixtures/formatter/`
- YAML 1.2 specification: https://yaml.org/spec/1.2.2/
- LSP specification 3.17:
  https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
- Kubernetes API reference (for Deployment fixture
  source): https://kubernetes.io/docs/reference/
- Compose file specification (for compose fixture
  source): https://compose-spec.io/

## Steps

- [x] Establish harness scaffolding and seed corpus
- [x] Implement invariants I1 (no panics) and I2
      (range validity)
- [x] Implement invariant I3 (code-action round-trip)
- [ ] Implement invariant I4 (refactor scalar/key
      preservation)
- [ ] Record baseline worklist

## Tasks

### Task 1: Harness scaffolding and seed corpus

Create the test harness file and populate the corpus
with 4 representative files.

- [x] Create directory `rlsp-yaml/tests/corpus/`
- [x] Add seed file `release-plz-workflow.yml` — an
      independent copy of the current
      `.github/workflows/release-plz.yml` (the file
      that triggered the originating bug report). Keep
      it as a checked-in corpus copy, not a symlink,
      so future workflow edits don't perturb the test
      corpus.
- [x] Add seed file `kubernetes-deployment.yaml` —
      a Deployment manifest exercising anchors,
      multi-container pods, env vars, volume mounts,
      and typical annotations. Source: a
      copyright-safe example from upstream Kubernetes
      documentation.
- [x] Add seed file `docker-compose.yml` — a typical
      compose file with services, environment,
      volumes, build context. Source: Compose spec
      example.
- [x] Add seed file `github-actions-matrix.yml` — a
      GitHub Actions workflow specifically exercising
      `strategy.matrix` with inline flow mappings
      (`{ target: …, os: … }`) *alongside*
      `${{ … }}` expressions, so the corpus contains
      both a legitimate flow collection case and the
      plain-scalar-with-braces case in the same file.
- [x] Create `rlsp-yaml/tests/corpus_invariants.rs`
      with:
  - Top-of-file comment stating the skip-list
    discipline: *the skip-list is shrink-only. Entries
    are removed as follow-up plans fix the root causes.
    New entries are only added when a NEW corpus file
    surfaces a known-fixable issue that has an
    immediate follow-up plan filed; never to silence
    a surprise failure. This constraint is the
    harness's enforcement surface — without it the
    test degrades to a rubber stamp.*
  - A `const CORPUS_DIR: &str = "tests/corpus"` anchor
  - A function enumerating every `.yml`/`.yaml` file in
    the corpus directory at test time (using `std::fs`,
    no new deps)
  - A data structure for registered invariants: each
    entry has an `id: &'static str` (e.g. `"I1"`), a
    `description: &'static str`, and a function
    `fn(&Path, &str) -> Result<(), String>` that runs
    the invariant on the file's contents
  - A `SKIP_LIST` constant: `&[(&str, &str, &str)]`
    tuples of
    `(corpus_file_name, invariant_id,
    followup_plan_reference_and_justification)`
  - A single `#[test] fn corpus_invariants()` entry
    point that for each (file, invariant) pair:
    1. Runs the invariant
    2. Compares against the skip-list
    3. Succeeds if (expected failure in skip-list AND
       actually failed) OR (not in skip-list AND
       actually passed)
    4. Fails if (expected failure in skip-list AND
       actually passed) — dead-weight skip entry; OR
       (not in skip-list AND actually failed) — new
       uncovered failure
  - At this task's scope, no invariants are registered
    yet (scaffolding only). The test runs successfully
    with an empty invariant set, printing
    "0 files × 0 invariants = 0 checks" or similar.
- [x] `cargo test --test corpus_invariants` passes
      (compilation + empty run completes cleanly)

Acceptance: the corpus directory exists with exactly
the 4 named seed files. The harness file compiles
under `cargo clippy --all-targets` with no warnings.
The empty test run completes successfully. The
shrink-only skip-list discipline is documented in a
top-of-file comment.

**Completed:** commit `678b16e` — harness scaffold
with 4 seed files, `INVARIANTS` / `SKIP_LIST` constants
empty, 8 harness-internal tests passing.

### Task 2: Register invariants I1 and I2

Implement the two foundational invariants and register
them in the harness.

- [x] Register I1 — "No panics on full LSP pipeline":
  - For each corpus file, sequentially invoke (each
    wrapped in `std::panic::catch_unwind` with
    `AssertUnwindSafe`):
    - `rlsp_yaml_parser::parse_yaml(text)` or
      equivalent top-level parse function
    - Every validator listed in Context
      (`validate_unused_anchors`, `validate_flow_style`,
      `validate_custom_tags`, `validate_key_ordering`,
      `validate_duplicate_keys`,
      `validate_yaml11_compat`), each with inputs
      appropriate to its signature (some take text,
      some take docs, some take both — build docs
      once per file and reuse)
    - `format_yaml` (or the top-level formatter entry
      point; inspect `rlsp-yaml/src/editing/formatter.rs`
      for the exact function name)
    - `code_actions(text, default_cursor_range,
      &all_diagnostics_from_above, &fake_uri)`
  - Any caught panic → invariant fails; the failure
    message identifies which pipeline stage panicked
    and the panic message
- [x] Register I2 — "Diagnostic range validity":
  - For each corpus file, collect all diagnostics from
    all validators
  - For each diagnostic's `range`:
    - `range.start.line <= range.end.line`, and if
      equal, `range.start.character <= range.end.character`
    - `range.end.line` is `< file_line_count` (where
      line count is computed the same way LSP
      positions are — split on `\n`, counting lines)
    - `range.end.character` is
      `<= utf16_code_units_in_line(range.end.line)`;
      same for `range.start.character`
    - The byte offsets derived from (line, character)
      must land on UTF-8 character boundaries in the
      source text
  - Any failed check → invariant fails; the failure
    message identifies the diagnostic (code, range)
    and which check failed
- [x] Run the full harness on the corpus. Record every
      (file, invariant) failure. For each failure that
      corresponds to a known issue that will be fixed
      in a filed follow-up plan, add a skip-list entry
      referencing that plan by file path. Surprise
      failures (failures that do not correspond to a
      currently-filed follow-up plan) are handled by
      the Surprise Failure Protocol: the developer
      sends the lead a `SendMessage` identifying the
      (file, invariant) pair and the failure detail,
      and waits for the lead to either file a
      follow-up plan (whose path the developer then
      references in the skip-list entry) or direct
      the developer to treat the failure as
      in-scope-for-this-plan. The developer never
      adds a skip-list entry with an ad-hoc `TODO(...)`
      marker lacking a plan reference.
- [x] Per-entry skip-list verification: for each
      skip-list entry added in this task, temporarily
      remove the entry and run the harness. The
      harness must fail citing that specific (file,
      invariant) pair. Restore the entry. Record
      verification in the commit message.

Acceptance: I1 and I2 run against every corpus file.
All currently-failing (file, invariant) pairs either
pass or have a verified skip-list entry with a
specific follow-up-plan reference. `cargo test --test
corpus_invariants` exits successfully. `cargo clippy
--all-targets` clean.

**Completed:** commit `9074b51` — I1 and I2
registered; skip-list stayed empty (all 4 corpus files
pass both invariants). Adds 12 unit tests for
`check_diagnostic_ranges` and `check_utf8_boundary`.
Per-entry verification not applicable (no skip entries
added).

### Task 3: Register invariant I3 (code-action round-trip)

Implement the invariant that most directly catches the
destructive quick-fix bug class.

- [x] Register I3 — "Code-action output parses":
  - For each corpus file:
    - Collect all diagnostics from all validators (as
      in I1/I2)
    - Build a fake `Url` for the file (e.g.
      `file:///corpus/<filename>`)
    - Call `code_actions(text, cursor_range_covering_whole_file,
      &diagnostics, &uri)` to enumerate available
      actions
    - For each returned `CodeAction` with a
      `WorkspaceEdit`:
      - Extract the `TextEdit`s for this file
      - Apply the edits in reverse-start-position
        order to a copy of the source text
      - Parse the resulting text via `parse_yaml`
      - Check the resulting diagnostics: any
        `DiagnosticSeverity::Error` not present in the
        pre-application parse counts as an invariant
        failure
  - Failure message identifies: the code-action
    title, the originating diagnostic's code/range,
    and the new error introduced
- [x] Add skip-list entries for expected failures
      following the Surprise Failure Protocol from
      Task 2. Entries fall into two known classes:
  - Entries caused by `validate_flow_style`
    false-positives on `${{ … }}` reference
    `.ai/plans/2026-04-18-one-parser-one-ast.md`
    (Move 1, which will fix them).
  - Entries caused by `flow_map_to_block` /
    `flow_seq_to_block` destructive behavior on
    legitimate flow maps reference the
    destructive-code-action-fix stub plan filed by
    the lead during the pre-execution step (see
    Context → Lead pre-execution step). If that stub
    plan is absent when the developer reaches this
    task, the developer blocks and messages the lead
    rather than adding a skip-list entry with an
    unfiled TODO marker.
- [x] Any other I3 failure beyond these two classes is
      a surprise failure — apply the Surprise Failure
      Protocol from Task 2.
- [x] Per-entry skip-list verification as in Task 2.
- [x] `cargo test --test corpus_invariants` exits
      successfully with all failures accounted for.
      `cargo clippy --all-targets` clean.

Acceptance: I3 runs against every (file, diagnostic,
action) tuple. All currently-failing cases are on the
skip-list with verified per-entry coverage and
specific follow-up-plan references.

**Completed:** commit `f9e28ae` — I3 registered.
Skip-list stayed empty: all 4 corpus files pass I3 as
currently defined (no code action produces
syntactically-invalid YAML). Note the significant gap
this exposes — see the Decisions section entry on the
I4 addition for the finding. Adds 9 unit tests for the
`apply_text_edits` helper (reverse-order application,
multi-line span, UTF-16 indexing after multi-byte
chars, empty/zero-width edits).

### Task 4: Register invariant I4 (refactor scalar/key preservation)

Add I4 to the harness. I4 catches the class of bugs
where a code action produces syntactically valid but
semantically destructive output — specifically, the
kind of destructive quick-fix that motivated this
architectural program. The `flow_map_to_block` /
`flow_seq_to_block` code actions currently drop the
`GITHUB_TOKEN` key on inputs like
`GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}` (the
mangled output is parseable YAML, so I3 misses it, but
the key has vanished). I4 surfaces this as a failure
the destructive-code-action-fix follow-up plan will
drive to green.

**Operational definition:** For every code action with
`kind == CodeActionKind::REFACTOR_REWRITE`:
1. Collect the multiset of `Scalar` values from the
   pre-edit AST — both keys and values, recursing
   into nested collections
2. Apply the code action's `TextEdit`s to a copy of
   the source text (reverse-start-position order, same
   as I3)
3. Parse the post-edit text and collect the multiset
   of `Scalar` values from the post-edit AST
4. Assert every scalar value in the pre-edit multiset
   appears at least once in the post-edit multiset —
   new scalars may appear, none may disappear
5. If any pre-edit scalar value is missing post-edit
   → invariant fails; message identifies the
   code-action title, the originating diagnostic's
   code/range, and the specific scalar string that
   disappeared

Rationale for the "multiset subset, not equality"
framing: a legitimate refactor may add structural
scalars (e.g. an explicit `null` marker where one was
implicit). What must not happen is pre-existing
scalar content going away. This catches the
destructive-quick-fix data loss without false-flagging
legitimate structural expansion.

- [ ] Register I4 in the `INVARIANTS` constant with
      id `"I4"` and description referencing the
      scalar-preservation rule
- [ ] Implement the helper `collect_scalar_values(
      docs: &[Document<Span>]) -> Vec<String>` that
      walks every document's node tree, descending
      into mapping entries (both keys and values) and
      sequence items, and returns every `Scalar`
      node's value string as a flat vec. Empty
      scalars (`""`) are included.
- [ ] Implement the comparison helper
      `missing_scalars(pre: &[String],
      post: &[String]) -> Vec<String>` that returns
      scalars present in `pre` whose count in `post`
      is less than in `pre` (multiset subset check).
      Returns the missing values.
- [ ] I4 invariant function: per file, collect
      diagnostics, request code actions via
      `code_actions(text, whole-file range,
      &diagnostics, &uri)`, filter to those with
      `kind == CodeActionKind::REFACTOR_REWRITE`, for
      each apply edits + parse + check
      `missing_scalars(pre, post)`. Any non-empty
      missing list → failure, with the code-action
      title + originating diagnostic code/range +
      first missing scalar named in the message.
- [ ] Run the harness. Known expected failures:
  - `flow_map_to_block` / `flow_seq_to_block`
    destructive output on any corpus file where
    they fire and drop content → skip-list entry
    cites `.ai/plans/2026-04-18-fix-destructive-flow-to-block-code-action.md`
  - Any other I4 failure → Surprise Failure
    Protocol: developer messages the lead, blocks
    until a follow-up plan is filed or the lead
    directs in-scope handling
- [ ] Per-entry skip-list verification: for each
      entry added, temporarily remove it, run the
      harness, confirm the test fails citing that
      specific (file, invariant) pair, restore.
      Record verification in the commit message —
      name each verified entry explicitly.
- [ ] Add unit tests for `collect_scalar_values` and
      `missing_scalars` covering:
  - Empty document
  - Flat mapping (keys + values collected)
  - Nested mapping (recursion works)
  - Sequence of scalars
  - Mapping with sequence values (both sides
    traversed)
  - Duplicate scalar values (multiset semantics —
    two `foo` in pre requires two `foo` in post)
  - Missing-scalars helper: equal multisets return
    empty; pre larger than post returns the extras
- [ ] `cargo test --test corpus_invariants` exits
      successfully with all failures accounted for.
      `cargo clippy --all-targets` clean.
      `cargo fmt` applied.

Acceptance: I4 runs against every (file,
refactor-kind action) pair. All currently-failing
cases are on the skip-list with verified per-entry
coverage and specific follow-up-plan references.
Unit tests for both helpers present and passing.

### Task 5: Baseline worklist document

Produce a human-readable `WORKLIST.md` derived from
the skip-list so that follow-up plans have a visible,
reviewable worklist outside Rust source.

- [ ] Create `rlsp-yaml/tests/corpus/WORKLIST.md` with:
  - A short header explaining what the file is (the
    human-readable mirror of the `SKIP_LIST` constant
    in `corpus_invariants.rs`), the shrink-only
    discipline, and the Surprise Failure Protocol
    that gates adding entries
  - One section per follow-up plan reference, listing
    the (file, invariant) pairs that plan is expected
    to resolve, with a one-line explanation per entry
- [ ] Content must match `SKIP_LIST` exactly — any
      entry in `SKIP_LIST` appears in `WORKLIST.md`
      and vice versa, and every entry references a
      filed plan (no ad-hoc markers). Note in the
      `WORKLIST.md` header that the Rust constant is
      the source of truth; this file is a
      human-readable mirror.
- [ ] Add a short note to `rlsp-yaml/docs/feature-log.md`
      recording that the corpus-invariants harness
      has been introduced and linking to
      `tests/corpus/WORKLIST.md` for the current
      failure worklist.

Acceptance: `WORKLIST.md` exists and its entries
correspond 1:1 to the current `SKIP_LIST`. Every entry
references a filed plan by file path — no ad-hoc
markers (per the Surprise Failure Protocol).
`docs/feature-log.md` mentions the new harness.

## Decisions

- **Minimum viable invariant set: I1, I2, I3, I4.**
  I4 was added after Task 3 landed and exposed that
  I3's diagnostic-delta framing cannot catch
  semantically-destructive-but-parseable edits (the
  destructive `flow_map_to_block` output on
  `${{ … }}` inputs is valid YAML, just with the key
  eaten — so I3 passes). I4 closes that gap with a
  scalar-preservation check on refactor-kind actions.
  I5 (validator-stability-under-whitespace-reformat)
  and I6 (formatter-round-trip) remain deferred to
  later Move 3 expansions.
- **Seed corpus size: 4 files.** One from each of the
  three primary real-world YAML shapes users will use
  the LSP on (GitHub Actions, Kubernetes,
  docker-compose) plus a matrix-specific file that
  exercises the legitimate flow-map case. Expansion
  beyond these is Move-3 follow-up work.
- **Corpus files are checked-in copies, not symlinks.**
  Future edits to the source files (like our real
  `.github/workflows/release-plz.yml`) must not
  silently alter the test corpus.
- **Skip-list is shrink-only.** Same discipline and
  rationale as Move 1's audit allow-list. Entries are
  removed as follow-up plans fix the root causes; new
  entries only permitted when a NEW corpus file
  surfaces a known-fixable issue with a follow-up plan
  already filed. A surprise failure is grounds for
  filing a follow-up plan, not for silently adding a
  skip entry.
- **Per-entry skip-list verification required.** For
  each skip-list entry, temporarily remove it and
  confirm the harness fails citing that specific
  (file, invariant) pair. Restore. Record in commit
  message. Prevents dead-weight entries from regex
  misses or wrong filenames.
- **Harness invokes code actions in-process.** The
  code-action API
  (`rlsp-yaml/src/editing/code_actions.rs::code_actions(...)`)
  is called directly rather than via LSP JSON-RPC.
  Faster; does not require a running server; matches
  the pattern used by existing unit tests.
- **Invariants do not fix production code.** Failures
  are recorded as skip-list entries with follow-up-plan
  references. Each follow-up plan owns the fix and the
  corresponding skip-list entry removal.
- **No new dependencies.** `std::fs` walks the corpus
  directory; `std::panic::catch_unwind` catches
  panics. Keeps the harness simple and its behavior
  predictable across Rust versions.
- **Corpus `WORKLIST.md` is a human-readable mirror of
  the `SKIP_LIST` constant, not the source of truth.**
  The constant is enforced by the test; the markdown
  file is for reviewers and follow-up-plan authors.
  This avoids the drift risk of two independent
  records.
- **Empty-skip-list state is permanent infrastructure,
  not a cleanup target.** When the skip-list reaches
  zero entries (every follow-up plan has landed), the
  `SKIP_LIST` constant stays in place as an empty array
  and `WORKLIST.md` stays as a file whose body states
  "No currently-failing (file, invariant) pairs.
  Empty state is the desired steady state; it does not
  mean the harness is unused." This is the signal that
  the harness is operational and expects to stay that
  way — removing the empty scaffolding would make it
  easier for a future agent to forget the discipline
  when a new corpus file gets added. Discipline is
  cheaper to preserve than to re-establish.
- **Surprise Failure Protocol is the gate for any new
  skip-list entry.** The protocol (developer messages
  the lead, waits for a filed plan or an in-scope
  directive, never adds an ad-hoc TODO marker) is the
  only path by which a skip-list entry can be added
  after Move 0 lands. This makes the shrink-only
  discipline operational, not just aspirational.
