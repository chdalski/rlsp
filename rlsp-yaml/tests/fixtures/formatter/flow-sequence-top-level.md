---
test-name: flow-sequence-top-level
category: flow-style
---

# Test: Top-Level Flow Sequence Is Preserved

A flow sequence as a top-level mapping value is preserved as flow style.
Safe quoted strings have quotes stripped.

## Test-Document

```yaml
items: ["a", "b", "c"]
```

## Expected-Document

```yaml
items: [a, b, c]
```
