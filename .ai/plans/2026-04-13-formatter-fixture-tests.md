**Repository:** root
**Status:** InProgress
**Created:** 2026-04-13

## Goal

Replace the ~70 inline formatter unit tests in
`formatter.rs` and ~11 ecosystem round-trip tests in
`ecosystem_fixtures.rs` with markdown-based fixture files.
Each fixture defines input YAML, optional settings, and
expected output in a human-readable format that doubles as
a bug-report template. A Rust test harness iterates over
fixture files using rstest's `#[files]` glob, similar to
the parser's conformance test harness in
`rlsp-yaml-parser/tests/conformance.rs`.

## Context

### Current state

- `formatter.rs` has 70 unit tests (lines 871-2182) that
  all call `format_yaml(input, &opts)` and assert on the
  output. They are inline Rust with string constants.
- `ecosystem_fixtures.rs` has 23 tests. Of these, ~11 are
  round-trip/idempotency tests calling `format_yaml`
  directly. The remaining ~12 test diagnostics
  (`validate_duplicate_keys`, `validate_flow_style`) and
  specific assertions that do not fit the fixture format.
- `lsp_lifecycle.rs` has 91 LSP protocol tests — these are
  out of scope (they need the full request/response cycle).
- The parser conformance suite
  (`rlsp-yaml-parser/tests/conformance.rs`) provides the
  pattern: rstest `#[files]` iterates over data files, a
  loader parses each file, assertions run per case.

### Fixture format (agreed with user)

One test per file, stored in a single flat directory
`rlsp-yaml/tests/fixtures/formatter/`. Each file is
markdown with YAML frontmatter:

```markdown
---
test-name: descriptive-kebab-case-name
category: quoting | flow-style | comments | blank-lines |
          structure | idempotency | scalars | ecosystem |
          enforce-block-style
settings:
  single_quote: true
  print_width: 40
  # any YamlFormatOptions field; omitted = default
idempotent: true  # optional: skip Expected-Document,
                  # just assert format(format(input)) == format(input)
---

# Test: Descriptive Name

Optional prose description of the behavior being tested.
For tests that assert spec-defined behavior, include
references inline — one `Ref:` line per source. Examples:

Ref: YAML 1.1 §10.2.1.2 — Boolean tag resolution
Ref: K8s API — status field is optional empty map
Ref: GHA workflow syntax — on: is a mapping key

## Test-Document

```yaml
input: yaml here
```

## Expected-Document

```yaml
expected: output here
```
```

### Design decisions

- **One test per file** — maps naturally to bug reports,
  file names are grep-able, no ambiguity about which test
  failed
- **Frontmatter settings map to `YamlFormatOptions`** —
  the Rust struct is the schema; no separate schema file
- **Category in frontmatter** — enables filtering without
  encoding in filenames
- **`idempotent` flag** — for round-trip tests, the
  harness formats twice and asserts stability; no
  Expected-Document section needed
- **Flat directory** — start simple, split later if count
  grows unwieldy

### What stays as Rust

- `ecosystem_fixtures.rs` diagnostic tests
  (`assert_no_false_positives`) — they call validator
  functions directly, not the formatter
- All 91 `lsp_lifecycle.rs` tests — need full LSP protocol
- 2 `on_type_formatting.rs` tests — need cursor position
- Formatter unit tests with non-standard assertion shapes
  (e.g., `needs_quoting` function tests, `escape_double_quoted`
  tests) — these test internal functions, not the
  `format_yaml` public API

### Reference implementation

- `rlsp-yaml-parser/tests/conformance.rs` — rstest
  `#[files]` pattern, custom loader, per-file assertions
- `rlsp-yaml-parser/tests/yaml-test-suite/` — external
  test data consumed by the conformance harness

### Key files

- `rlsp-yaml/src/editing/formatter.rs` — formatter
  implementation + 70 inline tests (lines 871-2182)
- `rlsp-yaml/tests/ecosystem_fixtures.rs` — 23 ecosystem
  tests
- `rlsp-yaml/src/server.rs` — `YamlFormatOptions` and
  `YamlVersion` definitions (lines 47-88)
- `rlsp-yaml-parser/tests/conformance.rs` — reference
  harness pattern

## Steps

- [x] Spike: proof-of-concept with 3-5 fixture files and
      minimal harness (3aa8a95)
- [x] Finalize fixture format based on spike learnings
- [x] Build complete test harness with all assertion modes
- [ ] Migrate formatter.rs unit tests to fixture files
- [ ] Migrate ecosystem round-trip tests to fixture files
- [ ] Remove migrated inline tests from Rust source
- [ ] Verify coverage: all existing test behaviors preserved

## Tasks

### Task 1: Spike — proof-of-concept harness and fixtures (3aa8a95)

Build a minimal end-to-end proof-of-concept to validate
the fixture format and harness approach before committing
to a full migration.

- [x] Create `rlsp-yaml/tests/fixtures/formatter/` directory
- [x] Write 3-5 representative fixture files covering
      different modes:
  - One simple exact-output test (e.g., simple key-value)
  - One with non-default settings (e.g., `single_quote: true`)
  - One idempotent round-trip test (e.g., an ecosystem doc)
  - One with `print_width` override (e.g., long flow
    sequence breaks)
  - One with `Ref:` lines in prose (e.g., a version-aware
    quoting test referencing the YAML spec section)
