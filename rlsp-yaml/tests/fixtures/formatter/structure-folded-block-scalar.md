---
test-name: structure-folded-block-scalar
category: structure
---

# Test: Folded Block Scalar Style Preserved

A folded block scalar with strip chomping (`>-`) retains its style indicator.
The block scalar content is emitted verbatim without re-indentation.

## Test-Document

```yaml
key: >-
  content
```

## Expected-Document

```yaml
key: >-
content
```
