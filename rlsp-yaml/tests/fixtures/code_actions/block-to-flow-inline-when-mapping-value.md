---
test-name: block-to-flow-inline-when-mapping-value
category: block-to-flow
cursor: 1:0
applies-action: block to flow
---

# Test: Emit flow inline when block collection is a mapping value

## Test-Document

```yaml
outer:
  inner:
    x: 1
    y: 2
```

## Expected-Document

```yaml
outer:
  inner: { x: 1, y: 2 }
```
