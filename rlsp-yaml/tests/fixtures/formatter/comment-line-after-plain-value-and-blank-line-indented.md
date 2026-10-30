---
test-name: comment-line-after-plain-value-and-blank-line-indented
category: comments
---

# Test: Indented Comment Line After Blank Line Following Plain Value

An indented comment line after a plain scalar is not part of the scalar. It ends
the scalar and keeps its own line at the indentation of the enclosing entry.

## Test-Document

```yaml
a: v

  # d
b: 1
```

## Expected-Document

```yaml
a: v

# d
b: 1
```
