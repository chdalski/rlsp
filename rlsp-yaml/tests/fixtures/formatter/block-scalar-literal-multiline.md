---
test-name: block-scalar-literal-multiline
category: block-scalar
---

# Test: Literal Block Scalar Multi-Line Content

A literal block scalar (`|`) with three content lines. Verifies that
indentation is applied to all content lines, not just the first.

## Test-Document

```yaml
script: |
  echo hello
  echo world
  exit 0
```

## Expected-Document

```yaml
script: |
  echo hello
  echo world
  exit 0
```
