**Repository:** root
**Status:** InProgress
**Created:** 2026-04-14

## Goal

Fix three confirmed bugs and one coverage gap in the YAML
formatter, discovered during the fixture test migration
audit. Each bug produces incorrect formatter output for
real-world YAML patterns. After fixes, add fixture tests
that exercise the corrected behavior and update existing
fixtures whose expected output encoded the buggy behavior.

## Context

### Bug inventory

**Bug 1 (CRITICAL): Block scalar indentation stripped**

`repr_block_to_doc` in `formatter.rs:556-572` emits block
scalar content lines with `hard_line() + text(line)` but
never wraps them in `indent()`. The content loses its
required indentation relative to the key, producing
invalid YAML.

Input:  `run: |\n  cargo build\n  cargo test\n`
Output: `run: |\ncargo build\ncargo test\n`

The output fails to parse because unindented content at
the same column as `run:` is interpreted as new mapping
keys. This affects every real-world YAML with `run: |`,
`script: |`, multi-line descriptions, etc.

The fix: wrap content lines in `indent()` so the
pretty-printer indents them relative to the key. The
`hard_line()` inside an `indent()` block already produces
the correct indentation — this is how block mappings and
sequences work in the formatter.

Existing fixtures that encode the bug:
- `structure-literal-block-scalar.md` — expected output
  has unindented content
- `blank-line-inside-block-scalar-unaffected.md` — same

Conformance suite baseline has 51 "formatted output
unparseable" cases, many likely caused by this bug.

**Bug 2 (HIGH): Anchor definitions silently dropped**

`node_to_doc` in `formatter.rs:393-444` destructures each
Node variant with `..` which skips the `anchor` field.
The anchor name (`&defaults`) is never emitted. Alias
references (`*defaults`) ARE preserved (line 443), so
the output has dangling aliases.

Input:  `defaults: &defaults\n  timeout: 30\n`
Output: `defaults:\n  timeout: 30\n`

The fix: for each Node variant (Scalar, Mapping,
Sequence), check `anchor` and prepend `&name ` to the
node's Doc output when present. Also handle anchors on
mappings and sequences (e.g., `&map_anchor` on a mapping
node).

No existing fixtures cover anchors — all new.

**Bug 3 (MODERATE): `single_quote` option quotes keys**

`string_to_doc` in `formatter.rs:453-468` unconditionally
wraps plain strings in single quotes when
`options.single_quote` is true (line 462-463). This
function is called for both keys and values via
`node_to_doc` → `key_value_to_doc` (line 634). Keys
should never be quoted by the `single_quote` option —
it's a value-only preference.

Input:  `key: hello\n` with `single_quote: true`
Output: `'key': 'hello'\n`
Expected: `key: 'hello'\n`

The fix: either pass a context flag to `string_to_doc`
indicating key vs value position, or handle key rendering
separately in `key_value_to_doc` without the
`single_quote` option.

Existing fixture encodes the bug:
- `single-quote-option.md` — expected output has
  `'key': 'hello'`

**Coverage gap: Tags on mappings/sequences dropped**

`node_to_doc` handles tags on scalars (lines 398-406)
but uses `..` for Mapping and Sequence variants (lines
439, 441), silently dropping their `tag` field. Tags like
`!!map`, `!!seq`, or custom tags on collections are lost.

The fix: extract `tag` from Mapping and Sequence the same
way it's done for Scalar.

No existing fixtures cover tags on collections.

### Key files

- `rlsp-yaml/src/editing/formatter.rs` — formatter
  implementation
  - `repr_block_to_doc` (line 556) — Bug 1
  - `node_to_doc` (line 393) — Bug 2, tag gap
  - `string_to_doc` (line 453) — Bug 3
  - `key_value_to_doc` (line 633) — Bug 3 call site
- `rlsp-yaml/tests/formatter_fixtures.rs` — fixture
  test harness
- `rlsp-yaml/tests/fixtures/formatter/` — fixture files
- `rlsp-yaml-parser/src/node.rs` — Node AST definition
  with `anchor: Option<String>` and `tag: Option<String>`
  fields on all variants
- `rlsp-fmt/src/ir.rs` — Doc IR (`indent`, `hard_line`,
  `text`, `concat`, `group`)

### References

- YAML 1.2 §8.1 — Block Scalar Headers (indentation
  indicator, chomp indicator)
- YAML 1.2 §6.9.2 — Node Anchors (`&anchor`)
- YAML 1.2 §6.9.1 — Node Tags

## Steps

