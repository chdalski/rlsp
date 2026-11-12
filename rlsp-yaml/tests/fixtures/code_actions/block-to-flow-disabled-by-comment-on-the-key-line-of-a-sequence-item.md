---
test-name: block-to-flow-disabled-by-comment-on-the-key-line-of-a-sequence-item
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment on the key's line of a mapping that is an item of a sequence is inside the converted range.

## Test-Document

```yaml
- k: # c
    b: 1
```
