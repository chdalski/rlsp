---
test-name: explicit-key-idempotent-block-scalar-key
category: explicit-key
idempotent: true
---

# Test: Literal Block Scalar Key Formatting Is Idempotent

Formatting a literal block scalar key twice gives the same result.

## Test-Document

```yaml
? |
  block key
: value
```
