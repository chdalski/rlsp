---
test-name: block-to-flow-second-document
category: block-to-flow
cursor: 3:0
applies-action: block to flow
---

# Test: Offer block-to-flow for block collection in second document

## Test-Document

```yaml
key: value
---
other: stuff
items:
  - a
  - b
```

## Expected-Document

```yaml
key: value
---
other: stuff
items: [a, b]
```
