---
test-name: block-scalar-multiline-plain-keeps-blank-line
category: block-scalar
cursor: 0:0
applies-action: literal
---

# Test: Convert a multi-line plain scalar and keep the blank line after it

The edit ends at the last content byte of the scalar, so the blank line and
the following key stay where they were.

## Test-Document

```yaml
description: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  bbbb

other: 1
```

## Expected-Document

```yaml
description: |
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbb

other: 1
```
