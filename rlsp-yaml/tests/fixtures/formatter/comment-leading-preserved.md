---
test-name: comment-leading-preserved
category: comments
---

# Test: Leading Comment Preserved Before Key

A comment line before a mapping key is preserved, appearing before the key in output.

## Test-Document

```yaml
# header
key: value
```

## Expected-Document

```yaml
# header
key: value
```
