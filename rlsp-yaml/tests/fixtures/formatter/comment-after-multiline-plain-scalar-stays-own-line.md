---
test-name: comment-after-multiline-plain-scalar-stays-own-line
category: comments
---

# Test: Comment Line After Multi-Line Plain Scalar Stays On Its Own Line

A comment line after a multi-line plain scalar is not part of the scalar and
not its trailing comment. The scalar folds onto one line and the comment keeps
its own line before the next key.

## Test-Document

```yaml
a:
  v
  w
# d
b: 1
```

## Expected-Document

```yaml
a: v w
# d
b: 1
```
