---
test-name: block-to-flow-disabled-by-comment-behind-an-inner-entry
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment behind an inner entry is inside the converted range, so the action is offered disabled.

## Test-Document

```yaml
a:
  b: 1 # c
  d: 2
e: 1
```
