---
test-name: block-to-flow-disabled-by-comment-deep-inside-nested-flow
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment deep inside nested flow collections is inside the converted range.

## Test-Document

```yaml
a:
  b:
    - { x: [ # c
        1] }
```
