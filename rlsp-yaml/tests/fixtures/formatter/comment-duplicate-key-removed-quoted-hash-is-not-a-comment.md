---
test-name: comment-duplicate-key-removed-quoted-hash-is-not-a-comment
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: A # inside a removed quoted scalar is content, not a comment

A # inside a removed quoted scalar is content, not a comment.

## Test-Document

```yaml
a: "q # not"
b: 1
a: 2
```

## Expected-Document

```yaml
b: 1
a: 2
```
