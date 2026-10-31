---
test-name: comment-document-root-block-scalar-header-stays-on-line
category: comments
---

# Test: A Comment On A Root Block Scalar's Header Stays There

The root block scalar is printed below the marker with its header comment.

## Test-Document

```yaml
--- | # c
  x
```

## Expected-Document

```yaml
---
|  # c
  x
```
