---
test-name: explicit-key-flow-sequence-as-key
category: explicit-key
---

# Test: Flow Sequence as Key Uses Explicit Key Form

When the key is a flow sequence (including empty `[]`), the entry uses `? key\n: value` form.
Corresponds to conformance case M2N8[1].

## Test-Document

```yaml
? []: x
```

## Expected-Document

```yaml
? []
:
  x: 
```
