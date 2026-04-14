---
test-name: explicit-key-folded-block-scalar-key
category: explicit-key
---

# Test: Folded Block Scalar as Key Uses Explicit Key Form

When the key is a folded block scalar (`>`), the entry must use `? key\n: value` form.

## Test-Document

```yaml
? >
  folded key
: value
```

## Expected-Document

```yaml
? >
    folded key
: value
```
