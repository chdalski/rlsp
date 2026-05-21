**Repository:** root
**Status:** InProgress
**Created:** 2026-05-20

# Split `src/editing/formatter.rs` into per-phase submodules

## Goal

The 2702-line `rlsp-yaml/src/editing/formatter.rs` contains
the YAML formatter end-to-end: configuration
(`YamlFormatOptions`), the two public entry points
(`format_yaml`, `format_subtree`), an AST→`Doc` dispatcher
(`node_to_doc`), per-shape rendering helpers (scalar,
mapping, sequence), comment preservation, duplicate-key
deduplication, and ~150 unit tests in a single `mod tests`
block. Reorganize the file using the project's `foo.rs`
+ adjacent `foo/` directory convention so each phase of the
pipeline lives in its own submodule with its private
helpers and its dedicated unit tests in the same file. The
parent `formatter.rs` becomes a thin orchestrator that
declares the submodules and re-exports the three public
symbols (`YamlFormatOptions`, `format_yaml`,
`format_subtree`) so every existing caller in `server.rs`,
the eight code actions, the benches, and the integration
tests compiles without source changes.

## Context

- **Module-layout convention in this repo:** `foo.rs` plus
  adjacent `foo/` directory (Rust 2018+ style, no
  `mod.rs`). Confirmed by `src/editing/code_actions.rs` +
  `src/editing/code_actions/` and `src/schema_validation.rs`
  + `src/schema_validation/formats.rs`.
- **Source-of-truth file:**
  `rlsp-yaml/src/editing/formatter.rs` (2702 lines).
- **Public surface (must remain reachable at the existing
  paths):**
  - `pub struct YamlFormatOptions` (line 326)
  - `pub fn format_subtree` (line 383)
  - `pub fn format_yaml` (line 422)
- **External callers (paths that must keep working):**
  - `src/server.rs` lines 1037, 1151, 1190, 1261, 1300 —
    uses `crate::editing::formatter::{YamlFormatOptions,
    format_yaml}`
  - `src/editing/code_actions.rs` line 11 (and inline test
    use at line 151) — uses `YamlFormatOptions`
  - Eight code-action submodules in
    `src/editing/code_actions/` use
    `crate::editing::formatter::{YamlFormatOptions,
    format_subtree}` (block_scalar.rs, block_to_flow.rs,
    delete_anchor.rs, flow_to_block.rs, quoted_bool.rs,
    tab_to_spaces.rs, yaml11_bool.rs, yaml11_octal.rs)
  - Integration tests: `tests/code_action_fixtures.rs`,
    `tests/code_action_idempotency.rs`,
    `tests/code_action_property_preservation.rs`,
    `tests/corpus_invariants.rs`,
    `tests/editorconfig_integration.rs`,
    `tests/formatter_conformance.rs`,
    `tests/formatter_fixtures.rs`,
    `tests/formatter_idempotency.rs`
  - Benchmarks: `benches/hot_path.rs`
- **Internal phases and their items:**
  - Options: `pub struct YamlFormatOptions` (line 326),
    `impl Default for YamlFormatOptions` (line 355).
  - Public entries: `pub fn format_subtree` (line 383),
    `pub fn format_yaml` (line 422). The latter is the
    main pipeline orchestrator (~119 lines).
  - AST→Doc dispatcher: `fn node_to_doc` (line 574,
    ~189 lines), `fn flow_item_to_doc` (line 862).
  - Scalar rendering: `fn string_to_doc` (line 834),
    `fn needs_flow_quoting` (line 854),
    `fn needs_quoting` (line 882),
    `fn looks_like_number` (line 959),
    `fn requires_double_quoting` (line 975),
    `fn escape_double_quoted` (line 989),
    `fn repr_block_to_doc` (line 1041),
    `fn is_core_schema_tag` (line 763),
    `fn format_tag` (line 773).
  - Mapping rendering: `fn mapping_to_doc` (line 1163),
    `fn flow_mapping_to_doc` (line 1193),
    `fn needs_explicit_key` (line 1247),
    `fn is_empty_key` (line 1278),
    `fn key_needs_space_before_colon` (line 1306),
    `fn explicit_key_to_doc` (line 1326),
    `fn key_value_to_doc` (line 1417),
    `fn prepend_collection_properties` (line 792).
  - Sequence rendering: `fn sequence_to_doc` (line 1546),
    `fn flow_sequence_to_doc` (line 1572),
    `fn sequence_item_to_doc` (line 1589).
  - Comment preservation: `struct Comment` (line 24),
    `fn find_comment_on_line` (line 34),
    `fn extract_doc_prefix_comments` (line 541),
    `fn attach_comments` (line 172).
  - Content-line tracking: `struct ContentEntry` (line 73),
    `fn content_signature` (line 64),
    `fn last_content_line_from_ast` (line 93),
    `fn last_content_line_idx` (line 157).
  - Duplicate-key dedup: `fn dedup_key_str` (line 1690),
    `fn dedup_mapping_keys` (line 1708).
