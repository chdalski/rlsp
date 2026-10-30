---
test-name: flow-sequence-in-sequence-item-long-breaks
category: flow-style
settings:
  print_width: 20
---

# Test: Long Flow Sequence in a Sequence Item Closes at the Item Column

A flow sequence that breaks inside a block sequence item closes at the column
of its items.

## Test-Document

```yaml
- [alpha, bravo, charlie]
```

## Expected-Document

```yaml
- [
  alpha,
  bravo,
  charlie
  ]
```
