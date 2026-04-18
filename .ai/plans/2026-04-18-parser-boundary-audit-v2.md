**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-18

## Goal

Extend `rlsp-yaml/tests/parser_boundary_audit.rs` so the
"one parser, one AST" rule is enforced for **all**
text-handling functions in `rlsp-yaml/src/`, not just
`pub fn validate_*` and `pub fn code_actions` with a
`text: &str` first parameter. The current audit misses two
real violator shapes: private helpers, and parameters
named anything other than `text` (such as `line`, `lines`,
`content`, `source`, `input`) with type `&str` or `&[&str]`.
The broadened audit produces a complete inventory of
text-handling surface and gives each function a visible
classification — either a `TODO(retrofit-*)` marker if
the function should consume the parser AST, or a
`// carve-out:` justification if the function handles a
genuine pre-parse lexical concern. The inventory is the
forward-protection mechanism for the remaining code-action
retrofits (`quoted_bool_to_unquoted`, `yaml11_bool_actions`,
`yaml11_octal_actions`, `schema_yaml11_bool_type_actions`,
`delete_unused_anchor`) — each retrofit shrinks the
allow-list, and no regression can be silently introduced.

## Context

### Why the current audit is insufficient

`rlsp-yaml/tests/parser_boundary_audit.rs` currently has
two narrow filters:

- **Function-shape filter** in `is_candidate_fn_line`: only
  matches `pub fn validate_*` and `pub fn code_actions`.
  Private helpers (`fn foo(...)`) are skipped entirely.
- **Parameter-shape filter** in `has_text_str_param`: only
  matches parameters literally named `text` with type
  `&str`. Parameters named `line`, `lines`, `content`,
  `source`, or `input` — or with type `&[&str]` — are
  missed.

Both filters were correct at the time they were written —
the audit was scoped to the then-only-known violators
(the four public validators and the old-style
`code_actions`). Subsequent retrofit work has revealed
many more text-handling surfaces the audit does not cover:

- The flow-to-block and block-to-flow code actions had
  private helpers `flow_map_to_block(lines: &[&str], ...)`,
  `flow_seq_to_block(lines: &[&str], ...)`,
  `block_to_flow(lines: &[&str], ...)`,
  `string_to_block_scalar(line: &str, ...)` that took text
  data but were neither `pub` nor had a first parameter
  named `text`. The audit missed all four.
- Queued code-action retrofits (`delete_unused_anchor`,
  `quoted_bool_to_unquoted`, `yaml11_bool_actions`,
  `yaml11_octal_actions`, `schema_yaml11_bool_type_actions`)
  are all private and take `lines: &[&str]` or `line: &str`.
  If one of them regressed, the audit would not catch it.
- Feature-level public APIs beyond validators and code
  actions — `hover_at`, `complete_at`, `format_on_type`,
  `find_document_links`, `find_colors` — all take
  `text: &str` as the first parameter and scan it with
  hand-rolled text logic. The audit does not consider them
  candidates because it filters by function name prefix.

### Enumeration of flagged functions

A broadened regex matching
`(pub )?fn \w+\s*\(\s*(text|line|lines|content|source|input)\s*:\s*&(?:\[&str\]|str)`
across `rlsp-yaml/src/` (excluding `#[cfg(test)]` modules)
surfaces the following functions. Each row states the
file, the function, the parameter shape that triggers the
match, and the classification: **V** = violator, needs
retrofit to AST-first consumption; **C** = carve-out,
legitimate pre-parse lexical concern exempt from the rule.

#### Already-allow-listed violators (carry forward)

| File | Function | Shape | Class |
|---|---|---|---|
| `validation/validators.rs` | `validate_unused_anchors` | `text: &str` | V (already listed) |
| `validation/validators.rs` | `validate_custom_tags` | `text: &str` | V (already listed) |
| `validation/validators.rs` | `validate_key_ordering` | `text: &str` | V (already listed) |
| `schema_validation.rs` | `validate_schema` | `text: &str` | V (already listed) |

