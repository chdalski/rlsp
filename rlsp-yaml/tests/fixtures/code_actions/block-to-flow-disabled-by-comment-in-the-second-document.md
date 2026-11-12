---
test-name: block-to-flow-disabled-by-comment-in-the-second-document
category: block-to-flow
cursor: 2:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment in the collection of a later document disables the action there.

## Test-Document

```yaml
x: 1
---
a: # c
  b: 1
```
