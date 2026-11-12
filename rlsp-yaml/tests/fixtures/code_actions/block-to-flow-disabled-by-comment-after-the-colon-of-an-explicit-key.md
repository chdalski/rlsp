---
test-name: block-to-flow-disabled-by-comment-after-the-colon-of-an-explicit-key
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment behind the colon of an explicit key is inside the converted range.

## Test-Document

```yaml
? a
: # c
  b: 1
```