#### New violators — queued retrofits (have specific follow-up items)

These have existing retrofit items in the follow-up queue
(`project_followup_plans.md`). Each allow-list entry
references the future retrofit plan marker.

| File | Function | Shape | Class |
|---|---|---|---|
| `editing/code_actions.rs` | `delete_unused_anchor` | `lines: &[&str]` | V |
| `editing/code_actions.rs` | `quoted_bool_to_unquoted` | `line: &str` | V |
| `editing/code_actions.rs` | `yaml11_bool_actions` | `lines: &[&str]` | V |
| `editing/code_actions.rs` | `yaml11_octal_actions` | `lines: &[&str]` | V |
| `editing/code_actions.rs` | `schema_yaml11_bool_type_actions` | `lines: &[&str]` | V |

#### New violators — feature-level public APIs (new retrofit plans to file)

These are public feature entry points that hand-roll YAML
scanning. Each needs a new retrofit follow-up plan filed
in `project_followup_plans.md` as part of this plan's
Task 2. The rule does not prohibit hover / completion /
decorator features — it prohibits them from re-parsing
YAML structure from raw text.

| File | Function | Shape | Class |
|---|---|---|---|
| `hover.rs` | `hover_at` | `text: &str` | V |
| `completion.rs` | `complete_at` | `text: &str` | V |
| `editing/on_type_formatting.rs` | `format_on_type` | `text: &str` | V |
| `decorators/document_links.rs` | `find_document_links` | `text: &str` | V |
| `decorators/color.rs` | `find_colors` | `text: &str` | V |

#### New violators — private helpers within already-flagged features

