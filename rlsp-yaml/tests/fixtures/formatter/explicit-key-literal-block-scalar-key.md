---
test-name: explicit-key-literal-block-scalar-key
category: explicit-key
---

# Test: Literal Block Scalar as Key Uses Explicit Key Form

When the key is a literal block scalar (`|`), the entry must use `? key\n: value` form.
Corresponds to conformance case 5WE3.

## Test-Document

```yaml
? |
  block key
: - one
  - two
```

## Expected-Document

```yaml
? |
    block key
:
  - one
  - two
```
