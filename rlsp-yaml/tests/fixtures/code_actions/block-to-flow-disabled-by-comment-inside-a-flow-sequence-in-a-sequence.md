---
test-name: block-to-flow-disabled-by-comment-inside-a-flow-sequence-in-a-sequence
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment inside a flow sequence that is an item of the block sequence is inside the converted range.

## Test-Document

```yaml
a:
  - [ # c
    1]
  - 2
```
