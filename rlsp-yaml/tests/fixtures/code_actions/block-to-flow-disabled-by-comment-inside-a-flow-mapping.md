---
test-name: block-to-flow-disabled-by-comment-inside-a-flow-mapping
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment inside a flow mapping that is a value of the block mapping is inside the converted range.

## Test-Document

```yaml
a:
  b: { # c
    x: 1}
  d: 2
```