- **Test groupings inside the single `mod tests` block
  (lines 1753–2702, ~150 tests):**
  - End-to-end format_yaml smoke tests, anchor/alias
    preservation tests, document-marker tests, and
    format_subtree behavior tests exercise the full
    pipeline through the public entry points.
  - Escape/quoting tests exercise the scalar helpers
    (`escape_double_quoted`, `needs_quoting`).
  - Dedup tests (~35 tests) exercise `dedup_mapping_keys`
    and `dedup_key_str`.
- **Test colocation rule (from the user):** every `mod
  tests` unit-test block must live in the same file as the
  function(s) it exercises. The single monolithic `mod
  tests` block is split per-phase during extraction —
  tests that exercise only the scalar helpers move into
  the scalar module, dedup tests move into the dedup
  module, full-pipeline tests stay with `format_yaml` /
  `format_subtree`.
- **Test routing rule (from the lsp_lifecycle split
  retrospective):** when extracting tests, decide each
  test's destination by what its body asserts — not by
  its name and not by its position in the file. Test
  groupings identified in this Context section are
  starting points; if a test in the original `mod tests`
  block calls a phase-specific helper directly, route it
  to that phase even if its name suggests otherwise. Do
  not leave a stray test stranded in `formatter.rs` just
  because its name doesn't match a phase.
- **Cross-module visibility:** sibling modules need to call
  each other (e.g., `node_to_doc` calls every per-shape
  helper). Move sibling-visible helpers with `pub(super)`
  visibility so they are reachable from siblings without
  becoming part of the crate-public API.
- **Build/test commands (from CLAUDE.md):** `cargo build`,
  `cargo test`, `cargo clippy --all-targets`, `cargo fmt`,
  `cargo bench` (do not run by default; the benches
  reference the public formatter API and must compile
  cleanly).

## Steps

- [x] Extract `options` and `scalar_render`
- [x] Extract `dedup`
- [x] Extract `sequence_render` and `mapping_render`
- [x] Extract `comment_preservation` and `content_tracking`
- [x] Extract `node_to_doc` (the AST→Doc dispatcher)
- [ ] Verify `formatter.rs` is orchestration only and every
      external caller continues to compile unchanged

## Tasks

### Task 1: Extract `options` and `scalar_render`

Create `src/editing/formatter/` and add the first two
submodules. `options` holds the formatter configuration;
`scalar_render` holds scalar-value rendering. Both are
leaves of the call graph. This task also updates two
documentation cross-references that name the pre-split
location of `YamlFormatOptions`.

- [x] `src/editing/formatter/options.rs` exists and
      contains:
  - `pub struct YamlFormatOptions`
  - `impl Default for YamlFormatOptions`
  - any `#[cfg(test)] mod tests` block holding unit tests
    that exercise only the option defaults or option
    field defaults (no scalar/mapping/sequence behavior)
- [x] `src/editing/formatter/scalar_render.rs` exists and
      contains the nine scalar helpers listed under
      "Scalar rendering" in Context, plus a
      `#[cfg(test)] mod tests` block holding the
      `escape_double_quoted_escapes`,
      `needs_quoting_returns_true`,
      `needs_quoting_returns_false`, and
      `looks_like_number`-style unit tests from the
      original `mod tests` block
- [x] `src/editing/formatter.rs` declares
      `pub mod options;` and `mod scalar_render;`
- [x] `src/editing/formatter.rs` re-exports
      `pub use options::YamlFormatOptions;`
- [x] `src/editing/formatter.rs` no longer defines the
      moved items or contains the moved unit tests
- [x] `/workspace/CLAUDE.md` line 95 (the "Settings Sync"
      table row) names
      `` `YamlFormatOptions` in
      `src/editing/formatter/options.rs` `` as the source
      of truth — the pre-split path `formatter.rs` is no
      longer cited