These are private helpers whose *root* entry point (the
containing feature's public API) is already on the
allow-list. Listing them individually would be noise —
fixing the root violator fixes the helpers as a byproduct.
Task 1 will add them to the allow-list with a `helper-of:`
marker referencing the root entry, so the count of
real tracked items stays close to the count of features
to retrofit.

Note: `collect_tag_diagnostics` (validators.rs) has
`node: &Node<Span>` as its first parameter and `lines:
&[&str]` second; the first-parameter anchor excludes it
from audit detection by design, so it does not need an
allow-list entry. `tab_to_spaces` (code_actions.rs) is a
carve-out and appears only in the carve-out table below.

| File | Function | Shape | Helper of |
|---|---|---|---|
| `validation/validators.rs` | `scan_tokens` | `lines: &[&str]` | helper-of: validate_unused_anchors |
| `validation/validators.rs` | `find_tag_occurrence` | `lines: &[&str]` | helper-of: validate_custom_tags |
| `validation/validators.rs` | `is_inside_quotes` | `line: &str` | helper-of: validate_custom_tags |
| `schema_validation.rs` | `build_key_index` | `lines: &[&str]` | helper-of: validate_schema |
| `hover.rs` | `document_index_for_line` | `lines: &[&str]` | helper-of: hover_at |
| `hover.rs` | `token_at_cursor` | `line: &str` | helper-of: hover_at |
| `hover.rs` | `find_mapping_colon` | `line: &str` | helper-of: hover_at |
| `hover.rs` | `indentation_level` | `line: &str` | helper-of: hover_at |
| `hover.rs` | `sequence_index` | `lines: &[&str]` | helper-of: hover_at |
| `decorators/document_links.rs` | `url_links` | `line: &str` | helper-of: find_document_links |
| `decorators/document_links.rs` | `include_links` | `line: &str` | helper-of: find_document_links |
| `decorators/document_links.rs` | `is_inside_quotes` | `line: &str` | helper-of: find_document_links |
| `decorators/document_links.rs` | `trim_trailing_punctuation` | `text: &str` | helper-of: find_document_links |
| `decorators/color.rs` | `value_start_offset` | `line: &str` | helper-of: find_colors |
| `editing/on_type_formatting.rs` | `leading_spaces` | `line: &str` | helper-of: format_on_type |
| `editing/on_type_formatting.rs` | `find_mapping_colon` | `line: &str` | helper-of: format_on_type |
| `completion.rs` | `build_key_path` | `lines: &[&str]` | helper-of: complete_at |
| `completion.rs` | `build_value_key_path` | `lines: &[&str]` | helper-of: complete_at |
| `completion.rs` | `collect_present_keys_at_indent` | `lines: &[&str]` | helper-of: complete_at |
| `completion.rs` | `classify_cursor` | `line: &str` | helper-of: complete_at |
| `completion.rs` | `suggest_sibling_keys` | `lines: &[&str]` | helper-of: complete_at |
| `completion.rs` | `is_in_sequence_item` | `lines: &[&str]` | helper-of: complete_at |
| `completion.rs` | `suggest_keys_for_sequence_item` | `lines: &[&str]` | helper-of: complete_at |
| `completion.rs` | `collect_current_sequence_item_keys` | `lines: &[&str]` | helper-of: complete_at |
| `completion.rs` | `find_current_item_start` | `lines: &[&str]` | helper-of: complete_at |
| `completion.rs` | `find_sequence_indent` | `lines: &[&str]` | helper-of: complete_at |
| `completion.rs` | `collect_all_sequence_item_keys` | `lines: &[&str]` | helper-of: complete_at |
| `completion.rs` | `collect_sibling_keys` | `lines: &[&str]` | helper-of: complete_at |

#### New carve-outs — pre-parse lexical concerns and whitespace

These functions handle concerns that the CLAUDE.md rule
explicitly exempts:

- Pre-parse lexical concerns (modeline/comment scanning,
  BOM detection)
- Whitespace-preserving edits that don't touch structure

| File | Function | Shape | Carve-out reason |
|---|---|---|---|
| `parser.rs` | `parse_yaml` | `text: &str` | Canonical parser entry point — this IS the "one parser" the rule references |
| `validation/suppression.rs` | `build_suppression_map` | `text: &str` | Pre-parse lexical concern: scans `# rlsp-yaml-disable-*` comments before any YAML parsing |
| `editing/formatter.rs` | `extract_doc_prefix_comments` | `text: &str` | Pre-parse lexical concern: document-prefix comment extraction |
| `editing/formatter.rs` | `find_comment_on_line` | `line: &str` | Comment-boundary scan helper; used by pre-parse document-prefix extraction |
| `editing/formatter.rs` | `content_signature` | `line: &str` | Helper of `find_comment_on_line`; pre-parse lexical |
| `editing/code_actions.rs` | `tab_to_spaces` | `lines: &[&str]` | Whitespace normalization: tabs are YAML 1.2 §6.1 pre-parse lexical; not represented in the AST |

### Allow-list shape after Task 1

The current allow-list has 4 entries. After Task 1 it
will have **48 entries** based on the inventory above:

- 4 existing (carry forward)
- 5 queued code-action retrofits
- 5 feature-level violators (new follow-up plans to file)
- 28 private helpers of flagged roots (`helper-of:`
  marker; helper count drawn from this plan's inventory,
  which may be off by a small number if the broadened
  regex flags a helper the inventory missed — see the
  inventory-reconciliation sub-task in Task 1)
- 6 carve-outs (`// carve-out:` marker)

The count is high but the structure makes it readable: an
entry's marker tells the reader exactly why it is there
and what removes it. Helper-of entries disappear as a
group when their root is retrofitted.

### Per-entry verification

Each new allow-list entry must be verified "load-bearing" —
without the entry, the audit must fail citing that
specific function. The verification protocol:

1. Note the entry's `(file, func)` pair.
2. Edit `parser_boundary_audit.rs` to remove just that one
   entry.
3. Run `cargo test --test parser_boundary_audit`. The test
   must fail. The failure message must include the
   `(file, func)` pair as a violation.
4. Restore the entry. Rerun the test; it passes.
5. Record the verification in the Task 1 commit message.

This protects against regex-miss dead entries — an entry
with no matching function in the source tree is silently
inert.

### References

- Root `CLAUDE.md` "One parser, one AST" rule (under
  Crate Boundaries).
- Current audit:
  `rlsp-yaml/tests/parser_boundary_audit.rs`.
- Follow-up queue entry that authorized this plan:
  `Extend parser_boundary_audit to detect private +
  broader-parameter-name text-scan (audit v2)` in
  `.ai/memory/project_followup_plans.md`.
- Prior allow-list discipline established in
  `.ai/plans/archive/2026-04-18-one-parser-one-ast.md`
  (now archived) — Task 4.

## Steps

- [ ] Broaden the detection regex and parameter-shape
      match; add allow-list entries for every flagged
      function with an explicit marker; per-entry
      load-bearing verification
- [ ] Update `project_followup_plans.md` — file new
      retrofit items for each feature-level violator
      (`hover_at`, `complete_at`, `format_on_type`,
      `find_document_links`, `find_colors`); add a note
      that helper-of entries disappear with their root

## Tasks

### Task 1: Broaden audit detection and install the allow-list

Change `rlsp-yaml/tests/parser_boundary_audit.rs` so the
audit inventories every text-handling function in
`rlsp-yaml/src/`. Keep the first-parameter anchor and the
shrink-only discipline; only the detection surface grows.

- [ ] Replace `is_candidate_fn_line` with a version that
      matches any `(pub )?fn <name>(...)` — no function
      name prefix filter. Drop the
      `validate_*`/`code_actions` filter entirely.
- [ ] Replace `has_text_str_param` with a version that
      matches the first positional parameter (after
      stripping an optional `&[mut ]self` receiver) with:
      - name in `{text, line, lines, content, source,
        input}`
      - type `&str` or `&[&str]`
      Use an anchored regex on the extracted parameter
      block; keep multi-line signature handling.
- [ ] Extend the `AllowEntry` struct with a third field
      to make the marker explicit:
      ```rust
      enum AllowMarker {
          TodoRetrofit { plan: &'static str },
          HelperOf { root: &'static str },
          CarveOut { reason: &'static str },
      }
      ```
      The existing 4 entries become `TodoRetrofit`.
      Display each marker inline in the `Display`
      impl so audit failure messages show the full
      marker.
- [ ] Add 44 new allow-list entries per the Context
      inventory — 5 queued-retrofit, 5 feature-level,
      28 helper-of, 6 carve-out. Each entry's marker must
      match its inventory row.
- [ ] **Inventory reconciliation.** Run the broadened
      audit without any new allow-list entries (just
      the existing 4) and capture the full set of
      violations. Diff against the 44 new entries
      from the inventory above. If the broadened audit
      flags a function not in the inventory, OR if the
      inventory lists a function the audit does not
      flag, the developer stops and messages the lead
      with the surprise finding. The lead escalates to
      the user — the inventory count (48) is a
      user-approved target, and any change requires
      user sign-off. Do not silently append or drop
      entries; do not proceed to the "add 44 new entries"
      sub-task until the discrepancy is resolved.
- [ ] Per-entry load-bearing verification for every new
      entry added (the final count, after
      reconciliation, which is 44 if no surprises).
      For each: temp-remove the entry, run `cargo test
      --test parser_boundary_audit`, confirm failure
      cites `(file, func)`, restore, rerun, confirm
      pass. Record one line per entry in the commit
      message (file + func + marker kind).
- [ ] Update the audit's top-of-file discipline comment
      to describe the new marker taxonomy: shrink-only
      still applies; a new entry is only acceptable when
      classified as a genuine carve-out with
      justification, or when a newly-introduced feature's
      root entry point requires a retrofit plan to be
      filed.
- [ ] Update detection-helper unit tests to cover the new
      shapes:
  - Private `fn` with `text: &str` first param IS detected
  - Private `fn` with `lines: &[&str]` first param IS
    detected
  - Private `fn` with `line: &str` first param IS detected
  - Private `fn` with `content: &str` first param IS
    detected
  - Private `fn` with `source: &str` first param IS
    detected
  - Private `fn` with `input: &str` first param IS
    detected
  - Function with the text param in a non-first position
    is NOT detected (preserve `docs: &[...], text: &str`
    whitelist)
  - Function with a differently-named `&str` first param
    (e.g. `raw: &str`) is NOT detected — only the 6 names
    trigger
  - Update the existing `code_actions_new_signature_not_detected`
    test to confirm the new regex still skips it
- [ ] Build/test gates:
  - `cargo fmt`
  - `cargo clippy --all-targets` zero warnings
  - `cargo test` workspace green
  - `cargo test --test parser_boundary_audit` passes
    with exactly 48 allow-list entries, all load-bearing.

Acceptance: the audit's allow-list contains exactly 48
entries (4 existing + 44 new) using the three-way
`AllowMarker` taxonomy; every entry is load-bearing
(per-entry verification recorded);
detection-helper unit tests cover all 6 parameter names
and both types; full workspace test suite passes; clippy
clean. The audit now fails if any new `(pub )?fn`
matching the new shape is introduced without an explicit
allow-list classification.

### Task 2: File new feature-level retrofit follow-up plans

The broadened audit surfaces 5 feature-level violators
that do not yet have items in the follow-up queue:
`hover_at`, `complete_at`, `format_on_type`,
`find_document_links`, `find_colors`. File a follow-up
item for each so future sessions have a traceable
retrofit task. Also annotate the helper-of convention so
future agents understand that retrofitting the root
automatically clears its helper-of entries.

- [ ] Add a new follow-up item to
      `.ai/memory/project_followup_plans.md` under
      "Open: rlsp-yaml" for each of the 5 feature-level
      violators. Each item must include:
  - Current signature and file:line location
  - Why it violates the rule (what structure it parses
    from text)
  - Rough sketch of the AST-first replacement (which
    parser API to consume — `parse_yaml` result's
    `documents` or `events`)
  - Which of the function's private helpers (listed as
    `helper-of:` entries in the allow-list added in
    Task 1) are retired when the root retrofit lands.
    Name them explicitly so the reader of the follow-up
    item knows the blast radius of the retrofit.
