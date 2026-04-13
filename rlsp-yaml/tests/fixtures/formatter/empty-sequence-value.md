---
test-name: empty-sequence-value
category: flow-style
---

# Test: Empty Sequence Value Formats as Brackets

A key with an empty sequence value `[]` is preserved as `[]` after formatting.
The key name does not affect the empty-sequence rendering.

## Test-Document

```yaml
empty_seq: []
```

## Expected-Document

```yaml
empty_seq: []
```
