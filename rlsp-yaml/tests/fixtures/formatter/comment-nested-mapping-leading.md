---
test-name: comment-nested-mapping-leading
category: comments
---

# Test: Leading Comments Between Entries in a Nested Mapping Preserved

Leading comments that appear between entries in a nested block mapping are
preserved in the formatted output. The indentation of sequence items that were
at the wrong level is also corrected.

## Test-Document

```yaml
Lists:
  # Style 1
  list-a:
    - item1
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
    - item1
    - item2

  # Style 2
  list-b:
    - item1
    - item2
```
