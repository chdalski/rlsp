---
test-name: comment-line-after-plain-sequence-item-indented
category: comments
---

# Test: Indented Comment Line After Plain Sequence Item

An indented comment line after a plain scalar is not part of the scalar. It ends
the scalar and keeps its own line at the indentation of the enclosing entry.

## Test-Document

```yaml
- v
  # d
- z
```

## Expected-Document

```yaml
- v
# d
- z
```
