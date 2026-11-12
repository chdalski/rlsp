---
test-name: block-to-flow-disabled-by-comment-before-the-anchor-of-the-first-key
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment before the anchor of the first key is inside the converted range, because the anchor belongs to the key, not to the collection.

## Test-Document

```yaml
a:
  # c
  &k b: 1
  d: 2
```
