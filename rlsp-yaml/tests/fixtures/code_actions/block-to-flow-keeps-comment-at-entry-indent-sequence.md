---
test-name: block-to-flow-keeps-comment-at-entry-indent-sequence
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: An own-line comment indented like the items stays after a converted sequence.

## Test-Document

```yaml
a:
  - x
  - y
  # tail
d: 1
```

## Expected-Document

```yaml
a: [x, y]
  # tail
d: 1
```
