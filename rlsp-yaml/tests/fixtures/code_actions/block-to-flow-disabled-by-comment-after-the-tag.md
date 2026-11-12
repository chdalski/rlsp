---
test-name: block-to-flow-disabled-by-comment-after-the-tag
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment after the collection's tag is inside the converted range.

## Test-Document

```yaml
a: !t # note
  b: 1
d: 1
```
