---
test-name: explicit-key-mapping-as-key
category: explicit-key
---

# Test: Mapping as Key Uses Explicit Key Form

When the key is a block mapping, the entry must use `? key\n: value` form.
Corresponds to part of conformance case V9D5.

## Test-Document

```yaml
? earth: blue
: moon: white
```

## Expected-Document

```yaml
? earth: blue
:
  moon: white
```
