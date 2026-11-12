---
test-name: block-to-flow-disabled-by-block-scalar-item
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style cannot hold the block scalar (| or >) in this collection
---

# Test: A block scalar as a sequence item disables the action.

## Test-Document

```yaml
a:
  - |
    text
  - x
```
