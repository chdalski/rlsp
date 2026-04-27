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

All fields are flat key:value pairs between `---` delimiters.

| Field | Required | Description |
|-------|----------|-------------|
| `test-name` | yes | Kebab-case name (informational) |
| `category` | no | Short label (informational) |
| `cursor` | yes | Zero-based `line:character` cursor position |
| `applies-action` | see below | Title substring of the action to invoke |
| `omits-action` | see below | Title substring asserted absent |

Exactly one of `applies-action` or `omits-action` must be set; they are mutually exclusive.

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
