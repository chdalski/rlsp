---
test-name: comment-nested-mapping-leading-and-trailing
category: comments
---

# Test: Leading and Trailing Comments in Nested Mapping Both Preserved

When a nested mapping entry has both a leading comment (before the key) and a
trailing inline comment (after a value in the sequence), both comments survive
formatting.

## Test-Document

```yaml
Lists:
  # Style 1
  list-a:
    - item1 # another comment
    - item2

  # Style 2
  list-b:
  - item1
  - item2
```

## Expected-Document

```yaml
Lists:
  # Style 1
  list-a:
    - item1  # another comment
    - item2

  # Style 2
  list-b:
    - item1
    - item2
```
