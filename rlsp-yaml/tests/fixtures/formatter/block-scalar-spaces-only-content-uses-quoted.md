---
test-name: block-scalar-spaces-only-content-uses-quoted
category: block-scalar
idempotent: true
---

# Test: Literal Block Scalar With Spaces-Only Content Falls Back To Quoted

When a literal block scalar's decoded value consists entirely of space
characters (no tabs or non-whitespace text), the YAML blank-line indentation
constraint prevents encoding it as a block scalar. A content line made of
only spaces would — after the formatter adds its indentation prefix — have
more spaces than the declared indent level, causing the re-parser to reject
it with "block scalar blank line has more indentation than the content".

The formatter detects this pattern and falls back to a double-quoted scalar,
which faithfully preserves the space content and round-trips idempotently.

Note: tab characters do not trigger this fallback because the YAML parser
treats a line containing a tab as a non-blank content line, not a blank one.

The parser normalises the source indentation when loading a block scalar, so
this fixture tests idempotency of the formatter output on the double-quoted
form that the formatter produces for whitespace-only block scalar values.

## Test-Document

```yaml
- "  \n"
```
