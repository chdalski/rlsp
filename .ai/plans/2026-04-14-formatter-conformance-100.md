**Repository:** root
**Status:** InProgress
**Created:** 2026-04-14

## Goal

Fix all 24 remaining formatter round-trip conformance
failures and close the fixture gap for interacting settings
combinations. After this plan, every non-fail case in the
yaml-test-suite should round-trip through the formatter
without producing unparseable or empty output.

Acceptance criterion: 0 formatter round-trip failures.
The formatter conformance test
(`rlsp-yaml/tests/formatter_conformance.rs`) has 80
known-failure entries across 70 unique test case stems
(see `KNOWN_FAILURES` allowlist). All must be resolved
— the allowlist must be empty when the plan completes.

## Context

### Current conformance state

Parser: 351/351 (100%). The formatter conformance test
(`rlsp-yaml/tests/formatter_conformance.rs`) has **80
known-failure entries** across 70 unique test case stems.
This is the authoritative list — earlier audit estimates
of 24-27 were based on a temporary measurement that
missed cases. The `KNOWN_FAILURES` allowlist in
`formatter_conformance.rs` is the source of truth.

Measured after the `2026-04-14-formatter-bug-fixes.md`
plan which fixed block scalar indentation, anchors,
single_quote key quoting, tags, and comment loss.

### Failure inventory (24 unparseable + 3 empty)

**Category 1: Double-quoted escape sequences decoded and
unquoted (2 cases)**
- G4RS — `control: "\b1998\t1999"` decoded to raw bytes
- 6SLA — `"foo\nbar:baz"` decoded, newline splits key

Root cause: `node_to_doc` Scalar branch for DoubleQuoted
style calls `needs_quoting` on the *decoded* value. If
`needs_quoting` returns false, the scalar is emitted
plain — but the decoded value may contain characters that
are not representable in a plain scalar (control chars,
newlines). The fix: when a double-quoted scalar contains
escape sequences that decode to non-printable or
structural characters (`\n`, `\t`, `\b`, `\0`, `\\`,
`\"`, etc.), always preserve double-quoting.

**Category 2: Block scalar explicit indentation indicator
stripped (2 cases)**
- F6MC — `a: >2` (explicit indent 2) becomes `a: >`
- 5GBF — folded scalar with tab-only blank line

Root cause: `repr_block_to_doc` emits the chomp indicator
but ignores the explicit indentation indicator digit. The
parser's `ScalarStyle::Literal(Chomp)` / `Folded(Chomp)`
does not carry the indentation indicator. Either the parser
needs to pass it through, or the formatter needs to
compute the minimum content indentation and emit the
indicator when required (content starts with space or tab).

**Category 3: Multiline double-quoted scalars with
document markers in content (3 cases)**
- 9MQT — `"a\n...x\nb"` — `...` in content treated as
  doc marker
- KSS4 — `"quoted\nstring"` on `---` line
- 6WPF/TL85 — flow folding with leading/trailing whitespace

Root cause: the formatter decodes the double-quoted string
and emits it as plain or re-quoted, but the decoded
multiline content contains `---` or `...` at line start,
which the parser interprets as document markers. Fix:
preserve double-quoting for multiline double-quoted
scalars, or escape document-marker-like lines.

**Category 4: Tags on root nodes / explicit-key mappings
misrendered (4 cases)**
- 2XXW — `!!set` with `?`-only keys
- 35KP — `!!map` with `? a\n: b` explicit keys
- J7PZ — `!!omap` with complex values
- M2N8 — `? : x` empty explicit key

Root cause: the formatter does not support explicit key
syntax (`? key\n: value`). It renders all mappings as
`key: value` pairs, which collapses the structure when
the key is complex or the mapping uses `?`-only form
(sets). Tags on these root-level special forms get
concatenated with the first entry.

**Category 5: Anchor/property placement on block
collections (3 cases)**
- 3R3P — `&seq\n- a` — anchor before block sequence
  inlined as `&seq - a`
- PW8X — anchors on empty scalars produce duplicate
  anchor errors
