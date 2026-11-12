---
test-name: block-to-flow-disabled-by-comment-after-the-dash
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment after the dash of a sequence item is inside the converted range.

## Test-Document

```yaml
a:
  - # c
    x
  - y
```
