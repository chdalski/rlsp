---
test-name: block-scalar-sequence-item-indented-converts
category: block-scalar
cursor: 1:2
applies-action: literal
---

# Test: Convert indented sequence item to block scalar (non-zero base_indent)

Scalar starts at column 4 (after `  - `), so base_indent = 4 - 2 = 2 and
block scalar body is indented 4 spaces.

## Test-Document

```yaml
parent:
  - "this is a very long scalar value that exceeds forty characters"
```

## Expected-Document

```yaml
parent:
  - |
    this is a very long scalar value that exceeds forty characters
```
