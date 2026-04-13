---
test-name: idempotent-block-sequence
category: idempotency
idempotent: true
---

# Test: Block Sequence Is Idempotent

Formatting a block sequence twice produces the same result.

## Test-Document

```yaml
items:
  - one
  - two
```
