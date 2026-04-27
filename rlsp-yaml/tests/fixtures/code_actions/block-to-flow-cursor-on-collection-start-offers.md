---
test-name: block-to-flow-cursor-on-collection-start-offers
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Offer block-to-flow when cursor is on block collection start line

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