- [x] Implement the test harness in
      `rlsp-yaml/tests/formatter_fixtures.rs`:
  - Parse markdown frontmatter (test-name, category,
    settings, idempotent flag)
  - Extract YAML code blocks from Test-Document and
    Expected-Document sections
  - Map frontmatter settings to `YamlFormatOptions`
  - Run `format_yaml` and assert output matches expected
  - For idempotent tests: format twice, assert stability
  - Prose section (between heading and Test-Document) is
    informational — not parsed by the harness
- [x] rstest `#[files]` glob over
      `tests/fixtures/formatter/*.md`
- [x] `cargo test` passes with the new fixture tests
      alongside existing tests
- [x] Report findings: did the format work well? Any
      changes needed before full migration?

### Task 2: Migrate formatter.rs quoting tests (~20 tests) — `eeefa0e`

Convert the quoting-related unit tests from `formatter.rs`
to fixture files. These include: `needs_quoting` output
tests (not the function-level tests), quote stripping,
version-aware quoting, plain scalar preservation.

- [x] Create fixture files for each quoting test that calls
      `format_yaml` (skip `needs_quoting` and
      `escape_double_quoted` function tests — those stay
      as Rust unit tests since they test internal functions)
- [x] Naming convention: `quoting-*.md`
- [x] Tests with `YamlVersion` settings use frontmatter
      `yaml_version: "1.1"` or `"1.2"`
- [x] Remove migrated tests from `formatter.rs`
- [x] `cargo test` passes, no regressions

### Task 3: Migrate formatter.rs flow/block style tests (~25 tests)

Convert flow-style preservation, `enforce_block_style`,
bracket spacing, empty collections, and mixed nesting
tests.

- [ ] Create fixture files for Groups B, C, D, E, F from
      `formatter.rs`
- [ ] Naming convention: `flow-*.md`, `block-*.md`,
      `empty-*.md`, `mixed-*.md`
- [ ] Tests with `format_enforce_block_style: true` or
      custom `bracket_spacing`/`print_width` use frontmatter
      settings
- [ ] Remove migrated tests from `formatter.rs`
- [ ] `cargo test` passes, no regressions

### Task 4: Migrate formatter.rs comment, blank-line, and structure tests (~24 tests)

Convert comments (C1-C10), blank-line preservation, multi-
document, nested sequences, and idempotency (Group G) tests.

- [ ] Create fixture files for comments, blank-lines,
      structure, multi-document, and idempotency tests
- [ ] Naming convention: `comment-*.md`, `blank-line-*.md`,
      `structure-*.md`, `multi-doc-*.md`, `idempotent-*.md`
- [ ] Remove migrated tests from `formatter.rs`
- [ ] `cargo test` passes, no regressions

### Task 5: Migrate ecosystem round-trip tests (~11 tests)

Convert the `assert_round_trip` tests from
`ecosystem_fixtures.rs` to fixture files. Diagnostic tests
(`assert_no_false_positives` and specific validator calls)
stay as Rust.

- [ ] Create fixture files for each ecosystem round-trip:
      K8s LimitRange, Deployment, ConfigMap, Service;
      GHA Workflow, Matrix; Ansible Playbook
- [ ] Also migrate the specific formatter assertions:
      `gha_on_key_stays_unquoted_after_format`,
      `gha_blank_lines_preserved_after_format`,
      `flow_sequence_command_items_indented_correctly`,
      `flow_collections_preserved_after_format`,
      `gha_workflow_flow_sequences_preserved`,
      `gha_matrix_flow_sequences_preserved`
- [ ] Naming convention: `ecosystem-*.md`
- [ ] Use `idempotent: true` for pure round-trip tests
- [ ] Remove migrated tests from `ecosystem_fixtures.rs`
- [ ] Remaining `ecosystem_fixtures.rs` tests (diagnostic
      assertions) still compile and pass
- [ ] `cargo test` passes, no regressions

### Task 6: Final cleanup and verification

- [ ] Verify total fixture file count matches expected
      (~70-80 files)
- [ ] Run `cargo test` — all tests pass
- [ ] Run `cargo clippy --all-targets` — zero warnings
- [ ] Verify that formatter.rs test module only contains
      tests for internal functions (`needs_quoting`,
      `escape_double_quoted`) that cannot use the fixture
      format
- [ ] Verify ecosystem_fixtures.rs only contains diagnostic
      tests that cannot use the fixture format
- [ ] Spot-check 5 fixture files for format consistency

## Decisions

- **Markdown with YAML frontmatter** — chosen over pure
  YAML because the fixture contains YAML *content* as test
  data; nesting YAML-in-YAML creates escaping ambiguity.
  Markdown code fences cleanly separate test data from
  metadata.
- **rstest `#[files]` pattern** — matches the existing
  conformance test harness pattern in the project. Gives
  per-file pass/fail visibility in test output.
- **Internal function tests stay as Rust** —
  `needs_quoting`, `escape_double_quoted`, and similar
  tests exercise private functions, not the `format_yaml`
  public API. They cannot use the fixture format and should
  remain as inline unit tests.
- **Diagnostic ecosystem tests stay as Rust** — they call
  `validate_duplicate_keys` and `validate_flow_style`
  directly, not the formatter. Different assertion shape.
- **One flat directory** — ~70-80 files is manageable.
  Subdirectories add navigation friction for marginal
  organization benefit.
- **Spec references in prose, not frontmatter** —
  references to specs, conventions, or standards (YAML
  spec sections, K8s API conventions, GHA syntax) belong
  in the prose description as `Ref:` lines. They're
  human-readable context, not harness inputs — putting
  them in frontmatter would add parsing complexity for a
  field the harness doesn't use. Multiple references are
  naturally supported as separate lines.
