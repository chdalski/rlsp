---
test-name: mixed-nested-flow-sequence-of-mappings
category: flow-style
---

# Test: Nested Flow — Sequence of Flow Mappings

A flow sequence whose items are flow mappings preserves both the outer flow
sequence and the inner flow mappings.

## Test-Document

```yaml
data: [{a: 1}, {b: 2}]
```

## Expected-Document

```yaml
data: [{ a: 1 }, { b: 2 }]
```
