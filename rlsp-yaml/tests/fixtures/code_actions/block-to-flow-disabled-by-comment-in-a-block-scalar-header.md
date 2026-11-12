---
test-name: block-to-flow-disabled-by-comment-in-a-block-scalar-header
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment in the header of a block scalar inside the collection is inside the converted range.

## Test-Document

```yaml
a:
  b: | # hdr
    text
  c: 1
```
