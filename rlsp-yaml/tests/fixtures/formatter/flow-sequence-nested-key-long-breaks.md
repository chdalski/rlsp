---
test-name: flow-sequence-nested-key-long-breaks
category: flow-style
settings:
  print_width: 20
---

# Test: Long Flow Sequence Under a Nested Key Closes at the Item Column

The closing bracket of a broken flow sequence is indented to the column of its
items. A bracket at the key's column is not valid YAML, because flow content
inside a block collection must be indented deeper than the parent.

## Test-Document

```yaml
k:
  items: [alpha, bravo, charlie]
```

## Expected-Document

```yaml
k:
  items: [
    alpha,
    bravo,
    charlie
    ]
```
