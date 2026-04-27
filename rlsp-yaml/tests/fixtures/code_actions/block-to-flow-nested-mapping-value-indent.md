---
test-name: block-to-flow-nested-mapping-value-indent
category: block-to-flow
cursor: 1:0
applies-action: block to flow
---

# Test: Use key indent plus 2 as base indent for nested mapping value block

## Test-Document

```yaml
outer:
  inner:
    x: 1
```

## Expected-Document

```yaml
outer:
  inner: { x: 1 }
```
