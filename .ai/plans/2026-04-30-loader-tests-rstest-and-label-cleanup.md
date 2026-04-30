**Repository:** root
**Status:** Completed (2026-04-30)
**Created:** 2026-04-30

## Goal

Convert the structurally-repetitive `#[test]` functions in
`rlsp-yaml-parser/src/loader.rs`'s test module to
parameterized `rstest` cases with named cases, and remove
stale labels (`Bug A` / `Bug B` section headers, the
`(Bug A)` / `(Bug B)` annotations in DISP assertion
messages, and the `UT-*-N` / `COW-UT-N` / `DISP-*-N` /
`LOAD-META-N` ID prefixes scattered through comments) that
no longer convey useful information now that the bugs are
shipped and the function names already describe what each
test verifies.

The aim is twofold: collapse 36 standalone tests into 7
parameterized functions while keeping every scenario
greppable via `#[case::name]`, and make the test source
read on its own merits without the temporal scaffolding
that bug-fix and test-list authoring left behind.

## Context

- `rstest = "0.26"` is already a dev-dep
  (`rlsp-yaml-parser/Cargo.toml:19`) and is used heavily
  in `rlsp-yaml-parser/src/schema.rs` (lines 481+, ~30
  invocations) — the pattern is established in this
  crate.
- The DISP test group (loader.rs lines 1895-2135) was
  added in commit `c972d32` to regression-test the
  combined-property block mapping fix. Two of the 17
  tests carry `(Bug A)` / `(Bug B)` annotations naming
  the bugs that motivated them; that lineage now lives
  in the commit message, not the source.
- Older test groups carry the same rot:
  - `// Bug A: comment between mapping key and its
    collection value` (loader.rs:1167-1169) — section
    header from a prior fix
  - `// Bug B: comment at end of collection preserved
    as leading on next sibling` (loader.rs:1281-1283) —
    section header from a prior fix
  - ID prefixes like `UT-A1:`, `UT-S1:`, `COW-UT-3:`,
    `DISP-BM-2:`, `LOAD-META-4:` in inline comments —
    diagnostic numbering left from when the test list
    was authored. The function names below them already
    describe what each test verifies, so the prefixes
    are redundant.
- `lang-rust-testing.md` rule (loaded automatically)
  governs rstest consolidation:
  - Use `#[case::name]` syntax so each case stays
    individually greppable and named in failure output
  - When assertion shapes diverge within a group, split
    into multiple parametrized functions named after
    the assertion shape, rather than synthesizing a
    unified return type
- `risk-assessment.md` lists "Test-only changes —
  adding or modifying tests without changing production
  code" as a skip-advisor case. This task touches only
  the `#[cfg(test)] mod tests` block in `loader.rs`; no
  production behavior changes.

## Steps

- [x] Survey complete (consolidation map captured in
      Tasks section below)
- [x] Capture baseline test count: run
      `cargo test -p rlsp-yaml-parser loader::tests --
      --list` and record the number of test cases
      reported in `loader::tests` (baseline: 62)
- [x] Convert UT-D, UT-S, COW-UT, LOAD-META, and DISP
      groups to rstest per the consolidation map
- [x] Strip stale labels from section headers, assertion
      messages, and inline comments
- [x] Verify post-refactor test count (70 — increase of
      8 reflects the DISP block-form split per the
      consolidation map; the original 8 block tests each
      had two assertion shapes and split into two
      parametrized functions)
- [x] Verify `cargo test -p rlsp-yaml-parser loader::tests`
      green
- [x] Verify `cargo clippy --all-targets` clean

## Tasks

### Task 1: rstest consolidation and stale-label cleanup in loader test module

**Commit:** `5570ad6`

Refactor the `#[cfg(test)] mod tests` block in
`rlsp-yaml-parser/src/loader.rs` to:

1. Consolidate five test groups into parameterized
   `rstest` functions per the map below
2. Remove stale labels that reference bugs and ID
   prefixes that no longer carry meaning

**Consolidation map.** Each row names the group, the
source line range, the test functions to consolidate, the
target rstest function name(s), and which tests stay
standalone (and why). Function names below are
descriptive — `#[case::<name>]` cases preserve each
original test's intent.

