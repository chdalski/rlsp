**Repository:** root
**Status:** InProgress
**Created:** 2026-04-20

## Goal

Retrofit `find_document_links` and `find_colors` in `rlsp-yaml`
to consume the parser AST for structural decisions (which text
positions are in scalar-value contexts vs. keys vs. comments)
instead of line-by-line text scanning with hand-rolled
key-vs-value detection. Retire text-scanning helpers, shrink the
`parser_boundary_audit` allow-list, and remove the corresponding
follow-up-queue entries.

## Context

### Current violations

Two `pub fn` entry points in `rlsp-yaml/src/decorators/` split
`text: &str` into lines and scan for patterns without consulting
the AST:

- `decorators/document_links.rs::find_document_links(text: &str, base_uri: Option<&Url>) -> Vec<DocumentLink>`
  — iterates `text.lines()`, invokes `url_links` and `include_links`
  per line. `url_links` runs a URL regex over the raw line.
  `include_links` looks for the literal `!include <path>` text
  using `is_inside_quotes` to skip quoted occurrences.
  `trim_trailing_punctuation` strips `.`/`,`/`)`/`]` etc. from
  matched URLs.
- `decorators/color.rs::find_colors(text: &str) -> Vec<ColorMatch>`
  — iterates `text.lines()`, skips comment-only lines (unless
  they look like hex), uses `value_start_offset(line)` to locate
  the first unquoted `:` and scans the remainder for color
  patterns.

### Prerequisite landed state

No parser changes required. `Node::Scalar.value` is the decoded
string value (post-folding/unescaping), `Node::Scalar.loc` is the
value's source span, and `Node::Scalar.tag` is the node's tag
(including `!include` when present as a YAML tag).

### Text-handling carve-outs (allowed per `rlsp-yaml/CLAUDE.md`)

Comments are not in the AST. The current implementations scan
comment text for URLs (document links) and for hex colors (the
`looks_like_hex_comment` branch). Two options per retrofit:

1. **Drop comment scanning.** Accept the behavior change and
   adjust tests — comments stop producing document links and
   color decorators. Simpler post-retrofit, but a feature
   regression that user-facing tests may flag.
2. **Keep comment scanning as an allow-listed carve-out.** Extract
   a narrow comment-scanner helper that takes raw text and
   returns comment byte-ranges, then apply the URL regex / color
   regex to only those ranges. The helper is allow-listed in
   `parser_boundary_audit.rs` as a pre-parse / non-AST concern.

**Decision for this plan: option 1 (drop comment scanning).**
Rationale: (a) the AST-first program's intent is to remove text
walking, not paper over it with more carve-outs; (b) users who
want comment-based URLs or colors are a niche; (c) the parser's
event stream does emit `Event::Comment` with spans if a future
need arises — that's a clean extension path. If any test
enforces comment-URL or comment-hex behavior, update the test to
assert the new "only scalar values" behavior and document the
drop in Decisions.

### Specifications and consumers

- LSP spec: `DocumentLink` carries a `target: Url` and a `range`;
  `ColorInformation` carries a `Color` and a `range`.
- Server call sites: `rlsp-yaml/src/server.rs` around line 941
  (`find_document_links`) and line 1371 (`find_colors`). Both
  currently read `text` from the document store and pass it; the
  retrofit reads `docs` instead.

### Involved files

- `rlsp-yaml/src/decorators/document_links.rs` — retrofit target
- `rlsp-yaml/src/decorators/color.rs` — retrofit target
- `rlsp-yaml/src/server.rs` — two call sites
- `rlsp-yaml/tests/parser_boundary_audit.rs` — allow-list
- `.ai/memory/project_followup_plans.md` — queue entry removal

### Allow-list target

**Baseline (start of plan):** **65 entries**. This is the
post-Plan-A state (Plan A starts from 74, removes 5 in its Task 1
and 4 in its Task 2, ending at 65 — verified by direct inspection
of the current allow-list).

Verified scoped entries by direct inspection:

- `decorators/document_links.rs`: `find_document_links` (root
  `TodoRetrofit`) + 4 HelperOf (`url_links`, `include_links`,
  `is_inside_quotes`, `trim_trailing_punctuation`). Total: 5
  entries. No `parse_docs` test-fixture entry for this file.
- `decorators/color.rs`: `find_colors` (root `TodoRetrofit`) + 1
  HelperOf (`value_start_offset`). Total: 2 entries.

