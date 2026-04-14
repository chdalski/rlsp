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