- [x] `/workspace/rlsp-yaml/tests/fixtures/CLAUDE.md`
      lines 13–14 reference
      `rlsp-yaml/src/editing/formatter/options.rs` as the
      file to read `YamlFormatOptions` from — the
      pre-split path is no longer cited
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` reports the same total test count as
      the pre-task baseline; record both numbers in the
      commit message
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `62b4bb0` (amended; see `git log --follow rlsp-yaml/src/editing/formatter/options.rs`)

### Task 2: Extract `dedup`

`dedup_mapping_keys` is invoked once from `format_yaml`
under the `remove_duplicate_keys` option. The two
functions (`dedup_key_str`, `dedup_mapping_keys`) have ~35
dedicated unit tests in the original `mod tests` block.

- [x] `src/editing/formatter/dedup.rs` exists and contains:
  - `fn dedup_key_str` (visibility `pub(super)` so
    `format_yaml` can call it indirectly through
    `dedup_mapping_keys`)
  - `pub(super) fn dedup_mapping_keys`
  - a `#[cfg(test)] mod tests` block holding every
    `dedup_*` unit test from the original `mod tests`
    block, including the `dedup_opts` test helper
- [x] `src/editing/formatter.rs` declares `mod dedup;` and
      uses `dedup::dedup_mapping_keys` where it previously
      called the local function
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `c5f0cb5` (amended; see `git log --follow rlsp-yaml/src/editing/formatter/dedup.rs`)

### Task 3: Extract `sequence_render` and `mapping_render`

The original `mod tests` block contains no `#[test]`
function that invokes `sequence_to_doc`,
`flow_sequence_to_doc`, `sequence_item_to_doc`,
`mapping_to_doc`, `flow_mapping_to_doc`,
`needs_explicit_key`, `is_empty_key`,
`key_needs_space_before_colon`, `explicit_key_to_doc`,
`key_value_to_doc`, or `prepend_collection_properties`
directly. Coverage of mapping and sequence rendering is
provided end-to-end through `format_yaml` and
`format_subtree` tests (anchor/alias preservation,
document markers, format_yaml_multi_contains,
format_subtree_*) which stay in the parent file. Both new
submodules therefore contain no `#[cfg(test)] mod tests`
block.

- [x] `src/editing/formatter/sequence_render.rs` exists
      and contains exactly:
  - `pub(super) fn sequence_to_doc`
  - `pub(super) fn flow_sequence_to_doc`
  - `pub(super) fn sequence_item_to_doc`
  - no `#[cfg(test)] mod tests` block (no test in the
    original block targets these helpers directly)
- [x] `src/editing/formatter/mapping_render.rs` exists and
      contains exactly:
  - `pub(super) fn mapping_to_doc`
  - `pub(super) fn flow_mapping_to_doc`
  - `pub(super) fn needs_explicit_key`
  - `pub(super) fn is_empty_key`
  - `pub(super) fn key_needs_space_before_colon`
  - `pub(super) fn explicit_key_to_doc`
  - `pub(super) fn key_value_to_doc`
  - `pub(super) fn prepend_collection_properties`
  - no `#[cfg(test)] mod tests` block (no test in the
    original block targets these helpers directly)
- [x] `src/editing/formatter.rs` declares `mod
      sequence_render;` and `mod mapping_render;`
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `bf79bd0` (amended; see `git log --follow rlsp-yaml/src/editing/formatter/mapping_render.rs`)

### Task 4: Extract `comment_preservation` and `content_tracking`

Comment preservation is orthogonal to value rendering and
runs as a separate pipeline phase inside `format_yaml`.
Content-line tracking is its dependency.

The original `mod tests` block contains no `#[test]`
function that invokes `find_comment_on_line`,
`extract_doc_prefix_comments`, `attach_comments`,
`content_signature`, `last_content_line_from_ast`, or
`last_content_line_idx` directly. Coverage of comment and
content-line behavior is provided end-to-end through
`format_yaml` tests (which retain inline-comment and
prefix-comment cases) that stay in the parent file. Both
new submodules therefore contain no `#[cfg(test)] mod
tests` block.

- [x] `src/editing/formatter/content_tracking.rs` exists
      and contains exactly:
  - `struct ContentEntry`
  - `pub(super) fn content_signature`
  - `pub(super) fn last_content_line_from_ast`
  - `pub(super) fn last_content_line_idx`
  - no `#[cfg(test)] mod tests` block (no test in the
    original block targets these helpers directly)
- [x] `src/editing/formatter/comment_preservation.rs`
      exists and contains exactly:
  - `struct Comment`
  - `pub(super) fn find_comment_on_line`
  - `pub(super) fn extract_doc_prefix_comments`
  - `pub(super) fn attach_comments`
  - no `#[cfg(test)] mod tests` block (no test in the
    original block targets these helpers directly)
