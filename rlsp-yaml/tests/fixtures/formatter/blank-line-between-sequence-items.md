---
test-name: blank-line-between-sequence-items
category: blank-lines
---

# Test: Blank Line Between Sequence Items Preserved

A blank line between two sequence items is preserved after formatting.

## Test-Document

```yaml
items:
  - a: 1

  - b: 2
```

## Expected-Document

```yaml
items:
  - a: 1

  - b: 2
```
