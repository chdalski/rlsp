---
test-name: block-to-flow-disabled-by-comment-between-the-anchor-and-the-tag
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment between the anchor and the tag is inside the converted range.

## Test-Document

```yaml
a: &x
  # c
  !t
  b: 1
```
