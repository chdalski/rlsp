---
test-name: anchor-empty-scalar-in-sequence-item
category: anchor
---

# Test: Anchor-Only Sequence Item (Empty Scalar)

An anchor on an empty scalar in a sequence item produces `- &a` with no
trailing space. The next item is a plain scalar and formats normally.

## Test-Document

```yaml
- &a
- a
```

## Expected-Document

```yaml
- &a
- a
```
