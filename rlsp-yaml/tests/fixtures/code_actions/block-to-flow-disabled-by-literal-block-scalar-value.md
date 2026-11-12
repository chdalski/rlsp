---
test-name: block-to-flow-disabled-by-literal-block-scalar-value
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style cannot hold the block scalar (| or >) in this collection
---

# Test: A literal block scalar as a value disables the action, because a flow collection cannot hold it.

## Test-Document

```yaml
a:
  b: |
    text
  c: 1
```
