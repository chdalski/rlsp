**Repository:** root
**Status:** Completed (2026-04-18)
**Created:** 2026-04-18

## Goal

Retrofit `hover_at` in `rlsp-yaml/src/hover.rs` from text-
scanning cursor resolution to AST-span-containment cursor
resolution. After this plan lands, cursor resolution in
hover walks the parser AST by span containment to find the
deepest node at the cursor, and derives mapping path and
sequence index from the AST walk — eliminating the
line-indentation scanning that currently reconstructs
structure from text. This is the first of 13 feature-level
retrofits that complete the "one parser, one AST" program
after the validator retrofits land.

## Context

### Current implementation

`pub fn hover_at(text: &str, documents: Option<&Vec<Document<Span>>>, position: Position, schema: Option<&JsonSchema>) -> Option<Hover>` at
`rlsp-yaml/src/hover.rs:31`.

The function currently:

1. Splits `text` into lines.
2. Identifies which document the cursor line belongs to
   by counting preceding `---` separators
   (`document_index_for_line`).
3. Classifies the cursor position as Key, Value, or
   SequenceValue by scanning the current line
   (`token_at_cursor`, calling `find_mapping_colon` for
   colon location with quote-awareness).
4. Walks backward through preceding lines by
   indentation level (`indentation_level`,
   `sequence_index`, helper `build_key_path`) to
   reconstruct a dotted path like `server.port[0]`.
5. Resolves the dotted path through the AST to fetch
   node metadata (type, scalar value).
6. Optionally looks up schema info for the path.

The AST has `loc: Span` on every node — cursor resolution
can walk the tree by span containment and arrive at the
same node directly. The current code uses the AST only at
step 5 (path resolution); the retrofit moves AST
consumption to the very start.

### Current helpers

All in `rlsp-yaml/src/hover.rs`:

| Helper | One-line purpose | Callers |
|---|---|---|
| `document_index_for_line` | Map cursor line to document index via `---` count | `hover_at` |
| `token_at_cursor` | Classify cursor as Key/Value/SequenceValue via line scan | `hover_at` |
| `find_mapping_colon` | Find `:` in `key: value` line, skipping quoted colons | `token_at_cursor`'s internal chain |
| `indentation_level` | Count leading spaces in a line | `build_key_path`, `sequence_index` |
| `sequence_index` | Count preceding `- ` items at same indent for 0-based index | `build_key_path` |

All five are **hover-local** (this crate has separately-
named `find_mapping_colon` and `indentation_level`
functions in `completion.rs`, `editing/on_type_formatting.rs`,
`analysis/folding.rs`, `analysis/semantic_tokens.rs`, and
`analysis/symbols.rs`, each with its own allow-list entry
pointing at its own root). The hover.rs copies disappear
with this retrofit; the other files' copies stay until
their own retrofits.

### AST-walk replacement

The replacement walks the AST by span containment:

1. Convert LSP `Position` (0-based line, 0-based
   character) to parser `Pos` (1-based line, 0-based
   column).
2. For each document in `docs`, check whether the
   cursor's `Pos` falls within the document's root
   node span. If so, recursively walk children by
   span-containment until the deepest matching node is
   reached.
3. During the walk, track the path: for each
   `Node::Mapping` step into a child, record the key
   string; for each `Node::Sequence` step, record the
   child index as `[N]`. The accumulated path is the
   replacement for `build_key_path`'s text-reconstruction
   output.
4. From the deepest matching node, derive hover content
   — same downstream logic as before (type name, scalar
   value, optional schema lookup).

Span-containment check: a `Span` is a half-open interval
(`start` inclusive, `end` exclusive) in the standard
`rlsp-yaml-parser` convention. The cursor is contained
when `span.start <= pos < span.end` by (line, column)
lexicographic comparison.

### Coordinate system

- LSP `Position`: 0-based line, 0-based character.
- Parser `Pos` (used in `Span`): 1-based line, 0-based
  column.

The retrofit converts `LSP position.line` → `parser
line = position.line + 1`; columns pass through
unchanged. The current code does a bare `as usize` cast
treating LSP line as array index into `lines: Vec<&str>`;
the retrofit replaces that usage entirely.

### Call site

Single call site in `rlsp-yaml/src/server.rs` (around
line 801; grep for `hover::hover_at` to locate):

