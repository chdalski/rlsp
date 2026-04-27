---
test-name: block-to-flow-inner-collection-cursor
category: block-to-flow
cursor: 1:0
applies-action: block to flow
---

# Test: Offer block-to-flow for innermost block collection on cursor line

## Test-Document

```yaml
outer:
  inner:
    - a
    - b
```

## Expected-Document

```yaml
outer:
  inner: [a, b]
```