- [x] Fix block scalar indentation (Bug 1)
- [x] Fix anchor preservation (Bug 2)
- [x] Fix single_quote key quoting (Bug 3)
- [x] Fix tag preservation on collections (coverage gap)
- [x] Fix comment loss in nested mappings (Bug 5)
- [x] Update existing fixtures that encoded buggy behavior
- [x] Add new fixtures for anchors, tags, block scalar
      real-world patterns
- [x] Verify conformance suite improvement

## Tasks

### Task 1: Fix block scalar indentation — `0b31477`

Fix `repr_block_to_doc` to emit indented content lines.
The content lines of a block scalar must be indented
relative to the key — the pretty-printer's `indent()`
handles this when content is wrapped in an indent block.

- [x] Modify `repr_block_to_doc` to wrap content lines
      in `indent()` so they are indented relative to the
      parent key
- [x] Handle all six block scalar styles: `|`, `|-`, `|+`,
      `>`, `>-`, `>+`
- [x] Handle empty lines within block scalars (they must
      remain empty, not gain indentation)
- [x] Update `structure-literal-block-scalar.md` expected
      output to show correct indentation
- [x] Update `structure-folded-block-scalar.md` expected
      output
- [x] Update `blank-line-inside-block-scalar-unaffected.md`
      expected output
- [x] Add new fixtures:
  - `block-scalar-literal-multiline.md` — multi-line `|`
    with 3+ lines
  - `block-scalar-gha-run-step.md` — GHA `run: |` with
    shell commands (real-world pattern)
  - `block-scalar-folded-paragraph.md` — `>` with wrapped
    paragraph text
  - `block-scalar-nested-in-sequence.md` — block scalar
    inside a sequence item (indentation stacking)
  - `block-scalar-chomp-strip.md` — `|-` trailing newline
    behavior
  - `block-scalar-chomp-keep.md` — `|+` trailing newline
    behavior
- [x] Run conformance suite, report improvement in
      pass rate vs baseline
- [x] `cargo test`, `cargo clippy --all-targets` pass

### Task 2: Fix anchor preservation — `5309f84`

Modify `node_to_doc` to emit anchor definitions when the
`anchor` field is present on any Node variant.

- [x] In `node_to_doc`, extract `anchor` from Scalar,
      Mapping, and Sequence variants
- [x] When `anchor` is `Some(name)`, prepend `&name ` to
      the node's Doc output
- [x] Handle anchor on alias nodes (edge case — an alias
      with an anchor: `&new_name *old_name`)
- [x] Add fixtures:
  - `anchor-scalar-definition.md` — `key: &val value`
  - `anchor-mapping-definition.md` —
    `defaults: &defaults\n  timeout: 30`
  - `anchor-alias-reference.md` — round-trip with anchor
    def + alias usage
  - `anchor-merge-key.md` — `<<: *defaults` merge pattern
  - `anchor-sequence-definition.md` — anchor on a sequence
    node
  - `anchor-idempotent.md` — idempotent round-trip of
    anchor/alias document
- [x] `cargo test`, `cargo clippy --all-targets` pass

### Task 3: Fix single_quote key quoting — `7390155`

Modify the formatter so `single_quote: true` only applies
to values, not keys.

- [x] Add a `in_key_position: bool` parameter to
      `string_to_doc` (or use a separate code path in
      `key_value_to_doc` for key rendering)
- [x] When in key position, skip the `single_quote` wrap
      — keys use plain style unless `needs_quoting` is
      true
- [x] Update `single-quote-option.md` expected output
      from `'key': 'hello'` to `key: 'hello'`
- [x] Add fixtures:
  - `quoting-single-quote-key-not-quoted.md` — key stays
    plain when `single_quote: true`
  - `quoting-single-quote-key-needs-quoting.md` — key
    that genuinely needs quoting still gets quoted
  - `quoting-single-quote-multiple-keys.md` — multiple
    keys all stay plain, values all get single-quoted
- [x] `cargo test`, `cargo clippy --all-targets` pass

### Task 4: Fix tag preservation on collections — `c45d048`

Extend `node_to_doc` to preserve tags on Mapping and
Sequence nodes, matching the existing Scalar tag handling.

- [x] Extract `tag` from Mapping and Sequence variants in
      `node_to_doc` (currently skipped via `..`)
- [x] Prepend `tag ` prefix for non-core-schema tags,
      same logic as Scalar (lines 398-406)
