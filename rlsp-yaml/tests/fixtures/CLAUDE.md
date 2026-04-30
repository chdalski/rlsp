# Test Fixtures

## Formatter Fixtures

### Setting Interaction Coverage

When adding or modifying formatter settings, ensure test
fixtures cover not just each setting in isolation but also
**combinations of settings that interact**. Two settings
interact when enabling both produces behavior that differs
from enabling either alone.

Derive interacting pairs by reading `YamlFormatOptions` in
`rlsp-yaml/src/editing/formatter.rs` and tracing which
settings affect the same formatting pass or decision point.
Do not rely on a hardcoded list — the struct is the source
of truth.

For each interacting pair, at least one fixture should set
both settings to non-default values and demonstrate the
combined behavior.

### Idempotency-Only Fixtures

Some YAML constructs are normalized by the parser during
AST construction — the formatter receives the normalized
form and cannot observe the original source layout. For
these constructs, a fixture with identical Test-Document
and Expected-Document (testing only idempotency) is the
correct and only testable behavior.

Common examples: anchor placement on nodes (the parser
stores anchors as a name field, not a source position),
tag normalization, and whitespace in block scalar headers.

When a fixture tests idempotency because the parser
normalizes the input, add a note in the fixture prose
explaining why — e.g., "The parser normalizes anchor
placement into the AST node's anchor field, so the
formatter only sees the anchor name, not its original
position. This fixture verifies the formatter preserves
the correct output form."

Without this note, a future reviewer may flag the fixture
as testing nothing useful.

## When to Write a Fixture vs an Inline Test

Use a fixture when the behavior is **visually self-explanatory**:
a reader who opens the `.md` sees the input, the action or
transformation invoked, and the resulting document — no hidden
ambient state.

The fixture format is the preferred home for:
- **Cursor-driven code actions** — actions triggered by
  cursor position, not by a diagnostic (e.g. tab-to-spaces,
  block-to-flow, block-scalar conversion, quoted-bool).
- **Cursor-driven rename** — rename triggered by cursor
  position and a new name (e.g. renaming an anchor and all
  its aliases, rejecting invalid names, rejecting cursors
  not on a renameable symbol).
- **Transformational formatter tests** — input → expected
  output pairs.

Use an inline `#[test]` when:
- The test depends on diagnostics (diagnostic-driven code
  actions such as flow-to-block, delete-anchor, yaml11-bool,
  yaml11-octal stay 100% inline — their trigger state cannot
  be expressed in the fixture format).
- The assertion is structural (e.g. verifying a range field,
  not the output document).
- The scenario is a property test or signature smoke test.

One fixture per inline test being replaced — no consolidation
of `#[rstest] #[case]` arms. Each fixture is a discrete,
browsable artifact.

## Code-Action Fixtures

Fixtures live in `tests/fixtures/code_actions/*.md`.
The harness is `tests/code_action_fixtures.rs`.

### Frontmatter fields

All fields are between `---` delimiters. Most are flat key:value pairs;
`format-options:` introduces an indented block.

| Field | Required | Description |
|-------|----------|-------------|
| `test-name` | yes | Kebab-case name (informational) |
| `category` | no | Short label (informational) |
| `cursor` | yes | Zero-based `line:character` cursor position |
| `applies-action` | see below | Title substring of the action to invoke |
| `omits-action` | see below | Title substring asserted absent |
| `format-options:` | no | Block of indented key-value pairs overriding `YamlFormatOptions` fields |

Exactly one of `applies-action` or `omits-action` must be set; they are mutually exclusive.

### `format-options:` block

When present, the harness builds a `YamlFormatOptions` with the specified fields
and passes it to `code_actions(...)`. Unspecified fields remain at their default
values. When absent, `YamlFormatOptions::default()` is used.

