**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-20

## Goal

Retrofit `completion.rs::complete_at` to consume the
parser AST instead of splitting `text` into lines and
reconstructing YAML structure with text-scanning helpers.
`complete_at` is the last feature-level function in
`rlsp-yaml/src/` that still bypasses the parser for its
own structural analysis, and the 16 private helpers it
owns are allow-listed as `HelperOf { root: "complete_at" }`
awaiting this retrofit. Completing this retrofit retires
17 allow-list entries at once (1 `TodoRetrofit` + 16
`HelperOf`), shrinking the audit from 44 entries to 27
and completing the feature-level scope of the
"One Parser, One AST" program for `rlsp-yaml`.

All 146 existing completion tests must continue to pass.
Bugs exposed during the retrofit must be fixed (not
preserved); tests that encode buggy behavior are updated
to the correct expectation and the fix is recorded in
Decisions.

## Context

### Current state

`completion.rs::complete_at` has this signature today
(`completion.rs:32`):

```rust
pub fn complete_at(
    text: &str,
    documents: Option<&Vec<Document<Span>>>,
    position: Position,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem>
```

The server call site (`server.rs:835`) already threads a
parsed `Option<&Vec<Document<Span>>>` into the function,
but the body ignores the AST for structural work — it
calls `text.lines().collect::<Vec<&str>>()` and then calls
16 private text-scanning helpers to reconstruct the key
path, siblings, sequence context, and cursor
classification. The `documents` parameter is only
consulted as an early-exit guard (`documents.is_none()`
returns empty) — the completion logic itself is driven
entirely from the `Vec<&str>` lines.

Measured in source terms (confirmed against current tree):

- `completion.rs` is 2701 lines total; the `#[cfg(test)]`
  module starts at line 997, so production code is ~996
  lines and tests are ~1705 lines.
- 146 test functions in `completion.rs::tests` (counted
  via `grep -c` against `^    #\[test\]|^    #\[rstest\]
  |^    fn `; the existing test suite already uses rstest
  parameterization for many shapes, so the number of
  `#[case]` entries is higher).
- 17 allow-list entries in
  `rlsp-yaml/tests/parser_boundary_audit.rs` pointing at
  this file (lines 96–216): 1 `TodoRetrofit` for
  `complete_at` and 16 `HelperOf` entries.

### The 16 helpers retired by this retrofit

Listed in allow-list order with a one-line purpose and
the source line each lives at today. Every one is
replaced by an AST-first equivalent (substrate in Task 1,
wiring in Task 2) and deleted in Task 2 in the same
commit as the rewire:

| Helper | Line | Purpose |
|---|---|---|
| `build_key_path` | 162 | ancestor key path from root to cursor's parent |
| `build_value_key_path` | 215 | `build_key_path` + appended key whose value is under cursor |
| `collect_present_keys_at_indent` | 257 | keys already in the enclosing mapping (exclude from schema suggestions) |
| `classify_cursor` | 526 | decide `CursorContext::Key(key)` vs `Value(key)` |
| `suggest_sibling_keys` | 559 | structural sibling-key fallback when no schema |
| `is_in_sequence_item` | 587 | is cursor inside a `- ` sequence item? |
| `suggest_keys_for_sequence_item` | 629 | sibling-key suggestion for sequence items |
| `collect_current_sequence_item_keys` | 657 | keys already in this sequence item |
| `find_current_item_start` | 707 | line index of the `- ` that begins this item |
| `find_sequence_indent` | 738 | indent at which the `- ` markers sit |
| `collect_all_sequence_item_keys` | 766 | union of keys across all siblings of this sequence |
| `collect_sibling_keys` | 831 | mapping-sibling key collector (scan back + forward) |
| `find_mapping_colon` | 977 | quote-aware `:` locator for a line |
| `indentation_level` | 958 | leading-whitespace count |
| `document_range` | 933 | inclusive `(start, end)` line range bounded by `---`/`...` |
| `suggest_values_for_key` | 893 | reuse values seen for the same key name in this document |
| (helper that remains) | — | `extract_key` is used only by helpers listed above, so it is deleted in Task 2 along with them |

### The established AST-lookup pattern

`hover.rs::ast_walk` (`hover.rs:87`) and
`navigation/references.rs::span_contains`
(`references.rs:153`) set the pattern. `Position` is
translated into a parser `Pos`:

```rust
let cursor = Pos {
    byte_offset: 0,
    line: position.line as usize + 1, // LSP 0-based → parser 1-based
    column: position.character as usize,
};
```

