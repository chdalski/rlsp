---
test-name: block-to-flow-disabled-by-comment-after-the-anchor
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment after the collection's anchor is inside the converted range, so the action is offered disabled.

## Test-Document

```yaml
a: &x # note
  b: 1
d: 1
```
