---
test-name: block-to-flow-inner-collection-beside-a-block-scalar-offers
category: block-to-flow
cursor: 1:0
applies-action: Convert block to flow style
---

# Test: The inner collection offers the action although the outer one holds a block scalar.

## Test-Document

```yaml
a:
  b:
    c: 1
  d: |
    text
```

## Expected-Document

```yaml
a:
  b: { c: 1 }
  d: |
    text
```
