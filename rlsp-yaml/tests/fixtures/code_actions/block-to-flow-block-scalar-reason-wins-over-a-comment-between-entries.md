---
test-name: block-to-flow-block-scalar-reason-wins-over-a-comment-between-entries
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style cannot hold the block scalar (| or >) in this collection
---

# Test: A block scalar and a comment in the collection: the reason names the block scalar.

## Test-Document

```yaml
a:
  b: |
    text
  # c
  d: 1
```
