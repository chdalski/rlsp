---
test-name: block-to-flow-keeps-blank-line-and-comment-after
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A blank line and a comment after the last entry stay where they are.

## Test-Document

```yaml
a:
  b: 1
  c: 2

# keep me
d: 1
```

## Expected-Document

```yaml
a: { b: 1, c: 2 }

# keep me
d: 1
```
