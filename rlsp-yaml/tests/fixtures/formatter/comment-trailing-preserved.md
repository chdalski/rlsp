---
test-name: comment-trailing-preserved
category: comments
---

# Test: Trailing Comment Preserved on Same Line

A trailing inline comment stays on the same line as the mapping value after formatting.

## Test-Document

```yaml
key: value  # comment
```

## Expected-Document

```yaml
key: value  # comment
```