`span_contains(span, cursor)` returns `true` when the
cursor's `(line, column)` pair lies in
`[span.start, span.end)`. Walking the AST picks the
deepest node containing the cursor. `hover_at`'s
signature — `hover_at(docs: &[Document<Span>], position,
schema)` — drops `text: &str` entirely; the retrofit of
`complete_at` matches that shape.

### Blank-line completion and the cursor-column extension

Completion differs from hover because the cursor often
sits on a line that is empty or all-whitespace — no node
contains the cursor position. `hover_at` returns `None`
there; `complete_at` must still suggest siblings of the
enclosing mapping. The extension to the established walk:

- First try `span_contains` against nodes (as
  `hover_at` does).
- If no node contains the cursor, walk Mappings whose
  span covers `cursor.line` and descend into nested
  Mappings while `entry.key.loc.start.column <=
  cursor.column`. The deepest such Mapping is the
  enclosing mapping. Its entries are the sibling set;
  its key path is the ancestor key path.

This uses only parser-provided span columns (and the
LSP `Position.character` which is already a column). No
text-scanning. The root `CLAUDE.md` Crate Boundaries
section lists "byte-range arithmetic on parser-provided
spans" as an explicit permitted carve-out — the column
comparison here is an equivalent allowance.

Example:

```yaml
server:
  host: localhost
              ← cursor at line 2, column 2 (empty line)
```

- No node contains `(line=3, col=2)` in parser-1-based
  coordinates (`server:` value is a Mapping spanning
  lines 1–2, then EOF).
- Walk top-level Mapping containing `server:` at column
  0. Descend into `server`'s value (a nested Mapping
  with `host: ...` at column 2). `col (2) <=
  entry.key.col (2)` → descend.
- Inner Mapping has one entry at column 2. No deeper
  Mapping. That's the enclosing Mapping. Siblings =
  `{"host"}`. Key path = `["server"]`.

### Involved files

- `rlsp-yaml/src/completion.rs` — retrofit the public
  entry point and every private helper it uses.
- `rlsp-yaml/src/server.rs` — update call site at line
  835 to pass `&docs[..]` instead of `&text, docs.as_ref()`.
- `rlsp-yaml/tests/parser_boundary_audit.rs` — remove the
  17 allow-list entries in Task 2 (same commit as helper
  deletion).
- `rlsp-yaml/tests/lsp_lifecycle.rs` — integration test
  coverage through the server entry point (see Task 2).
- `rlsp-yaml-parser/tests/corpus.rs` / any existing
  invariant harness — corpus invariant added in Task 3.
- `.ai/memory/project_followup_plans.md` — the
  `complete_at` bullet (lines 29–33) is removed in
  Task 3; the post-program cleanup bullet remains.

### Specifications

- YAML 1.2 §8 (Block Styles) — block-mapping and
  block-sequence indentation semantics that the
  substrate must respect when determining the enclosing
  mapping from cursor column.
- LSP 3.17 Completion — `CompletionItem`, `Position`
  (line/character 0-based UTF-16 code units) semantics.
  The LSP spec on character encoding units is already
  handled upstream by the server; this retrofit
  inherits the existing Position handling.
- Root `/workspace/CLAUDE.md` Crate Boundaries section —
  "One parser, one AST. No code in rlsp-yaml/ may
  re-parse YAML structure from raw text." This is the
  constraint this retrofit eliminates for `complete_at`.

### Existing tests (preserved)

Existing test groups in `completion.rs::tests`:
backward-compat structural completion (no schema), schema
key completion, value enum completion, schema composition
branches (allOf/anyOf/oneOf), sequence items, deeply
nested paths, blank-line behavior, comment/separator
early exit, deduplication/ordering of merged suggestions.
All groups must continue to pass after the retrofit. Any
test whose expected output was derived from a bug in the
current implementation is updated to the correct
expectation as part of this plan — see the `Decisions`
section on bug-fixing policy.

## Steps

- [ ] Task 1: AST-first cursor-context substrate
- [ ] Task 2: Rewire `complete_at`, delete text helpers, shrink allow-list
- [ ] Task 3: Corpus invariant, memory queue cleanup

## Tasks

### Task 1: AST-first cursor-context substrate

Introduce the AST-walking helpers that Task 2 consumes.
New helpers sit alongside the existing text-scanning
helpers in `completion.rs` so that the workspace compiles
and the test suite passes end-to-end after this task. No
existing helpers are deleted yet and `complete_at`'s body
does not change — this task is pure addition.

