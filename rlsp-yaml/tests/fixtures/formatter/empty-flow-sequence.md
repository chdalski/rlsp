---
test-name: empty-flow-sequence
category: flow-style
---

# Test: Empty Flow Sequence Stays as Brackets

An empty flow sequence `[]` is preserved as-is — it does not get converted to
a block sequence.

## Test-Document

```yaml
items: []
```

## Expected-Document

```yaml
items: []
```