Supported keys (mirror the formatter fixtures' `settings:` block convention):

| Key | Type | Description |
|-----|------|-------------|
| `print_width` | usize | Maximum line width (default: 80) |
| `tab_width` | usize | Spaces per indent level (default: 2) |
| `single_quote` | bool | Prefer single-quoted strings (default: false) |
| `bracket_spacing` | bool | Spaces inside flow braces `{ a: 1 }` vs `{a: 1}` (default: true) |
| `preserve_quotes` | bool | Preserve source quote style (default: false) |

Unknown keys are silently ignored — forward-compatible with future options.

Example:

```
---
test-name: block-to-flow-respects-configured-print-width
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
format-options:
  print_width: 120
---
```

### Assertion modes

**`applies-action: <title-substring>`**

The harness finds the first action whose title contains the
substring, extracts `action.edit.changes[uri][0]` (the first
`TextEdit`), applies it to the `## Test-Document`, and asserts
the result equals `## Expected-Document`.

Limitation: only the first TextEdit of the matching action is
applied. This is correct for single-edit actions (tab-to-spaces,
quoted-bool); document this if a future action produces multiple
edits.

**`omits-action: <title-substring>`**

The harness asserts that no returned action title contains the
substring. `## Expected-Document` is not required and should
be omitted.

### Cursor convention

`cursor: line:character` uses zero-based line and character
indices, matching the LSP `Position` type. Do not add 1.

### Sections

- `## Test-Document` — fenced YAML input (always required)
- `## Expected-Document` — fenced YAML output (required for
  `applies-action`; omit for `omits-action`)

The harness strips fenced code block delimiters and any
language tag (e.g. ` ```yaml `). Content starts at the
first line after the opening fence.

## Rename Fixtures

Fixtures live in `tests/fixtures/rename/rename-*.md`.
The harness is `tests/rename_fixtures.rs`.

### Frontmatter fields

All fields are between `---` delimiters.

| Field | Required | Description |
|-------|----------|-------------|
| `test-name` | yes | Kebab-case name (informational) |
| `category` | no | Short label (informational, e.g. `rename`) |
| `cursor` | yes | Zero-based `line:character` cursor position |
| `new-name` | yes | The proposed replacement anchor/alias name |
| `applies-rename` | see below | Set to `true`; asserts rename succeeds and applies edits |
| `omits-rename` | see below | Set to `true`; asserts rename returns `None` |

Exactly one of `applies-rename` or `omits-rename` must be present; they are mutually exclusive.

### Assertion modes

**`applies-rename: true`**

The harness calls `rename(docs, uri, cursor, new_name)`. On a `Some(WorkspaceEdit)` result,
it collects all `TextEdit`s from `changes[uri]`, sorts them in **reverse range-start order**
(highest line/character first), applies them to the `## Test-Document` string in that order,
then asserts the result equals `## Expected-Document`.

Applying edits in reverse order is necessary when rename produces multiple edits (anchor +
aliases): applying the edit at the later position first keeps the byte offsets of earlier
edits valid.

**`omits-rename: true`**

The harness asserts `rename(...)` returned `None`. `## Expected-Document` is not required
and should be omitted.

### Cursor convention

`cursor: line:character` uses zero-based line and character indices, matching the LSP
`Position` type. Do not add 1.

### Sections

- `## Test-Document` — fenced YAML input (always required)
- `## Expected-Document` — fenced YAML output (required for `applies-rename`; omit for
  `omits-rename`)

The harness strips fenced code block delimiters and any language tag (e.g. ` ```yaml `).
Content starts at the first line after the opening fence.

### Multi-edit application rule

When a rename produces multiple `TextEdit`s (one per anchor occurrence and one per alias
occurrence), the harness sorts all edits in **reverse range-start order** (descending by
line, then by character) and applies them sequentially. This ensures that applying the edit
at the later position first does not shift the byte offsets of edits at earlier positions.

### What stays inline (not ported to fixtures)

- All `prepare_rename` tests — `prepare_rename` returns `Option<Range>`, not a document
  transformation. Tests assert specific `Range` field values; there is no "expected output
  document" to compare against. These are Pattern C by construction.
- Rename tests that assert specific `Range` field values within `TextEdit`s (e.g.
  `should_produce_correct_edit_ranges`). The fixture format does not support range-field
  assertions — only output-document comparison.
- Existence-only assertions that don't validate output shape (e.g.
  `should_accept_new_name_at_exactly_max_length` — the 256-char name makes a readable
  `Expected-Document` impractical).
- Test inputs containing characters that cannot appear in YAML frontmatter — specifically
  newline (`\n`), tab (`\t`), and carriage return (`\r`). These are security-relevant
  rejection cases (YAML-structure injection, CRLF injection, whitespace-semantics
  protection) and must be exercised with the actual control characters, not surrogates.
  They stay inline as Pattern C in `rename.rs`.
