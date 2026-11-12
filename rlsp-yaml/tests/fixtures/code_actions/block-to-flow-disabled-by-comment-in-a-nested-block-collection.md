---
test-name: block-to-flow-disabled-by-comment-in-a-nested-block-collection
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment inside a block collection that the converted collection holds is inside the converted range.

## Test-Document

```yaml
a:
  b:
    - x
    # c
    - y
  d: 1
```