- [x] `src/editing/formatter.rs` declares both submodules
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline (6217 formatter tests + 2 new audit
      scanner detection unit tests = 6219)
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `2702b4d` (amended; see `git log --follow rlsp-yaml/src/editing/formatter/comment_preservation.rs`)

Note: this task also updated `rlsp-yaml/tests/parser_boundary_audit.rs`
to widen the audit scanner regex to recognise `pub(super) fn` /
`pub(crate) fn` (the moved helpers' new visibility forms), update the
three relocated CarveOut entries' file paths, and classify eight
pre-existing functions the previous bare-`fn` regex did not detect.
This is a strict deviation from plan Non-Goal #5 ("Modifying any
integration test or benchmark") but was unavoidable to keep the audit
enforcing the One-Parser-One-AST rule for the relocated helpers — the
alternative (leaving the audit broken) is worse than the deviation.
Two new audit scanner unit tests (`pub_super_fn_detected`,
`pub_crate_fn_detected`) accompany the regex widening.

### Task 5: Extract `node_to_doc`

`node_to_doc` is the AST→`Doc` dispatcher and the largest
internal helper (~189 lines). It calls into every
per-shape module already extracted.

- [x] `src/editing/formatter/node_to_doc.rs` exists and
      contains:
  - `pub(super) fn node_to_doc`
  - `pub(super) fn flow_item_to_doc`
  - imports from sibling modules
    (`use super::scalar_render::*;`,
    `use super::mapping_render::*;`,
    `use super::sequence_render::*;`)
- [x] `src/editing/formatter.rs` declares `mod
      node_to_doc;` and uses
      `node_to_doc::node_to_doc` from `format_yaml` and
      `format_subtree`
- [x] `cargo build` succeeds without new warnings
- [x] `cargo test` total test count matches the previous
      task's baseline
- [x] `cargo clippy --all-targets -- -D warnings` passes
- [x] `cargo fmt --check` passes

Commit: `662b5f7` (amended; see `git log --follow rlsp-yaml/src/editing/formatter/node_to_doc.rs`)

### Task 6: Verify orchestration-only `formatter.rs`

This task is primarily a verification of the post-Task-5
state. If Tasks 1–5 already produced an orchestration-only
parent file, Task 6 will have no source diff for
`formatter.rs` itself (beyond the CLAUDE.md
cross-reference updates that may still be pending if not
already done in Task 1). Submit a verification-only
handoff documenting the measured criteria
(grep/ls/cargo/bench-compile command outputs) and the
plan-progress update only. If verification reveals any
leftover internal helpers, missing re-exports, or stranded
unit tests in `formatter.rs`, fix them and report what
changed.

After all extractions, `src/editing/formatter.rs` contains
only:

- a module-level doc comment
- `pub mod options; mod scalar_render; mod dedup; mod
  sequence_render; mod mapping_render; mod
  content_tracking; mod comment_preservation; mod
  node_to_doc;`
- `pub use options::YamlFormatOptions;`
- `pub fn format_subtree` (~39 lines, the public entry
  point that delegates to `node_to_doc::node_to_doc`)
- `pub fn format_yaml` (~119 lines, the public pipeline
  orchestrator that wires
  comment_preservation/content_tracking/dedup/node_to_doc
  together)
- a `#[cfg(test)] mod tests` block holding only the
  end-to-end tests that exercise `format_yaml` or
  `format_subtree` as a whole — anchor/alias preservation
  tests, document-marker tests, format_yaml_multi_contains,
  format_subtree_* tests

No internal helper `fn` items, no `struct`/`enum` items,
no per-phase unit tests remain in `formatter.rs`.

- [ ] `src/editing/formatter.rs` contains exactly two
      `pub fn` items (`format_subtree`, `format_yaml`),
      one `pub use` re-export
      (`pub use options::YamlFormatOptions;`), eight `mod`
      declarations, and one `#[cfg(test)] mod tests`
      block; nothing else at the item level
- [ ] Every sibling `.rs` file under
      `src/editing/formatter/` corresponds to a `mod
      <name>;` declaration in `formatter.rs`, and every
      declaration corresponds to an existing sibling file
- [ ] `cargo build` succeeds without new warnings
- [ ] `cargo test` reports the same total test count as
      the pre-Task-1 baseline; record both numbers in the
      commit message
- [ ] `cargo bench --no-run` succeeds (compile-only check;
      do not execute benches by default)
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes
- [ ] No external caller listed in Context was modified
      (`git diff --stat` shows only `formatter.rs` and new
      submodule files under `formatter/`)
- [ ] `/workspace/CLAUDE.md` references
      `src/editing/formatter/options.rs` as the source of
      truth for `YamlFormatOptions`; the pre-split path
      `formatter.rs` no longer appears in line 95 of that
      file (this verification stands independently of
      whether Task 1 already applied the update)
- [ ] `/workspace/rlsp-yaml/tests/fixtures/CLAUDE.md`
      references
      `rlsp-yaml/src/editing/formatter/options.rs` as the
      file to read `YamlFormatOptions` from; the pre-split
      path no longer appears in lines 13–14 of that file
      (this verification stands independently of whether
      Task 1 already applied the update)

## Decisions

- **Module-layout convention:** `formatter.rs` becomes the
  module-entry file alongside a new
  `src/editing/formatter/` directory containing the
  submodules. Matches `src/editing/code_actions.rs` +
  `src/editing/code_actions/`.
- **Public API preservation:** the three public symbols
  (`YamlFormatOptions`, `format_yaml`, `format_subtree`)
  stay reachable at their current paths
  (`crate::editing::formatter::*` and
  `rlsp_yaml::editing::formatter::*`). `YamlFormatOptions`
  is re-exported from the `options` submodule;
  `format_yaml` and `format_subtree` stay defined in
  `formatter.rs` itself because they are the pipeline
  orchestrators and the natural place to wire the phases
  together.
- **Phase-based slicing:** the formatter is a pipeline
  with discrete phases (options → comment extraction →
  dedup → AST→Doc → text). Each phase becomes a sibling
  module; the public entry points stay in the parent and
  do the wiring.
- **`pub(super)` visibility for internal helpers:** all
  internal helpers move with `pub(super)` so siblings
  (notably `node_to_doc`) can call them, but they are not
  part of the crate-public API.
- **Test colocation:** scalar/dedup/mapping/sequence
  helpers carry their own unit tests in their own
  submodule. End-to-end tests that drive `format_yaml`
  through anchors, document markers, or full-pipeline
  smoke checks stay in the parent file because they
  exercise the orchestration, not any single helper.
- **No incremental shim file:** the parent file
  `formatter.rs` stays present throughout; only new files
  are added and old contents are removed in-place.
- **Caller-path references in Context may shift:** the
  Context section names
  `tests/corpus_invariants.rs` line 42 and several other
  test files. The sibling plan
  `2026-05-20-split-corpus-invariants-tests.md` renames
  the corpus_invariants test file into a directory with
  submodules; if it runs before this plan, the `use
  rlsp_yaml::editing::formatter::*` statement moves to a
  submodule under `tests/corpus_invariants/`. The
  acceptance criterion "No external caller listed in
  Context was modified" still holds because the public
  API path (`rlsp_yaml::editing::formatter::*`) does not
  change.
- **`rlsp-yaml/README.md` is not updated:** the README's
  "Architecture" section (lines 186–208) is a conceptual
  module map, not a literal file tree — it already lists
  several modules (e.g. `validators.rs`, `code_actions.rs`,
  `hover.rs`) as if they sat directly under `src/` even
  though several already live in subdirectories. The
  description documents what each module does for callers,
  not what its file contains; after the split, the
  formatter module continues to deliver the same behavior
  through its re-exports, so the README description
  remains accurate.
- **Two CLAUDE.md cross-references DO update:** the
  Settings Sync table at `/workspace/CLAUDE.md` line 95
  and the fixture-spec at
  `/workspace/rlsp-yaml/tests/fixtures/CLAUDE.md` lines
  13–14 both name a *specific file path* as the
  source-of-truth definition of `YamlFormatOptions`. After
  the split, the struct definition moves to
  `src/editing/formatter/options.rs`; a reader following
  the existing path would only find a re-export. Both
  references are updated in Task 1 alongside the
  extraction.

## Non-Goals

- Changing formatter behavior, output bytes, or
  configuration semantics. This plan only reorganizes
  source layout.
- Adding new formatter options, deprecating existing
  options, or modifying defaults.
- Refactoring the `Doc` IR (lives in the `rlsp-fmt`
  crate).
- Splitting `src/editing/code_actions.rs` or any code
  action module.
- Splitting any other source file
  (`src/validation/validators.rs`,
  `src/schema_validation.rs`, `src/completion.rs`, etc.)
  — covered by separate plans.
- Modifying any integration test or benchmark.
