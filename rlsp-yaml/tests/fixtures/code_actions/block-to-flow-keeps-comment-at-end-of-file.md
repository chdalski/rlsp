---
test-name: block-to-flow-keeps-comment-at-end-of-file
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A comment at the end of the file, with no next sibling, stays.

## Test-Document

```yaml
a:
  b: 1
# end
```

## Expected-Document

```yaml
a: { b: 1 }
# end
```
