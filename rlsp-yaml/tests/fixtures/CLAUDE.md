# Formatter Fixtures

## Setting Interaction Coverage

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

## Idempotency-Only Fixtures

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
