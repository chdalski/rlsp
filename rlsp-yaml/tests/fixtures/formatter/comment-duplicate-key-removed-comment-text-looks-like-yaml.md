---
test-name: comment-duplicate-key-removed-comment-text-looks-like-yaml
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: A comment that reads like YAML syntax is a comment still

A comment that reads like YAML syntax is a comment still.

## Test-Document

```yaml
a: 1 # ---
b: 1 # x: [y]
a: 2
```

## Expected-Document

```yaml
# ---
b: 1  # x: [y]
a: 2
```
