---
test-name: block-to-flow-disabled-by-comment-before-the-first-mapping-entry
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: An own-line comment before the first entry of a mapping is inside the converted range, so the action is offered disabled.

## Test-Document

```yaml
a:
  # lead
  b: 1
d: 1
```