**Task 1 removes exactly 5 entries.** After Task 1: **60 entries**.

**Task 2 removes exactly 2 entries.** After Task 2: **58 entries**.

If Plan A's actual exit count differs from 65 (e.g. Plan A
discovered additional entries to remove), this plan's
Task 1 baseline adjusts correspondingly — the hard invariant is
that Task 1 removes the 5 named entries and Task 2 removes the 2
named entries, leaving the `ALLOW_LIST` length reduced by
exactly 7 across both tasks. Developer records the pre-edit
baseline for each task in the handoff.

## Steps

- [x] Task 1: retrofit document_links.rs (find_document_links)
- [ ] Task 2: retrofit color.rs (find_colors)

## Tasks

### Task 1: Retrofit decorators/document_links.rs to AST-only

Committed as `ee14566cba20b6272cb6c52d4633760eb75cd6c1` (may be
superseded by follow-up amend for SHA recording). Drop
`text: &str`; walk `Node::Scalar` nodes across all documents;
extract URLs from scalar values and `!include` references from
scalar `tag` field.

- [x] New signature:
      `pub fn find_document_links(docs: &[Document<Span>], base_uri: Option<&Url>) -> Vec<DocumentLink>`.
- [x] Implement: walk every `Document<Span>` recursively. For
      each `Node::Scalar`, examine its `value` with the existing
      `URL_REGEX`; produce `DocumentLink` with the range computed
      as an offset from `node.loc` (URL match offset within
      `value` maps to a column offset within `loc.start.column`
      for single-line scalars; for multi-line scalars, use the
      node's full `loc` span and skip or adjust per the match's
      byte offset). Enforce `MAX_URL_LENGTH` gate.
- [x] `!include` handling: for each `Node::Scalar` where
      `node.tag == Some("!include")`, treat `node.value` as the
      path; resolve against `base_uri` when provided; skip when
      `base_uri` is `None`. Link range = the full `node.loc`.
      If the current implementation treats `!include` only as
      text (not as a YAML tag), the retrofit switches to the
      tag-based detection — verify this produces identical
      results for the existing tests.
- [x] Drop comment URL scanning per this plan's decision (option
      1). Update any test that asserted a URL in a comment line
      to instead assert no link, and note the behavior change in
      the commit message.
- [x] Delete `url_links`, `include_links`, `is_inside_quotes`,
      `trim_trailing_punctuation` from
      `decorators/document_links.rs`. Keep `URL_REGEX` and
      `MAX_URL_LENGTH`. For `calculate_range`: grep the crate
      for callers after implementing the AST walk; if there are
      zero remaining callers, delete it; if the AST walk reuses
      it, keep it. Either outcome is correct; the test is
      "no dead code" (clippy's `dead_code` lint will flag a
      kept-but-unused function).
- [x] Update `rlsp-yaml/src/server.rs` at line ~941 to pass
      `docs.as_deref().unwrap_or(&[])` instead of `&text`.
      Preserve the empty-check and `Ok(None)` short-circuit.
- [x] Update every existing unit test in
      `decorators/document_links.rs` (and any integration test)
      to use the new signature. Parse inputs via
      `rlsp_yaml_parser::load`. No test case may be deleted
      unless supplanted by an equivalent case, EXCEPT for
      comment-scanning tests as noted above — those are
      deliberately removed or inverted with a commit-message note.
- [x] Add rstest regression cases (named): (a) URL in a quoted
      scalar (`url: "https://example.com"`) produces a link
      whose range is inside the quoted scalar's loc; (b) URL in
      a plain scalar (`url: https://example.com`) produces a
      correct range; (c) `!include` tag on a scalar produces a
      link resolved against `base_uri`; (d) URL in a comment
      line produces NO link (asserts the deliberate drop); (e)
      URL exceeding `MAX_URL_LENGTH` is skipped.
- [x] **Before editing:** confirm `ALLOW_LIST` length is **65**
      (after Plan A completes) and the set of entries with
      `file: "decorators/document_links.rs"` matches:
      `find_document_links`, `url_links`, `include_links`,
      `is_inside_quotes`, `trim_trailing_punctuation` (5 total).
      Remove exactly those 5 entries. After removal,
      `ALLOW_LIST` length must be exactly **60**. Allow-list
      may shrink only.
- [x] Remove the follow-up-queue entry for `find_document_links`
      from `.ai/memory/project_followup_plans.md`.