- KK5P — explicit block mappings with nested sequences

Root cause: `node_to_doc` prepends `&anchor ` as inline
text, but block sequences/mappings need the anchor on a
separate line or before the first indicator. The current
`prepend_node_properties` concatenation doesn't account
for block-level placement.

**Category 6: Empty-key mappings (2 cases)**
- NKF9 — `: empty key` and `{ : empty key }` in flow
- V9D5 — compact block mappings with `? key: val`

Root cause: the formatter doesn't handle the empty-key
case (`Node::Scalar { value: "", .. }` as a mapping key)
or the compact block mapping form.

**Category 7: Multiline plain scalars with tabs (2 cases)**
- NB6Z — tab-only blank line in multiline plain scalar
- RZP5/XW4D — multiline plain + trailing comments +
  explicit keys interaction

Root cause: multiline plain scalars that span multiple
lines in the source are loaded as a single string by the
parser. The formatter emits them as a single-line value,
which may exceed print width or lose structural whitespace.

**Category 8: Comment-only / doc-end documents → empty
output (3 cases)**
- 98YD — `# Comment only.`
- HWV9 — `...` bare document-end
- QT73 — `# comment\n...`

Root cause: `format_yaml` calls `load()` which returns
empty documents for comment-only or `...`-only input. The
formatter produces an empty string. Comments are extracted
by `extract_doc_prefix_comments` but only reattached if
there's formatted content to attach them to.

### Fixture gap: settings interaction coverage

`YamlFormatOptions` has 8 settings. Existing fixtures test
each in isolation but no combinations. Interacting pairs
to cover (settings that affect the same formatting
decision):

- `single_quote` + `yaml_version` — quoting decision
  depends on both (V1.1 reserved words + quote style)
- `single_quote` + `format_enforce_block_style` — flow
  collections converted to block, then values quoted
- `bracket_spacing` + `print_width` — flow collections
  with spacing may break differently at narrow widths
- `bracket_spacing` + `format_enforce_block_style` —
  spacing irrelevant when block enforced (should be no-op)
- `format_enforce_block_style` + `print_width` — block
  style means no line-breaking in flow collections
- `use_tabs` + `tab_width` — tab indentation width

### Key files

- `rlsp-yaml/src/editing/formatter.rs` — formatter
- `rlsp-yaml-parser/src/loader.rs` — AST loader
- `rlsp-yaml-parser/src/node.rs` — Node AST
- `rlsp-yaml-parser/src/lexer/quoted.rs` — double-quoted
  scalar lexing / escape handling
- `rlsp-yaml/tests/fixtures/formatter/` — fixture files
- `rlsp-yaml-parser/tests/yaml-test-suite/` — conformance
  test data

### References

- YAML 1.2 §5.7 — Escaped Characters
- YAML 1.2 §8.1 — Block Scalar Headers
- YAML 1.2 §8.2.1 — Block Sequences (explicit entries)
- YAML 1.2 §8.2.2 — Block Mappings (explicit keys)
- YAML 1.2 §7.2 — Empty Nodes
- YAML 1.2 §9.1.4 — Document End Marker

## Steps

- [x] Move yaml-test-suite to shared location and add
      formatter conformance test to rlsp-yaml (fb5e904)
- [ ] Fix double-quoted escape sequence handling (Cat 1)
- [ ] Fix block scalar indentation indicators (Cat 2)
- [ ] Fix multiline double-quoted scalars (Cat 3)
- [ ] Support explicit key syntax (Cat 4)
- [ ] Fix anchor placement on block collections (Cat 5)
- [ ] Handle empty-key mappings (Cat 6)
- [ ] Handle multiline plain scalars with tabs (Cat 7)
- [ ] Handle comment-only / doc-end documents (Cat 8)
- [ ] Add interacting-settings fixture combinations
- [ ] Verify 0 conformance failures

## Tasks

### Task 1: Move yaml-test-suite to shared location and add formatter conformance test

