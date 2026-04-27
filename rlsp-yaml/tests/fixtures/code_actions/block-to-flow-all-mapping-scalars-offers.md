---
test-name: block-to-flow-all-mapping-scalars-offers
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Offer block-to-flow when all mapping values are scalars

## Test-Document

```yaml
point:
  x: 1
  y: 2
```

## Expected-Document

```yaml
point: { x: 1, y: 2 }
```
