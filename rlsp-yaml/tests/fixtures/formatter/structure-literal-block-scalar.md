---
test-name: structure-literal-block-scalar
category: structure
---

# Test: Literal Block Scalar Style Preserved

A literal block scalar (`|`) retains its style indicator. The block scalar
content is indented one level relative to the parent key.

## Test-Document

```yaml
key: |
  line one
  line two
```

## Expected-Document

```yaml
key: |
  line one
  line two
```
