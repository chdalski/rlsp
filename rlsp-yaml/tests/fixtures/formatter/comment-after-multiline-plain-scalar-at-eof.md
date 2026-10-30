---
test-name: comment-after-multiline-plain-scalar-at-eof
category: comments
---

# Test: Comment Line After Multi-Line Plain Scalar At End Of Input

The comment line after the last multi-line plain scalar of the document stays
on its own line after the folded value.

## Test-Document

```yaml
a:
  v
  w
# d
```

## Expected-Document

```yaml
a: v w
# d
```
