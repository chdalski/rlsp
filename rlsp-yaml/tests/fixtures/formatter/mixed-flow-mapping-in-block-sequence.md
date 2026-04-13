---
test-name: mixed-flow-mapping-in-block-sequence
category: flow-style
---

# Test: Flow Mapping Inside Block Sequence Is Preserved

A flow mapping used as a block sequence item retains its flow style, including
the bracket spacing from `bracket_spacing: true` (default).

## Test-Document

```yaml
items:
  - {a: 1, b: 2}
```

## Expected-Document

```yaml
items:
  - { a: 1, b: 2 }
```
