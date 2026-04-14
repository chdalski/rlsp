---
test-name: explicit-key-idempotent-sequence-as-key
category: explicit-key
idempotent: true
---

# Test: Block Sequence Key Formatting Is Idempotent

Formatting a block sequence key twice gives the same result.

## Test-Document

```yaml
? - a
  - b
: - c
  - d
```