Move the vendored yaml-test-suite from
`rlsp-yaml-parser/tests/yaml-test-suite/` to a shared
workspace-level directory `tests/yaml-test-suite/` so both
`rlsp-yaml-parser` and `rlsp-yaml` can reference the same
data without symlinks or duplication.

Add a permanent formatter conformance test to `rlsp-yaml`
that runs alongside the fixture tests in CI.

- [x] Create `tests/yaml-test-suite/` at workspace root
- [x] Move (not copy) the vendored data from
      `rlsp-yaml-parser/tests/yaml-test-suite/` to
      `tests/yaml-test-suite/`
- [x] Update `rlsp-yaml-parser/tests/conformance.rs`
      `#[files]` path to reference `../tests/yaml-test-suite/src/*.yaml`
- [x] Verify parser conformance still passes (351/351)
- [x] Create `rlsp-yaml/tests/formatter_conformance.rs`:
  - rstest `#[files]` over `../tests/yaml-test-suite/src/*.yaml`
  - For each non-fail case: `format_yaml(input)` must
    parse cleanly via `parse_yaml`, and formatting must
    be idempotent (`format(format(input)) == format(input)`)
  - Maintain an allowlist of known-failing case IDs
    (the 24 unparseable + 3 empty cases) — test passes
    if a case is on the allowlist and fails, or not on
    the allowlist and passes
  - Test FAILS if: a non-allowlisted case fails
    (regression) or an allowlisted case passes (remove
    it from the list — the test enforces shrinking the
    allowlist)
- [x] Populate the allowlist with the 27 currently-failing
      case IDs
- [x] `cargo test` passes for both crates
- [x] `cargo clippy --all-targets` clean

### Task 2: Fix double-quoted escape sequence handling (Cat 1)

Preserve double-quoting on scalars whose decoded content
contains non-printable characters, structural characters,
or escape sequences that cannot be represented in plain
YAML.

- [ ] In `node_to_doc` Scalar/DoubleQuoted branch, check
      if the decoded value contains characters that require
      double-quoting beyond what `needs_quoting` catches
      (control chars, newlines, tabs, backslashes, etc.)
- [ ] When such characters are present, always emit as
      double-quoted with proper escaping via
      `escape_double_quoted`
- [ ] Extend `escape_double_quoted` if needed to handle
      all YAML 1.2 §5.7 escape sequences
- [ ] Add fixtures for G4RS and 6SLA patterns
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 3: Fix block scalar indentation indicators

Preserve or compute the explicit indentation indicator
digit on block scalars whose content starts with leading
spaces or tabs.

- [ ] Check if the parser's `ScalarStyle` carries the
      indentation indicator — if not, compute it from the
      scalar content (minimum leading whitespace of
      non-empty lines)
- [ ] In `repr_block_to_doc`, emit the indentation
      indicator digit when content requires it (first
      content line starts with space or tab)
- [ ] Add fixtures for F6MC and 5GBF patterns
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 4: Fix multiline double-quoted scalars

Ensure double-quoted scalars whose decoded content spans
multiple lines (contains `\n`) or contains document
markers (`---`, `...`) are preserved as double-quoted.

- [ ] In the Scalar/DoubleQuoted branch, detect multiline
      content or content containing document marker
      patterns
- [ ] Preserve double-quoting with proper escaping
- [ ] Add fixtures for 9MQT, KSS4, 6WPF/TL85 patterns
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 5: Support explicit key syntax

Add support for rendering mappings that use `? key` /
`: value` explicit key form — needed for sets (`!!set`),
ordered maps (`!!omap`), complex keys, and empty keys.

- [ ] Detect when a mapping entry requires explicit key
      form: complex key (mapping/sequence as key), empty
      key, or tagged root mapping with `?`-only entries
- [ ] Render explicit keys as `? key\n: value` instead of
      `key: value`
- [ ] Handle `!!set` (sequence of `?`-only entries)
- [ ] Handle `!!omap` (sequence of single-entry mappings)
- [ ] Add fixtures for 2XXW, 35KP, J7PZ, M2N8 patterns
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 6: Fix anchor placement on block collections

