---
test-name: flow-sequence-multi-item
category: flow-style
---

# Test: Multi-Item Flow Sequence Stays on One Line

A short multi-item flow sequence stays inline when it fits within `print_width`.

## Test-Document

```yaml
items: [a, b, c]
```

## Expected-Document

```yaml
items: [a, b, c]
```
