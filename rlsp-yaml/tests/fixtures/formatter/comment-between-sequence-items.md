---
test-name: comment-between-sequence-items
category: comments
---

# Test: Comment Between Sequence Items

A comment placed between two sequence items is preserved, appearing after the
first item and before the second.

## Test-Document

```yaml
items:
  - item1
  # between
  - item2
```

## Expected-Document

```yaml
items:
  - item1
  # between
  - item2
```