Fix anchor/tag property placement for block sequences and
block mappings so properties appear on a separate line or
correctly before the first indicator.

- [ ] For block sequences with anchors, emit
      `&anchor\n- first` instead of `&anchor - first`
- [ ] For block mappings with anchors, emit
      `&anchor\nkey: value` instead of `&anchor key: value`
- [ ] Handle the edge case of anchors on empty scalars
      (PW8X)
- [ ] Handle complex explicit block mapping structures
      (KK5P)
- [ ] Add fixtures for 3R3P, PW8X, KK5P patterns
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 7: Handle empty-key mappings

Support the empty mapping key pattern (`: value` in block,
`{ : value }` in flow) and compact block mappings.

- [ ] Detect empty-key entries and render with explicit
      key form or proper empty-key syntax
- [ ] Handle flow-context empty keys
- [ ] Handle compact block mapping form (`? key: val`)
- [ ] Add fixtures for NKF9, V9D5 patterns
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 8: Handle multiline plain scalars with tabs

Preserve or correctly re-emit multiline plain scalars
that contain tab-only blank lines or complex whitespace
patterns.

- [ ] Detect multiline plain scalars from the AST
- [ ] Emit them preserving their multiline structure or
      convert to block scalar form when plain representation
      would be ambiguous
- [ ] Add fixtures for NB6Z, RZP5/XW4D patterns
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 9: Handle comment-only and doc-end documents

Ensure comment-only documents and bare document-end
markers produce non-empty output.

- [ ] In `format_yaml`, handle the case where `load()`
      returns empty documents but prefix comments exist
- [ ] Emit standalone comments when no document content
      exists
- [ ] Handle bare `...` document-end markers
- [ ] Add fixtures for 98YD, HWV9, QT73 patterns
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 10: Add interacting-settings fixture combinations

Add fixtures that test combinations of settings that
interact (affect the same formatting decision).

- [ ] `single_quote` + `yaml_version` — V1.1 keyword
      with single-quote preference
- [ ] `single_quote` + `format_enforce_block_style` —
      flow-to-block conversion with single-quote values
- [ ] `bracket_spacing` + `print_width` — narrow width
      with flow collections and spacing
- [ ] `bracket_spacing` + `format_enforce_block_style` —
      spacing irrelevant when block enforced
- [ ] `format_enforce_block_style` + `print_width` —
      block style ignores width for collections
- [ ] `use_tabs` + `tab_width` — tab indentation with
      custom tab width
- [ ] `cargo test`, `cargo clippy --all-targets` pass

### Task 11: Final conformance verification

Run the full formatter round-trip conformance measurement
and verify 0 failures.

- [ ] Run conformance measurement, record results
- [ ] Target: 0 unparseable, 0 empty output
- [ ] If any failures remain, document root cause and
      determine if they require parser changes (genuine
      blockers) vs formatter changes
- [ ] Update VS Code extension `package.json` and
      `config.ts` to reflect any new or changed settings
      in `YamlFormatOptions` — ensure all user-facing
      formatter settings are exposed in the extension
- [ ] `cargo test` passes
- [ ] `pnpm run build && pnpm run test && pnpm run lint`
      passes in `rlsp-yaml/integrations/vscode/`

## Decisions

- **All fixes in rlsp-yaml only** — `rlsp-fmt` must not
  be modified. If a fix seems to require `rlsp-fmt`
  changes, re-evaluate.
- **Explicit key syntax is the hardest task** — Task 4
  requires adding a rendering mode that doesn't exist in
  the formatter yet. It should be tackled after the
  simpler escape/quoting fixes.
- **Parser changes allowed in rlsp-yaml-parser** — some
  fixes (block scalar indentation indicator, multiline
  scalar metadata) may require the parser to expose
  additional information in the AST. Parser changes are
  in scope when they're necessary for correct formatting.
- **Reviewer actively bug-hunts** — same directive as the
  previous plan.
- **Acceptance criterion is 0 failures** — no silent
  weakening. If a case genuinely cannot be fixed without
  major architectural work, it must be presented to the
  user for explicit approval before being deferred.
