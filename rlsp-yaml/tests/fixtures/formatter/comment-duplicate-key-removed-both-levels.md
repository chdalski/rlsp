---
test-name: comment-duplicate-key-removed-both-levels
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: Removal on two levels moves the comments on each level

Removal on two levels moves the comments on each level.

## Test-Document

```yaml
a: 1 # x
b:
  c: 1 # y
  c: 2
a: 2
```

## Expected-Document

```yaml
# x
b:
  # y
  c: 2
a: 2
```