- [ ] Add a short note near the top of the "Open:
      rlsp-yaml" section explaining the `helper-of:`
      allow-list marker convention: a helper-of entry
      exists because its root is allow-listed; the entry
      disappears when the root's retrofit plan lands, not
      through an independent retrofit of the helper.
- [ ] Update the authorizing follow-up item in
      `project_followup_plans.md` (the "Extend
      `parser_boundary_audit` to detect private +
      broader-parameter-name text-scan (audit v2)"
      bullet). Remove the stale "~10-15 new allow-list
      entries" estimate and replace with the actual
      final count (the number from Task 1's commit
      message). This item will later be removed entirely
      when the plan is marked Completed per memory
      convention, but the interim update prevents the
      stale estimate from misleading agents reading the
      queue between Task 2 landing and plan completion.
- [ ] Update `.ai/memory/2026-04-18-rlsp-yaml-architectural-program.md`:
      the file currently states the audit has 4–5 allow-list
      entries and describes the old single-string `note`
      field. Update the relevant passages to state the
      post-Task-1 count (48 entries at baseline) and the new
      `AllowMarker` taxonomy (TodoRetrofit / HelperOf /
      CarveOut). Do not rewrite the whole file; only the
      audit-related paragraphs need editing.
- [ ] Commit the memory updates (both
      `project_followup_plans.md` and
      `2026-04-18-rlsp-yaml-architectural-program.md`)
      with message `chore(memory): queue audit-v2
      feature-level retrofits and update allow-list
      taxonomy`.

