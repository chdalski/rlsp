---
test-name: comment-line-after-plain-value-keeps-single-comment
category: comments
---

# Test: Indented Comment Line After Plain Value Is Not Duplicated

An indented comment line after a plain scalar is not part of the scalar. It ends
the scalar and keeps its own line at the indentation of the enclosing entry.

## Test-Document

```yaml
port: 8080
  # default
```

## Expected-Document

```yaml
port: 8080
# default
```