| Group | Lines | Standalone tests to consolidate | Target rstest | Cases | Stays standalone |
|---|---|---|---|---|---|
| UT-D | 1497-1555 | `bare_document_has_both_flags_false`, `document_with_start_marker_has_explicit_start_true`, `document_with_end_marker_has_explicit_end_true`, `document_with_both_markers_has_both_flags_true`, `empty_document_with_explicit_markers_has_both_flags_true` | `document_marker_flags_match_input` | 5 named cases | `multi_document_flags_are_independent` (UT-D5) — different shape: array of per-doc flags |
| UT-S escapes | 1561-1604 | `sanitize_newline_replaced_with_escape`, `sanitize_carriage_return_replaced_with_escape`, `sanitize_null_byte_replaced_with_escape` | `sanitize_replaces_control_char_with_escape` | 3 named cases | UT-S4..S7 (truncation tests, 1606-1671) — different assertion shape (length/ellipsis checks) |
| COW-UT borrowed | 1681-1788 | `resolver_injected_str_tag_is_borrowed`, `resolver_injected_int_tag_is_borrowed`, `resolver_injected_null_tag_is_borrowed`, `resolver_injected_map_tag_is_borrowed`, `resolver_injected_seq_tag_is_borrowed`, `bare_excl_tag_resolver_path_is_borrowed` | `resolver_emitted_tag_is_borrowed` | 6 named cases | — |
| COW-UT owned | 1741-1775 | `user_authored_tag_on_scalar_is_owned`, `user_authored_tag_on_mapping_is_owned`, `user_authored_tag_on_sequence_is_owned` | `user_authored_tag_is_owned` | 3 named cases | `alias_node_has_no_tag_field` (UT-10), `tag_value_content_preserved_across_cow_variants` (UT-11) — different shapes |
| LOAD-META no-meta | 1832-1878 | `loaded_plain_scalar_has_no_meta`, `loaded_mapping_with_no_meta_fields_has_meta_none`, `loaded_sequence_with_no_meta_fields_has_meta_none` | `loaded_node_with_no_meta_fields_has_meta_none` | 3 named cases | `loaded_anchored_scalar_has_meta_some` (META-2), `loaded_scalar_with_anchor_has_meta_some_with_anchor_loc` (META-5) — additional assertions beyond meta-None check |
| DISP root | 1901-2120 | All 16 `*_anchor_only_*`, `*_tag_only_*`, `*_anchor_then_tag_*`, `*_tag_then_anchor_*` tests across BM/BS/FM/FS | `combined_properties_attach_to_root_collection` | 16 named cases | — |
| DISP block first-child | 1901-2036 | First-child assertion bodies inside the 8 block-mapping and block-sequence tests above (currently split across the same 8 functions) | `first_child_of_block_collection_has_no_properties` | 8 named cases | DISP-FM/FS have no first-child assertions; this function covers only block forms |
| DISP alias | 2127-2135 | — | — | — | `anchor_on_block_mapping_with_combined_properties_resolves_via_alias` (DISP-ALIAS-1) — uses `LoaderBuilder` and alias resolution; different mechanism |

The eight block-form DISP tests currently each contain
two assertion shapes: a root-properties check and a
first-child-clean check. The refactor splits them: the
root-properties check moves into the 16-case
`combined_properties_attach_to_root_collection`
function; the first-child-clean check moves into the
8-case `first_child_of_block_collection_has_no_properties`
function. After the move the original 8 standalone
functions are deleted, since both halves of their
assertions are now covered by the two parametrized
functions.

**Groups that stay as standalone tests** (assertion
shapes diverge — parametrizing would obscure intent):

- **UT-1/UT-2/UT-3 loader state** (1100-1165) — three
  unrelated scenarios
- **UT-A comment-between-key-and-collection** (1172-1278)
  — five tests, each navigates a different AST path
- **UT-B overflow-comments** (1286-1404) — five distinct
  propagation scenarios
- **UT-C combined scenarios** (1411-1490) — three unique
  end-to-end fixtures

Their `Bug A:` / `Bug B:` section headers and `UT-A1:` /
`UT-B2:` / `UT-C3:` ID prefixes still get cleaned up
(see label-stripping list below) even though the test
bodies do not change.

**Label-stripping list.** Within the test module, remove
or rewrite the following, replacing them with descriptive
text where the surrounding comment carries useful
information:

- Section header `// Bug A: comment between mapping key
  and its collection value` (loader.rs:1167-1169) →
  `// Comment between mapping key and nested collection
  is attached to first nested entry`
- Section header `// Bug B: comment at end of collection
  preserved as leading on next sibling`
  (loader.rs:1281-1283) → `// Trailing comment of nested
  collection becomes leading comment on next sibling`
- Inline `(Bug A)` / `(Bug B)` annotations in DISP
  assertion messages (loader.rs:1946, 1961, 1966) — drop
  the parenthetical, keep the descriptive part of the
  message