Acceptance: `project_followup_plans.md` has 5 new items
for the feature-level violators, each describing the
retrofit shape; the helper-of convention is documented so
future sessions handle allow-list maintenance correctly;
the architectural-program memory reflects the final
allow-list count (48 entries at baseline) and the
`AllowMarker` taxonomy; memory commit lands
with the descriptive message.

## Non-Goals

- **Retrofitting any flagged function** — this plan adds
  the allow-list entries and the inventory. The actual
  AST-first retrofits are separate follow-up plans filed
  in `project_followup_plans.md`.
- **Changing validator/code-action/hover/completion
  behavior** — purely a test-surface and memory-file
  change.
- **Broadening to additional parameter shapes** (e.g.
  `&[String]`, `Vec<String>`) — the queue scope is
  specifically `&str` and `&[&str]` first parameters with
  the 6 listed names.
- **Detecting functions where a text parameter appears in
  a non-first position** — e.g. `fn foo(docs: &[...],
  text: &str, ...)`. The existing regex has an explicit
  first-parameter anchor so that the properly-retrofitted
  `code_actions(docs: &[...], text: &str, ...)` is not
  flagged; that design is preserved.
- **Move 2 fixture pattern** — separate future plan.

## Decisions

- **Scope: enumerate all `rlsp-yaml/src/` private and
  public text-handling helpers.** The follow-up queue
  item is explicit about this scope; narrowing to only
  `code_actions.rs` would leave `hover.rs`,
  `completion.rs`, `decorators/`, and
  `on_type_formatting.rs` uncovered and let them regress
  silently.
