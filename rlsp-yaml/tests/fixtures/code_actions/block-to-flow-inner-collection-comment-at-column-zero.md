---
test-name: block-to-flow-inner-collection-comment-at-column-zero
category: block-to-flow
cursor: 1:0
applies-action: Convert block to flow style
---

# Test: The spans of the inner and the outer collection end on the same line; the comment stays.

## Test-Document

```yaml
a:
  b:
    c: 1
# outer
e: 1
```

## Expected-Document

```yaml
a:
  b: { c: 1 }
# outer
e: 1
```
