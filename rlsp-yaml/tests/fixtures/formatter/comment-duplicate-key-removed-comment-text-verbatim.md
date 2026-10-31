---
test-name: comment-duplicate-key-removed-comment-text-verbatim
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: A comment keeps its text when it moves

A comment keeps its text when it moves.

## Test-Document

```yaml
a: 1 #x
a: 2 #
```

## Expected-Document

```yaml
#x
a: 2  #
```