- [x] Add fixtures:
  - `tag-custom-on-mapping.md` — `!custom {a: 1}`
  - `tag-custom-on-sequence.md` — `!custom [a, b]`
  - `tag-core-schema-stripped.md` — `!!map` / `!!seq`
    core tags not preserved (matching scalar behavior)
  - `tag-custom-on-block-mapping.md` — `!custom\n  a: 1`
- [x] `cargo test`, `cargo clippy --all-targets` pass

### Task 5: Fix comment loss in nested mappings (Bug 5) — `168c0e3`

Fix `attach_comments` to preserve leading comments
between entries in nested block mappings. User-reported
bug (`.ai/reports/user-bug-repot-comments.md`) with three
variants:

1. Leading comments (`# Style 1`, `# Style 2`) between
   entries in a nested mapping are completely stripped
2. When trailing inline comments exist, leading comments
   are still stripped but inline comments survive (partial
   loss)
3. A colon inside an inline comment (`# another : bug`)
   may break syntax highlighting downstream

Root cause: `attach_comments` signature-based matching
fails for leading comments indented inside nested block
mappings. The comments are not matched to any content
entry and are silently dropped.

- [x] Investigate `attach_comments` to identify why
      leading comments inside nested mappings are lost
- [x] Fix the matching/reattachment logic
- [x] Add fixtures:
  - `comment-nested-mapping-leading.md` — leading comments
    between entries in a nested mapping preserved
  - `comment-nested-mapping-leading-and-trailing.md` —
    leading comments + trailing inline comment, both
    preserved
  - `comment-with-colon-in-text.md` — inline comment
    containing `: ` does not break output
- [x] `cargo test`, `cargo clippy --all-targets` pass

### Task 6: Add real-world ecosystem fixtures — `7f81bd0`

Add fixtures for real-world patterns that exercise the
fixed behaviors, covering patterns from the audit that had
no fixture coverage.

- [x] GHA workflow with `run: |` block scalar steps
- [x] GitLab CI with `script:` array and `variables:`
- [x] Docker Compose with `healthcheck.test` flow array
- [x] Ansible with handlers and `when` conditions
- [x] K8s Deployment with annotations (long quoted values)
- [x] Anchors + aliases real-world pattern (shared defaults)
- [x] All fixtures pass with exact or idempotent assertion
- [x] `cargo test`, `cargo clippy --all-targets` pass

### Task 7: Conformance suite verification — measurement only, no code commit

Run the conformance suite and compare results against the
pre-fix baseline to measure improvement.

- [x] Run conformance suite, record pass/fail counts
      — Parser: 351/351 (100%). Formatter round-trip:
      268/307 (87.3%), 31 unparseable, 8 empty.
- [x] Compare to baseline (51 "formatted output
      unparseable" failures expected to decrease after
      Bug 1 fix) — decreased from 51 to 31 (20 fewer)
- [x] Document any remaining failures and their root cause
      — 31 failures categorized: tab handling (~8), explicit
      keys (~6), escape sequences (~3), folded scalar edge
      cases (~3), trailing comments (~2), tab indentation
      (~2), anchor edge cases (~2), other (~4)
- [x] `cargo test` passes (no regressions)

## Decisions

- **Fix order: block scalars first** — Bug 1 is the most
  impactful and blocks the most real-world fixtures. Other
  bugs can be fixed independently.
- **Key position via parameter, not context struct** —
  for Bug 3, adding a `bool` parameter to `string_to_doc`
  is simpler than introducing a formatting context struct.
  YAGNI — if more context is needed later, refactor then.
- **Anchors prepended to Doc output** — `&name ` goes
  before the node's Doc, matching YAML source order. This
  is a concatenation, not a wrapper.
- **Core schema tags stripped on collections** — matching
  existing scalar behavior (lines 398-406). Only user/
  custom tags are preserved.
- **All fixes in rlsp-yaml only** — `rlsp-fmt` is a
  generic Wadler-Lindig pretty-printing engine designed
  to be reusable across language servers (e.g., a future
  `rlsp-json`). Every bug fix changes how the YAML
  formatter *uses* `rlsp-fmt` primitives (`indent()`,
  `hard_line()`, `text()`), not the primitives themselves.
  Do not modify `rlsp-fmt` — if a fix seems to require
  changes there, re-evaluate the approach.
- **Reviewer bug-spotting** — during each task review,
  the reviewer should actively look for additional
  formatter bugs beyond the scope of the task. The
  reviewer will be reading formatter code deeply and may
  spot issues the audit missed. Any findings should be
  reported to the lead for triage and possible addition
  to this plan or follow-up.
