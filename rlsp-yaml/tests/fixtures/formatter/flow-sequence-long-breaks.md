---
test-name: flow-sequence-long-breaks
category: flow-style
settings:
  print_width: 20
---

# Test: Long Flow Sequence Breaks Across Lines

When a flow sequence is too wide to fit on one line within `print_width`, the
Wadler-Lindig pretty-printer breaks it across lines with indented items.

## Test-Document

```yaml
items: [alpha, bravo, charlie]
```

## Expected-Document

```yaml
items: [
  alpha,
  bravo,
  charlie
]
```