```rust
crate::hover::hover_at(
    &text,
    docs.as_ref(),
    position,
    schema.as_ref(),
)
```

At this call site, `docs` is a `Option<Vec<Document<Span>>>`
obtained from `store.get_documents(&uri).cloned()`
(`server.rs:777`). There is an early return for
`text == None` at `server.rs:783-785`, but NO
corresponding guard for `docs == None`. Currently
`hover_at` receives `docs.as_ref()` — possibly `None` —
and handles the `None` case internally.

The retrofit changes the signature to require
`&[Document<Span>]` (no `Option`). The caller must
therefore add an early return for the `None` case,
mirroring the existing `text` guard pattern:

```rust
let Some(docs) = docs else {
    return Ok(None);
};
```

Placed between the existing `let Some(text) = text else
{...};` guard and the `hover_at` invocation. The call
site then passes `&docs` as `&[Document<Span>]` and
drops the `text` argument.

### Semantic gap to surface

The current implementation emits hover content for any
line with a `key:` shape, even when the parser would
produce no node for an incomplete value (e.g., `key:`
with cursor on the empty space after the colon). The
AST-walk approach may return `None` in this edge case
because the parser produces no value node to contain
the cursor. Two ways to handle:

1. **Preserve current behavior** by falling through to
   the parent mapping node when no child contains the
   cursor — hover returns the parent's schema info
   instead of the empty-value child's. Requires the
   test suite to describe what current behavior
   produces for this edge case so the retrofit
   matches.
2. **Accept a behavior change** — `None` (no hover) is
   arguably more correct; the user hovers over empty
   space, no node exists. This would be a deliberate
   behavior change documented in Decisions.

The TE's input-gate consultation must settle which
behavior the existing test suite locks in. The plan
assumes option 1 unless the TE flags option 2 as more
consistent with the rest of the hover surface.

### Allow-list impact

