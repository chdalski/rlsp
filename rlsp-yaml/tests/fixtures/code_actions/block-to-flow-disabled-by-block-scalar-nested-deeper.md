---
test-name: block-to-flow-disabled-by-block-scalar-nested-deeper
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style cannot hold the block scalar (| or >) in this collection
---

# Test: A block scalar deep inside the collection disables the action of the outer collection.

## Test-Document

```yaml
a:
  b:
    - c: 1
      d: |
        text
```
