---
test-name: comment-duplicate-key-removed-value-sequence-with-inner-comments
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: Comments inside a sequence value of a removed entry move

Comments inside a sequence value of a removed entry move.

## Test-Document

```yaml
a:
  - x # i1
  - # i2
    y: 1 # i3
b: 1
a: 2
```

## Expected-Document

```yaml
# i1
# i2
# i3
b: 1
a: 2
```
