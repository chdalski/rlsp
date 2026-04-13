---
test-name: block-enforce-idempotent
category: enforce-block-style
idempotent: true
settings:
  format_enforce_block_style: true
---

# Test: enforce_block_style Is Idempotent

When `format_enforce_block_style: true`, formatting twice produces the same
result — flow is converted to block on the first pass, and the second pass
leaves block unchanged.

## Test-Document

```yaml
items: [a, b]
```
