---
test-name: block-to-flow-block-scalar-outside-the-collection-offers
category: block-to-flow
cursor: 2:0
applies-action: Convert block to flow style
---

# Test: A block scalar before and after the collection stays as it is and does not disable the action.

## Test-Document

```yaml
x: |
  before
a:
  b: 1
y: >
  after
```

## Expected-Document

```yaml
x: |
  before
a: { b: 1 }
y: >
  after
```
