---
test-name: block-to-flow-disabled-by-comment-several-comments
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: Several comments in the converted range disable the action once.

## Test-Document

```yaml
a:
  b: 1 # c
  d: # e
    f: 1
  g: x
```