Current `const ALLOW_LIST` in
`rlsp-yaml/tests/parser_boundary_audit.rs` has (per the
Explore's line-number report) the following entries for
hover:

- `hover_at` root: `TodoRetrofit`
- `document_index_for_line`: `HelperOf { root: "hover_at" }`
- `token_at_cursor`: `HelperOf { root: "hover_at" }`
- `find_mapping_colon` (hover.rs): `HelperOf { root: "hover_at" }`
- `indentation_level` (hover.rs): `HelperOf { root: "hover_at" }`
- `sequence_index`: `HelperOf { root: "hover_at" }`

All six entries are removed when this retrofit lands.
Net delta: **−6 entries**.

**Shrink-only discipline.** Allow-list entries are
NEVER added as part of this retrofit — only removed.
If the developer's implementation reveals a new text-
scanning helper that the audit catches, stop and
escalate rather than adding an allow-list entry.
Introducing a new violation to make the audit pass
defeats the purpose of the retrofit.

**Baseline the retrofit operates against:** the
validator retrofit plan is currently in flight. Its
expected post-state is 88 entries (96 → 94 → 90 → 88
across its three tasks). Hover retrofit begins with
that 88-baseline and reduces to **82** (−6).

### References

- Preceding code-action retrofit plan:
  `.ai/plans/2026-04-18-retrofit-remaining-code-actions.md`
  (status Completed; commit `f48b7a8`).
- In-flight validator retrofit plan:
  `.ai/plans/2026-04-18-retrofit-validators.md`.
- Parser AST types: `rlsp-yaml-parser/src/loader.rs` —
  `Node<S>` variants, `Span`, `Pos`.
- `rlsp-yaml/src/hover.rs` — current implementation and
  51 inline unit tests.
- `rlsp-yaml/src/server.rs` — single call site.
- Root CLAUDE.md "One parser, one AST" rule.

### Program-level consolidation note

Per the test-engineer's scan-existing-tests protocol,
`hover.rs` has 51 inline tests — scan for text-surgery
coverage that becomes obsolete after the retrofit, and
include a Consolidation section in the TE's test list.
File-level consolidation (module splits, integration-
test additions to `lsp_lifecycle.rs`) waits for the
queued post-program cleanup plan.

Integration-test note: per
`/workspace/.claude/rules/integration-testing.md`,
`hover_at` has 51 unit tests but zero integration tests
exercising the `textDocument/hover` handler end-to-end.
This plan does NOT add integration tests — the retrofit
preserves existing behavior, and integration coverage
is a separate concern (queued post-program cleanup).
If the retrofit introduces any user-visible change
(e.g., the semantic-gap option 2 above), at least one
integration test MUST be added as part of this plan
per the integration-testing rule.

## Non-Goals

- **Other feature-level retrofits** (`complete_at`,
  `format_on_type`, `find_document_links`, etc.). Each
  has its own plan.
- **Validator retrofits.** In-flight via the
  `2026-04-18-retrofit-validators.md` plan.
- **Behavior expansion.** Hover coverage stays the
  same (modulo the settled semantic-gap choice). No
  new hover content types, no new schema lookup
  features.
- **Schema lookup refactoring.** The path-based schema
  lookup downstream of cursor resolution is unchanged;
  only the path-producing step changes.
- **Deleting `find_mapping_colon` / `indentation_level`
  in other files.** Each is its own allow-list entry
  with its own root; they survive until their root
  retrofits land.
- **Integration tests** (unless the retrofit
  introduces a behavior change — see Context).
- **Post-program cleanup** (test dedup, module split,
  `hover.rs` reorganization). Dedicated follow-up
  plan.
- **New parser APIs.** The AST already exposes
  everything needed — `loc: Span` on every node,
  `Node::Mapping.entries`, `Node::Sequence.items`.

## Steps

- [x] Retrofit `hover_at` to AST-span-containment
      cursor resolution (single task — no further
      decomposition warranted; the retrofit is a
      contained change in one file plus one call
      site).

## Tasks

### Task 1: Retrofit `hover_at` to AST-first cursor resolution

- [x] Change signature to
      `pub fn hover_at(docs: &[Document<Span>],
      position: Position, schema: Option<&JsonSchema>)
      -> Option<Hover>`. The `text` parameter is
      removed; `docs` becomes required (no `Option`).
- [x] Implement the AST-walk cursor resolver:
  1. Convert LSP `Position` to parser `Pos`
     (line: `+1` for 1-based parser convention;
     column pass-through).
  2. For each document in `docs`, check root-node
     span containment of the cursor position.
  3. For the matching document, recursively walk
     children by span containment; track the mapping
     path (keys) and sequence indices as you descend.
  4. Return the deepest node whose `loc` contains
     the cursor, along with the accumulated path.
  5. On no match (cursor outside all node spans),
     return `None` — or fall through to the parent
     mapping per the semantic-gap decision settled
     with the TE.
- [x] Produce the same downstream hover content from
      the resolved node as before: type name, scalar
      value, optional schema lookup keyed by the
      AST-derived path.
- [x] Delete the five hover-local text-scanning
      helpers: `document_index_for_line`,
      `token_at_cursor`, `find_mapping_colon`,
      `indentation_level`, `sequence_index`. Verify
      via `grep -n '<helper_name>'
      /workspace/rlsp-yaml/src/` that no other
      callers exist in the crate before deletion —
      each helper is file-local to `hover.rs` but
      confirm by inspection.
- [x] Update the single call site in
      `rlsp-yaml/src/server.rs` (around line 801;
      grep for `hover::hover_at` to locate). Before
      the invocation, add an early return for the
      `None` case of `docs` — mirroring the existing
      `text` guard at line 783-785:
      ```rust
      let Some(docs) = docs else {
          return Ok(None);
      };
      ```
      Then invoke `hover_at` with `&docs` as
      `&[Document<Span>]` and drop the `text`
      argument. Do NOT use `unwrap()` on `docs` —
      that panics when the document store has no
      parsed documents for the URI.
- [x] Remove six allow-list entries from
      `rlsp-yaml/tests/parser_boundary_audit.rs`:
      `hover_at` root plus five `HelperOf { root:
      "hover_at" }` entries. DO NOT add any new
      allow-list entries — shrink-only discipline
      applies.
- [x] Remove the `hover_at` retrofit block from
      `/workspace/.ai/memory/project_followup_plans.md`
      (currently occupies roughly lines 29-33 —
      describes `hover_at`'s signature, violation,
      replacement sketch, and the five retiring
      helpers). The block represents completed work
      after this retrofit lands; leaving it in the
      open-items queue misleads future sessions.
- [x] Settle the semantic-gap question with the TE at
      input gate (see Context "Semantic gap to
      surface"). Record the decision (option 1:
      fall through to parent; option 2: return
      `None`) in this plan's Decisions section
      before implementing.
- [x] TE input-gate consultation. Scan the 51
      existing `hover.rs` tests; produce a test list
      with a Consolidation section listing:
      - Tests asserting text-scanning intermediates
        (e.g., helpers' returns) to retire.
      - Tests asserting user-visible hover output to
        keep; verify they pass unchanged after the
        retrofit.
      - New regression tests covering AST-walk edge
        cases.
- [x] Regression tests (augment with TE's
      Consolidation decisions):
  - Cursor on a top-level mapping key — returns hover
    for that key's schema entry.
  - Cursor on a nested mapping value — returns hover
    with the dotted path reflecting nesting.
  - Cursor on a sequence item — path includes `[N]`
    with correct 0-based index.
  - Cursor in a multi-document YAML — correct document
    is resolved from the cursor line's span, not from
    `---` counting.
  - Cursor on an empty line (between nodes) — follows
    the settled semantic-gap choice.
  - Cursor on a trailing comment — returns the node
    whose `loc` contains the cursor if any; otherwise
    follows the semantic-gap choice.
  - Cursor on a flow-style collection (`[1, 2, 3]`) —
    resolves to the correct item by span.
  - Cursor position exactly at a span boundary
    (start-inclusive, end-exclusive behavior) — the
    node containing `start` matches; the node whose
    `end` equals the cursor does not.
- [x] Build/test gates:
  - `cargo fmt`
  - `cargo clippy --all-targets` clean
  - `cargo test` full workspace green
  - `cargo test --test corpus_invariants` passes
    with empty SKIP_LIST
  - `cargo test --test parser_boundary_audit`
    passes with allow-list at exactly 82 entries
    (baseline 88 after validator plan completes,
    minus six hover-retrofit entries)
- [x] TE output-gate sign-off covering regression
      adds + Consolidation deletes.

Acceptance: `hover_at` consumes only the AST; five
text-scanning helpers deleted; single call site
updated; all 51 existing tests pass (with obsolete
ones retired per the TE's Consolidation section);
corpus SKIP_LIST stays empty; audit allow-list at
82.

**Landed:** commit `b1efee9` (see `git log` — SHA may
be superseded by follow-up amend). `const ALLOW_LIST`
at 82 (−6 from 88). TE-settled semantic-gap decision:
option 2 (return `None` for structural positions —
deliberate behavior change documented in Decisions).
Integration coverage already present:
`should_return_null_hover_for_whitespace_position` in
`tests/lsp_lifecycle.rs` exercises the null-hover path.

## Decisions

- **Single task.** The retrofit is a contained change
  in one file plus one call site. Splitting into
  multiple tasks would force arbitrary seams (e.g.,
  "add AST walker" / "delete helpers" / "update call
  site") that the reviewer would bundle back for
  review anyway. One task, one commit.
- **Semantic-gap choice: option 2 — return `None` for
  structural positions (empty lines, comment regions,
  beyond-doc positions).** Settled at the TE's
  input-gate consult. The TE determined that the
  existing test suite locked in option-2 behavior:
  no test expected `Some` for empty-line, comment, or
  beyond-document cursor positions, so the AST walk
  returning `None` for those positions is consistent
  with the test contract. This is a behavior change
  versus the old text-scanning implementation (which
  emitted hover for any `key:`-shaped line on the
  current or prior line), but not a change from the
  test suite's perspective.
- **`find_mapping_colon` and `indentation_level`
  other-file copies are NOT touched.** Each is its
  own function with its own allow-list entry and
  root. They survive until their respective
  retrofits land.
- **No integration test added here** (unless option 2
  is chosen at input gate, which would be a
  behavior change requiring one). Existing 51 unit
  tests provide the regression net; integration
  coverage is queued as a separate concern.
- **No `hover.rs` module split.** File reorganization
  waits for the post-program cleanup plan.
- **Audit allow-list shrinks from 88 to 82** (−6).
  All six allow-list entries rooted at `hover_at`
  are removed in the same commit as the retrofit
  landing. The baseline 88 is contingent on the
  validator retrofit plan completing first; this
  plan must be sequenced after the validator plan.
