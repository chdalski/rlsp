---
test-name: block-to-flow-keeps-comment-at-entry-indent
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: An own-line comment indented like the entries is outside the collection's content, so it stays.

## Test-Document

```yaml
a:
  b: 1
  # tail
d: 1
```

## Expected-Document

```yaml
a: { b: 1 }
  # tail
d: 1
```