- ID-prefix inline comments throughout the test module —
  remove the leading `UT-A1:` / `UT-S3:` / `COW-UT-7:` /
  `DISP-BM-3:` / `LOAD-META-4:` token. If the comment
  has no other content (just the ID and the function
  name in a sentence), delete it entirely; the function
  name says what the test does.
- Group divider comments like `// --- Group 1: Block
  mapping ---` inside the DISP section (loader.rs:1901,
  1970, 2038, 2080, 2122) — keep the dividers themselves
  for readability, but drop the "Group N:" numbering
  prefix; "Block mapping" alone is the section name.
- "after the fix" / "Bug A regression test" / "Bug B
  regression test" inline annotations (loader.rs:1932,
  1950) — remove the temporal references, keep the
  descriptive part of the test purpose.

**Acceptance criteria.** All measured against the test
module in `rlsp-yaml-parser/src/loader.rs`:

- [x] `cargo test -p rlsp-yaml-parser loader::tests`
      passes (70 tests). **Note:** the literal count
      went from 62 to 70 (+8), reflecting the DISP
      block-form split documented in the Consolidation
      Map — the 8 BM/BS tests each carried two
      assertion shapes and split into two parametrized
      functions. The plan's "matches baseline" wording
      was internally inconsistent with its own split
      design; the reviewer confirmed the math.
- [x] `cargo clippy --all-targets` reports zero warnings
- [x] `cargo fmt --check` reports clean
- [x] No occurrence of the literal strings `Bug A`,
      `Bug B`, `Bug-A`, `Bug-B` anywhere in
      `loader.rs` (verified — zero hits)
- [x] No occurrence of ID-prefix patterns `UT-A[0-9]`,
      `UT-B[0-9]`, `UT-C[0-9]`, `UT-D[0-9]`,
      `UT-S[0-9]`, `COW-UT-[0-9]`, `LOAD-META-[0-9]`,
      `DISP-(BM|BS|FM|FS|ALIAS)-[0-9]` in the test
      module (verified — zero hits)
- [x] Each rstest function uses `#[case::name(...)]`
      syntax (named cases, not bare `#[case(...)]`);
      verified via `grep '#\[case\('` — zero hits
- [x] All assertions and input strings preserved
      exactly. Reviewer noted one minor *strengthening*
      in `first_child_of_block_collection_has_no_properties`:
      the original 8 BM/BS tests asserted only one of
      `anchor()` / `tag_loc()` per case; the
      consolidated function asserts both for all 8
      cases (strengthening, not weakening — all tests
      pass).

## Decisions

- **No advisor consultation.** Test-only refactor with
  no production code change and no new behavior; falls
  squarely under `risk-assessment.md`'s "skip advisors"
  category. The pattern (rstest with named cases) is
  already established in `rlsp-yaml-parser/src/schema.rs`,
  so no new testing convention is being introduced.
- **One task, not five.** The consolidation map covers
  five groups, but they share one file, one mechanical
  pattern, and one verification step. Splitting into
  per-group tasks would produce five tiny commits with
  five rounds of CI overhead for no review benefit;
  diffs at the group level are still independently
  legible inside one commit.
- **Non-rstest groups still get label cleanup.** The
  UT-A, UT-B, UT-C, and UT-1/2/3 groups stay structured
  as standalone `#[test]` functions, but their
  `Bug A:` / `Bug B:` section headers and `UT-A1:` ID
  prefixes are dead weight regardless of structure.
  Cleaning them in the same commit avoids leaving the
  same rot in part of the file after the rest is fixed.
- **Original ID prefixes are not preserved as case
  names.** A naive translation would map `UT-S1` to
  `#[case::ut_s1(...)]`, but the IDs were never
  meaningful — they were authoring-time numbering. The
  `#[case::name]` slug should describe the scenario
  (e.g. `#[case::newline_to_u000a]`), matching what the
  function name conveyed.

## Non-Goals

- No production code changes. The fix in `step.rs`
  (commit `c972d32`) and the existing `loader.rs`
  loader logic are unchanged.
- No new test scenarios. This is a structural refactor;
  every input/output pair from the existing tests must
  appear unchanged in the consolidated form.
- No changes to test groups whose assertion shapes
  diverge (UT-1/2/3, UT-A, UT-B, UT-C). The survey
  explicitly concluded their tests parameterize poorly;
  forcing it would obscure intent. Only their stale
  labels get cleaned up.
- No file split. The test module stays inline in
  `loader.rs`; this plan does not move tests to a
  `tests/` integration crate or to a sibling module
  file.
- No update to `architecture.md` or any other doc. The
  refactor is internal to the test module and does not
  change the documented behavior.
