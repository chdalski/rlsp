---
test-name: block-to-flow-disabled-by-block-scalar-in-the-second-document
category: block-to-flow
cursor: 2:0
disables-action: Convert block to flow style
disabled-reason: Flow style cannot hold the block scalar (| or >) in this collection
---

# Test: A block scalar in the collection of a later document disables the action there.

## Test-Document

```yaml
x: 1
---
a:
  b: |
    text
```
