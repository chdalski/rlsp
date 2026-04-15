---
test-name: block-scalar-non-whitespace-unchanged
category: block-scalar
idempotent: true
---

# Test: Literal Block Scalar With Non-Whitespace Content Stays As Block Scalar

When a literal block scalar's content lines contain at least one non-whitespace
character, the whitespace-only guard does not fire and the value remains encoded
as a block scalar.

This is a boundary test: it confirms the guard is precise and does not
over-trigger. A line like `"  hello"` starts with spaces but contains the
non-whitespace word `hello` — it must not be converted to double-quoted.

## Test-Document

```yaml
key: |
  hello
```
