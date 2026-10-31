---
test-name: comment-duplicate-key-removed-nested-value-comments-in-source-order
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: Comments inside the value of a removed entry move in source order

Comments inside the value of a removed entry move in source order.

## Test-Document

```yaml
a: # k
  # lead v
  b: 1 # t
  # tail
c: 2
a: 3
```

## Expected-Document

```yaml
# k
# lead v
# t
# tail
c: 2
a: 3
```
