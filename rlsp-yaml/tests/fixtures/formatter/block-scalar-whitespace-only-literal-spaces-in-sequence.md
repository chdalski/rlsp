---
test-name: block-scalar-whitespace-only-literal-spaces-in-sequence
category: block-scalar
---

# Test: Literal Block Scalar With Spaces-Only Content In Sequence Falls Back To Quoted

When a literal block scalar inside a sequence has a decoded value consisting
solely of space characters followed by a newline, the formatter must fall back
to a double-quoted scalar. Emitting the spaces as a block scalar content line
would — after the formatter's indentation is applied — produce a line with more
indentation than the declared indent level, which the re-parser rejects as a
blank-line indentation violation.

This fixture covers the `- |` (sequence item, literal clip) form.

## Test-Document

```yaml
- |
   
```

## Expected-Document

```yaml
- "  \n"
```
