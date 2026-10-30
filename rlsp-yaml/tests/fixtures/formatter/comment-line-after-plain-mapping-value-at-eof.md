---
test-name: comment-line-after-plain-mapping-value-at-eof
category: comments
---

# Test: Indented Comment Line After Plain Mapping Value At End Of Input

An indented comment line after a plain scalar is not part of the scalar. It ends
the scalar and keeps its own line at the indentation of the enclosing entry.

## Test-Document

```yaml
a: v
  # d
```

## Expected-Document

```yaml
a: v
# d
```
