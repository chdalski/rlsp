---
test-name: block-to-flow-block-scalar-reason-wins-over-a-comment-in-the-header
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style cannot hold the block scalar (| or >) in this collection
---

# Test: A block scalar and a comment in its header: the reason names the block scalar.

## Test-Document

```yaml
a:
  b: | # hdr
    text
  c: 1
```
