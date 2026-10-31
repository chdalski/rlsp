---
test-name: comment-duplicate-key-removed-block-scalar-hash-line-is-not-a-comment
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: A line starting with # inside a removed block scalar is content, not a comment

A line starting with # inside a removed block scalar is content, not a comment.

## Test-Document

```yaml
a: |
  # not a comment
  y
b: 1
a: 2
```

## Expected-Document

```yaml
b: 1
a: 2
```