- [x] `cargo test` passes with zero failures.
- [x] `cargo clippy --all-targets` passes with zero warnings.
- [x] `cargo fmt --check` passes.

### Task 2: Retrofit decorators/color.rs to AST-only

Drop `text: &str`; walk `Node::Scalar` nodes; scan scalar values
for color patterns; use `node.loc` for result ranges.

- [ ] New signature:
      `pub fn find_colors(docs: &[Document<Span>]) -> Vec<ColorMatch>`.
- [ ] Implement: walk every `Document<Span>` recursively. For
      each `Node::Scalar`, run the existing color-pattern
      scanner (`scan_line_for_colors` or its equivalent — reuse
      the current matching logic) on `node.value`; for each
      match, construct a `ColorMatch.range` from `node.loc` by
      offsetting the match's byte position within `value` to the
      correct column within `loc.start.column`. Multi-line
      scalars: if the match straddles a line boundary, map the
      offset correctly (use the existing `value_start_offset`
      logic replaced by AST coordinates).
- [ ] Drop comment color scanning (`looks_like_hex_comment`
      branch). Update any test that asserted a color in a
      comment to instead assert no color match.
- [ ] Delete `value_start_offset` and `looks_like_hex_comment`
      (if present) from `decorators/color.rs`. Keep the color
      parsing and range construction logic. `scan_line_for_colors`
      may require signature adjustment to take `&str` +
      `Span` instead of `line_idx` + `col_offset` — adapt to the
      new AST-based coordinate system.
- [ ] Update `rlsp-yaml/src/server.rs` at line ~1371 to pass
      `docs.as_deref().unwrap_or(&[])` instead of `&text`.
      Preserve the `color_decorators_enabled` gate and the
      `ColorInformation` mapping.
- [ ] Update every existing unit test in `decorators/color.rs`
      (and any integration test) to use the new signature. Parse
      inputs via `rlsp_yaml_parser::load`. No test case may be
      deleted unless supplanted by an equivalent case, EXCEPT
      for comment-scanning tests noted above.
- [ ] Add rstest regression cases (named): (a) hex color in a
      value position produces correct range; (b) CSS named color
      in a value position is detected; (c) quoted string color
      value produces correct range; (d) hex color in a comment
      produces NO match (asserts deliberate drop); (e) color in
      a mapping key position is NOT matched (keys aren't value
      positions — AST walk naturally excludes them because keys
      are separate `Node::Scalar` entries but the retrofit must
      not accidentally pick them up; regression-gate this).
- [ ] **Before editing:** confirm `ALLOW_LIST` length is **60**
      (after this plan's Task 1) and the set of entries with
      `file: "decorators/color.rs"` matches: `find_colors`,
      `value_start_offset` (2 total). Remove exactly those 2
      entries. After removal, `ALLOW_LIST` length must be
      exactly **58**. Allow-list may shrink only.
- [ ] Remove the follow-up-queue entry for `find_colors` from
      `.ai/memory/project_followup_plans.md`.
- [ ] `cargo test` passes with zero failures.
- [ ] `cargo clippy --all-targets` passes with zero warnings.
- [ ] `cargo fmt --check` passes.

## Decisions

- **Drop comment scanning.** URL and color detection only runs
  on `Node::Scalar` values. Comments are no longer scanned.
  This is a deliberate behavior change; the AST-first program's
  intent is to consume the parser, not layer more text
  workarounds. Future sessions needing comment awareness should
  consume `Event::Comment` via a dedicated, allow-listed adapter.
- **`!include` via tag, not text.** `find_document_links` detects
  `!include` by checking `Node::Scalar.tag == Some("!include")`
  rather than scanning for the literal text. The parser already
  reifies tags; text-scanning for `!include` is a duplicate
  parser.
- **Color key-position exclusion.** When walking `Node::Mapping.entries`,
  color patterns MUST NOT match on the `key` node — only on the
  `value` node. A naive recursive walk that hits both produces
  false positives for color names used as keys (e.g.
  `red: true`). Add a regression test that gates this.
- **Shrink-only allow-list discipline.** Entries may be removed,
  never added.

## Non-Goals

- Any other retrofit (`complete_at`, `folding_ranges`,
  `semantic_tokens`, `format_on_type`, `document_symbols`,
  `selection_ranges`). Those are in other plans or the queue.
- Any parser changes.
- Color parsing/presentation logic changes — only the scanning
  entry point changes.
- Test dedup / file split — post-program cleanup.
