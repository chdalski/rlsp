---
test-name: block-to-flow-disabled-by-comment-before-the-first-sequence-item
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: An own-line comment before the first item of a sequence is inside the converted range, so the action is offered disabled.

## Test-Document

```yaml
a:
  # lead
  - x
d: 1
```
