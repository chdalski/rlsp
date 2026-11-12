---
test-name: block-to-flow-disabled-by-comment-after-an-empty-inner-value
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment behind the key of an entry without a value is inside the converted range when another entry follows.

## Test-Document

```yaml
a:
  b: # n
  c: 1
```