- **Marker taxonomy with three kinds (`TodoRetrofit`,
  `HelperOf`, `CarveOut`).** The old single-`note` string
  is lossy — a reader cannot distinguish "queued for
  retrofit" from "carve-out" without reading each note
  carefully. Typed markers make the audit's intent
  mechanical.
- **Helper-of entries stay as individual allow-list rows,
  not aggregated.** Aggregation ("all private helpers of
  `complete_at` are allowed") would require parsing the
  call graph — out of scope for a regex-based test.
  Listing each helper is verbose but simple and
  transparent; a future retrofit of the root deletes its
  helpers' entries together.
- **First-parameter anchor preserved.** The existing
  design (correctly-retrofitted APIs take
  `docs: &[Document<Span>]` first, then `text: &str`
  second as pre-parsed source reference) must continue to
  work. The broadened regex only changes the name-set and
  type-set for the first positional parameter.
- **Carve-out list is explicit and narrow.** Only the six
  functions listed are carve-outs. If a future function
  appears to qualify as a carve-out, the allow-list
  discipline requires an explicit `CarveOut` marker with
  written justification — no implicit carve-outs.
- **`parse_yaml` is a carve-out, not a violator.** It is
  the one parser the rule references; exempting it
  prevents the rule from self-referring.
- **Estimate-vs-actual allow-list count (~10–15 vs 44
  new entries).** The follow-up queue note that authorized
  this plan estimated "~10-15 new allow-list entries."
  Actual enumeration against `rlsp-yaml/src/` produces
  44 new entries (48 total with the existing 4). The gap is explained by two categories
  the original estimate did not anticipate: (a) the five
  feature-level public APIs (`hover_at`, `complete_at`,
  `format_on_type`, `find_document_links`, `find_colors`)
  that also match the broadened regex because they take
  `text: &str` first — the original estimate assumed the
  broadening would mostly catch private helpers within
  `code_actions.rs`; and (b) the 28 private helpers of
  those feature-level roots that the broadened
  `(pub )?fn` match surfaces. The larger count is the
  correct reading of the rule's scope as worded in
  root `CLAUDE.md` ("No code in `rlsp-yaml/` may re-parse
  YAML structure from raw text") — the estimate was
  optimistic, not the inventory.
