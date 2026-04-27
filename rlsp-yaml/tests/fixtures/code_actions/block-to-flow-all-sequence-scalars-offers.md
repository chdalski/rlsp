---
test-name: block-to-flow-all-sequence-scalars-offers
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Offer block-to-flow when all sequence items are scalars

## Test-Document

```yaml
items:
  - one
  - two
```

## Expected-Document

```yaml
items: [one, two]
```
