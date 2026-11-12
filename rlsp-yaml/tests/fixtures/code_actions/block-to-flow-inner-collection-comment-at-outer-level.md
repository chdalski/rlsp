---
test-name: block-to-flow-inner-collection-comment-at-outer-level
category: block-to-flow
cursor: 1:0
applies-action: Convert block to flow style
---

# Test: A comment at the indent of the outer collection lies behind the inner one and stays.

## Test-Document

```yaml
a:
  b:
    c: 1
    d: 2
  # mid
e: 1
```

## Expected-Document

```yaml
a:
  b: { c: 1, d: 2 }
  # mid
e: 1
```