Scope of new helpers (all private to `completion.rs`):

- `locate_cursor(docs: &[Document<Span>], position:
  Position) -> CursorLocation` — returns an enum
  describing what the cursor is on. Variants must cover
  every case `complete_at` currently distinguishes:
  - `OnKey { key: String, enclosing_path: Vec<String>,
    mapping: &Node<Span> }` — cursor is in a mapping
    key token.
  - `OnValue { key: String, enclosing_path:
    Vec<String>, mapping: &Node<Span> }` — cursor is in
    the value position of `key:`.
  - `InBlankMapping { enclosing_path: Vec<String>,
    mapping: &Node<Span> }` — cursor is on a
    blank/whitespace-only line inside (or at the end
    of) a mapping whose entries are at column ≤
    cursor column. This is the case
    `hover_at` returns `None` for.
  - `InSequenceItem { enclosing_path: Vec<String>,
    sequence: &Node<Span>, current_item: &Node<Span> }`
    — cursor is inside a specific sequence item.
  - `InBlankSequence { enclosing_path: Vec<String>,
    sequence: &Node<Span> }` — cursor is on a
    blank/whitespace-only line directly inside a
    sequence.
  - `OutsideAny` — cursor is not inside any
    structure the substrate can locate (empty
    document, position out of bounds, cursor on
    `---`/`...`, cursor on a comment).

  `enclosing_path` mirrors `build_key_path`'s output:
  ancestor mapping keys from document root down to the
  enclosing mapping/sequence, with `"[]"` sentinels for
  sequence descents. `OnKey`/`OnValue`/`InSequenceItem`
  all report the path of the enclosing structure —
  schema resolution in Task 2 appends the current key
  (or `"[]"`) as needed.

- `present_keys(mapping: &Node<Span>, cursor_line:
  usize) -> HashSet<String>` — keys already in the
  mapping, excluding the entry at `cursor_line` (the
  one being edited). Replaces
  `collect_present_keys_at_indent`.

- `collect_sibling_keys_ast(mapping: &Node<Span>)
  -> Vec<String>` — all keys in the enclosing
  mapping, preserving declaration order.

- `collect_sequence_sibling_keys(sequence:
  &Node<Span>) -> HashSet<String>` — union of keys
  across all items in the sequence (for sequence-item
  sibling suggestion).

Acceptance criteria (all must hold for Task 1 to be
complete):

- [ ] `locate_cursor` added with all six variants
      enumerated above; rustdoc on the function and
      every variant states precisely which cursor
      situation it represents.
- [ ] `present_keys`, `collect_sibling_keys_ast`,
      `collect_sequence_sibling_keys` added with
      rustdoc describing inputs, outputs, and
      side-effect freedom.
- [ ] A reusable `span_contains` that matches the
      existing hover/navigation implementations
      (`[start, end)` on `(line, column)` tuples) is
      either imported from an existing location or
      re-implemented locally; in either case, the
      behavior matches exactly so that cross-feature
      cursor-lookup semantics remain uniform.
- [ ] Unit tests (rstest with `#[case::name]` naming
      per `lang-rust-testing.md`) covering every
      `CursorLocation` variant across representative
      shapes:
      - key context: top-level key, nested key, key
        being typed in a sequence item
      - value context: scalar value, empty value
        (after `key:` with no value)
      - blank-mapping: blank line at root, blank line
        inside a nested mapping, blank line at EOF
        after a partial mapping
      - blank-sequence: blank line directly inside a
        `- items:` block
      - sequence item: cursor in a mapping-shaped `-`
        item, cursor in a scalar `-` item
      - outside-any: empty document, cursor past EOF,
        cursor on `---`, cursor on a comment
- [ ] Unit tests for `present_keys` assert that the
      entry at `cursor_line` is excluded and every
      other entry is included.
- [ ] Unit tests for `collect_sibling_keys_ast` and
      `collect_sequence_sibling_keys` assert ordering
      and deduplication rules.
- [ ] UTF-8 coverage: one rstest case per new helper
      uses a multi-byte key name (e.g., `café`) and
      asserts the helper handles it correctly; this
      catches byte-vs-char index mistakes at the
      column-comparison boundary.
- [ ] `cargo test -p rlsp-yaml` passes with zero
      failures — the new code compiles alongside the
      existing path, and the existing 146 completion
      tests are unchanged.
