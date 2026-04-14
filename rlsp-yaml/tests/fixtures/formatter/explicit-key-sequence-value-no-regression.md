---
test-name: explicit-key-sequence-value-no-regression
category: explicit-key
---

# Test: Sequence Value Under Plain Key Does Not Trigger Explicit Key Form

A plain scalar key with a block sequence value must render as `key:\n  - item`,
not as explicit key form. Guards against regression.

## Test-Document

```yaml
items:
  - one
  - two
```

## Expected-Document

```yaml
items:
  - one
  - two
```
