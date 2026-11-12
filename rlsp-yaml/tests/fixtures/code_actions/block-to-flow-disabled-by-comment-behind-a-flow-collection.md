---
test-name: block-to-flow-disabled-by-comment-behind-a-flow-collection
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment behind a flow collection that is an item of the block sequence is inside the converted range.

## Test-Document

```yaml
a:
  - [1] # c
  - 2
```