- [ ] `cargo clippy --all-targets` passes with zero
      warnings across the workspace.
- [ ] `cargo fmt --check` passes.
- [ ] No change to `complete_at`'s signature or
      body, and no change to the server call site —
      this task is purely additive.
- [ ] No entries removed from
      `rlsp-yaml/tests/parser_boundary_audit.rs`
      ALLOW_LIST yet — the shrink happens in Task 2
      alongside helper deletion.

### Task 2: Rewire `complete_at`, delete text helpers, shrink allow-list

Replace the body of `complete_at` so all structural work
routes through the Task 1 substrate, delete the 16
text-scanning helpers (and `extract_key`, which is used
only by those helpers), and remove the 17 allow-list
entries pointing at this file — all in the same commit.
The server call site and the public signature both
change to match the `hover_at` shape:

```rust
// Before:
pub fn complete_at(
    text: &str,
    documents: Option<&Vec<Document<Span>>>,
    position: Position,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem>

// After:
pub fn complete_at(
    docs: &[Document<Span>],
    position: Position,
    schema: Option<&JsonSchema>,
) -> Vec<CompletionItem>
```

Caller change in `server.rs` around line 835:

```rust
let items = crate::completion::complete_at(
    docs.as_deref().unwrap_or(&[]),
    position,
    schema.as_ref(),
);
```

Rewiring, helper deletion, and allow-list shrink go in
the same task for two reasons. First, the workspace lint
config sets `warnings = "deny"` (covers `dead_code`) —
keeping the helpers around without suppressions would
fail the build, and suppressions are explicitly
disallowed by this plan's Decisions. Second,
`parser_boundary_audit.rs` (`rlsp-yaml/tests/parser_boundary_audit.rs`:599–611)
has a "DEAD ALLOW-LIST ENTRIES" check that fails the
test if any allow-list entry does not match a live
violation in `src/`. Once `complete_at`'s params change
and the 16 helpers are deleted, all 17 entries become
dead and must be removed in the same commit to keep the
audit green.

Acceptance criteria (all must hold for Task 2 to be
complete):

- [ ] `complete_at`'s body reconstructed to branch on
      `locate_cursor`'s `CursorLocation` result:
      - `OutsideAny` → return `Vec::new()`, unless a
        schema is present and the location is
        `InBlankMapping` or `InBlankSequence` with a
        resolvable schema path, in which case fall
        through to schema key suggestions (preserves
        the current blank-line + schema branch at
        `completion.rs:64-76`).
      - `OnKey { key, path, mapping }` → structural
        sibling keys (`collect_sibling_keys_ast` +
        filter by `key`) plus schema key completions
        (`schema_key_completions` on
        `resolve_schema_path(path)` minus
        `present_keys`), merged via the existing
        `merge_completions`.
      - `OnValue { key, path }` → schema value
        completions (`schema_value_completions` on
        `resolve_schema_path(path ++ [key])`); fall
        back to `suggest_values_for_key` equivalent on
        the AST (scan the enclosing document's
        mapping entries for the same key and collect
        distinct values) when the schema branch
        returns nothing.
      - `InSequenceItem { sequence, current_item }`
        → sibling keys from the sequence minus keys
        already in `current_item`; schema logic
        descends into `items` via the `"[]"` sentinel
        path.
      - `InBlankSequence { sequence }` → schema items
        sibling-keys suggestion (if schema present)
        or structural union of sibling keys across
        items (if no schema).
- [ ] `complete_at`'s signature changed to
      `(docs: &[Document<Span>], position, schema)`.
      `text: &str` is removed. `Option<&Vec<...>>` is
      replaced with `&[...]`.
- [ ] `server.rs:835` updated to pass
      `docs.as_deref().unwrap_or(&[])` — no other
      server changes.
- [ ] Schema key/value completion plumbing
      (`resolve_schema_path`,
      `schema_key_completions`,
      `schema_value_completions`,
      `merge_completions`, `snippet_default`,
      `collect_schema_properties`, `type_label`,
      `truncate_description`, `truncate_enum_label`,
      `json_value_to_yaml_label`, `schema_has_properties`)
      is preserved unchanged — only the calls that
      feed them shift from text-derived paths to
      AST-derived paths.
- [ ] All 146 existing completion tests pass
      unchanged where they encode correct behavior.
      Tests that encoded a bug in the prior
      implementation are updated to the correct
      expectation, and every such update is listed in
      the Decisions section with the bug description.
      If zero tests are updated, state "no tests
      updated — no bugs surfaced" in the task's
      completion note.
