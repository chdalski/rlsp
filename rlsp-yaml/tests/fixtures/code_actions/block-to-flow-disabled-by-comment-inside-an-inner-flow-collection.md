---
test-name: block-to-flow-disabled-by-comment-inside-an-inner-flow-collection
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment inside a flow collection that the block collection holds is inside the converted range, so the action is offered disabled.

## Test-Document

```yaml
a:
  b: [ # c
    1]
  d: 2
```
