---
test-name: block-to-flow-disabled-by-comment-between-a-block-scalar-and-the-next-entry
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment between a block scalar and the next entry is inside the converted range.

## Test-Document

```yaml
a:
  b: |
    text
  # c
  d: 1
```