- [ ] An integration test added to
      `rlsp-yaml/tests/lsp_lifecycle.rs` (or the
      existing completion-oriented integration file
      if one is closer) exercises `complete_at`
      through the server's `textDocument/completion`
      handler with at least these four shapes: cursor
      on an existing key, cursor on a value,
      cursor on a blank line inside a nested
      mapping, cursor on a blank line inside a
      sequence item. Each shape asserts at least one
      specific expected label. This satisfies the
      `integration-testing.md` rule for user-facing
      behavior.
- [ ] `cargo test -p rlsp-yaml` passes with zero
      failures.
- [ ] `cargo clippy --all-targets` passes with zero
      warnings across the workspace.
- [ ] `cargo fmt --check` passes.
- [ ] Every helper listed in the Context "The 16
      helpers retired" table is deleted from
      `completion.rs` in the same commit, along with
      any `use` imports that become unused.
      `extract_key` is deleted (it is used only by
      the retired helpers). The `CursorContext` enum
      at the top of the private section (before line
      526) is deleted if it has no remaining
      consumers after helper removal; otherwise left
      unchanged.
- [ ] No `#[allow(dead_code)]` or `#[expect(dead_code,
      ...)]` attributes are introduced at any point
      during the task — the helpers are deleted, not
      suppressed.
- [ ] `rlsp-yaml/tests/parser_boundary_audit.rs`
      ALLOW_LIST is updated in the same commit:
      the 1 `TodoRetrofit` entry for `complete_at`
      and all 16 `HelperOf { root: "complete_at" }`
      entries (lines ~96–216) are removed. The
      ALLOW_LIST count drops from 44 entries to 27.
      The audit test passes end-to-end.
- [ ] `parser_boundary_audit` test passes in this
      commit — no "DEAD ALLOW-LIST ENTRIES" failure,
      no "BOUNDARY VIOLATION" failure.

### Task 3: Corpus invariant, memory queue cleanup

Add the corpus-wide invariant test and scrub stale
references from the project follow-up queue. Code
cleanup (helper deletion, allow-list shrink) already
landed in Task 2 — this task's job is verification
against the yaml-test-suite corpus and housekeeping.

Acceptance criteria (all must hold for Task 3 to be
complete):

- [ ] Corpus invariant test added (standing team law,
      per `.ai/memory/feedback_yaml_test_suite_mandatory.md`):
      for each file in the yaml-test-suite corpus,
      parse it and call `complete_at` at every line
      in the file (cursor column 0, mid-line, and
      end-of-line for each line); assert the call
      does not panic and returns `Vec<CompletionItem>`
      with length ≤ `MAX_COMPLETION_ITEMS`. The test
      lives alongside existing corpus harnesses
      (extend the existing harness file in
      `rlsp-yaml/tests/` that already walks the
      corpus — match the pattern established by the
      navigation and document-symbols retrofits;
      create a new file only if no existing harness
      is suitable).
- [ ] If the corpus invariant exposes any panic or
      incorrect result, the production code is
      fixed in this task (see Decisions bug-fixing
      policy); a regression test for the failing
      corpus input is added alongside the fix; the
      fix is recorded in Decisions with the file
      path and one-line bug description. If zero
      failures, note "corpus invariant passed on
      first run — no production-code fixes required"
      in the task's completion message.
- [ ] `.ai/memory/project_followup_plans.md`: the
      `Retrofit complete_at to AST-first` bullet
      (currently lines ~29–33) is removed. The
      post-program cleanup bullet remains as a
      separate queue item — it explicitly lists this
      retrofit as its prerequisite and remains
      pending after this plan lands.
- [ ] `cargo test` workspace-wide passes with zero
      failures, including the new corpus invariant
      test.
- [ ] `cargo clippy --all-targets` across the
      workspace passes with zero warnings.
- [ ] `cargo fmt --check` passes.
- [ ] No test is marked `#[ignore]`, no regression
      is moved to a skip list, no scope is deferred
      to "post-program cleanup" — if any bug is
      exposed by the corpus invariant, it is fixed
      in this task and recorded in Decisions.

## Decisions

- **Signature change mirrors `hover_at`.**
  `complete_at` drops `text: &str` and takes
  `&[Document<Span>]` directly. Precedent:
  `hover.rs::hover_at(docs: &[Document<Span>],
  position, schema)`. The caller in `server.rs`
  already has the parsed documents; passing them as a
  slice is simpler than `Option<&Vec<...>>`. The empty
  slice signals "no AST available".
- **Blank-line completion uses parser span columns.**
  When no node contains the cursor, walk Mappings
  whose span covers `cursor.line` and descend into
  nested Mappings while `entry.key.loc.start.column
  <= cursor.column`. This preserves the user-visible
  blank-line-completion behavior without text
  scanning. The root `/workspace/CLAUDE.md` Crate
  Boundaries section permits "byte-range arithmetic
  on parser-provided spans" — the column comparison
  here is the same category.
- **Bugs found during implementation are fixed.**
  Preserve the 146 existing tests in general, but
  when the retrofit exposes a genuine bug, fix it.
  Two distinct classes both count as in-scope:
  (a) **Incorrect test expectation** — a test
      asserts a value that only matched because the
      text-based implementation had a bug. Update
      the test to the correct expectation and
      record it here during Task 2.
  (b) **Production-code defect** — a panic, an
      out-of-bounds access, an incorrect structural
      output, a missing early-exit guard, or any
      runtime misbehavior exposed by the new AST
      substrate or by the corpus invariant test.
      Fix the production code, add a regression
      test covering the failing input (test file
      the developer picks based on what the bug
      touches), and record it here during the task
      the bug surfaced in.
  Every fix is listed by class, filename (or test
  name), and one-line bug description. If zero
  fixes are required, state "no bugs surfaced" in
  the task completion note. This matches the
  `claim-verification.md` rule's bar — specifics
  per bug, not a category claim. Bugs are not
  deferred to post-program cleanup; that plan is
  scoped to test dedup and file splits, not
  correctness fixes.
- **Rewire, helper deletion, and allow-list shrink
  land in a single commit (Task 2).** Three forces
  make this necessary: (i) workspace lint deny for
  `warnings` + `dead_code` forbids leaving retired
  helpers behind across commits; (ii) the plan's
  own "no new `#[allow]` / `#[expect]`" decision
  forbids dead-code suppressions as a workaround;
  (iii) `parser_boundary_audit.rs`'s DEAD
  ALLOW-LIST ENTRIES check (lines 599–611) fails
  the test if allow-list entries outlive the
  functions they name. Task 3 becomes the
  corpus-invariant + queue-cleanup task as a
  result. The 3-task shape is preserved — Task 2
  is just denser than originally drafted.
- **One file, one task per slice.** The retrofit is
  scoped to `completion.rs` (and its callers /
  audit / memory queue). The accompanying `completion.rs`
  file split (still ~1800-2000 lines post-retrofit,
  over the 1500-line soft target) is explicitly
  owned by the post-program cleanup plan —
  this plan does NOT bundle a file split.
- **No new `#[allow]` or `#[expect]` attributes.**
  If the retrofit surfaces a lint we previously
  suppressed or surfaces a new one, fix the root
  cause. No suppressions are added to paper over
  dead code; Task 2 deletes the code instead.
- **Shrink-only allow-list discipline stands.**
  `parser_boundary_audit.rs::ALLOW_LIST` entries may
  only be removed by this retrofit, never added or
  renamed. The post-retrofit count is 27, documented
  in Task 3 acceptance.

## Non-Goals

- **File split of `completion.rs`** — tracked in the
  post-program cleanup plan (listed in
  `.ai/memory/project_followup_plans.md` under
  "Post-AST+formatter-program cleanup"). Not bundled
  here.
- **Test dedup across retrofit targets** — same
  cleanup plan owns this.
- **Adding new completion shapes** — the retrofit
  preserves existing user-facing behavior; new
  shapes (e.g., cursor inside a flow mapping, custom
  tag annotations) are out of scope.
- **Changing schema resolution logic** — the
  `resolve_schema_path`, `schema_key_completions`,
  `schema_value_completions`, `merge_completions`,
  and related schema utilities are preserved
  unchanged. Only the callers shift from text-derived
  paths to AST-derived paths.
- **Completion inside unparseable input** — when
  parsing fails, `docs` is empty and the retrofit
  returns `Vec::new()` (same as the current behavior
  when `documents.is_none()`). Any prior test that
  relied on text-scanning producing completions
  inside unparseable input is updated per the
  bug-fixing policy in Decisions.
- **Performance benchmarking** — if the retrofit
  changes completion latency, a follow-up plan can
  benchmark it. Not scoped here.
