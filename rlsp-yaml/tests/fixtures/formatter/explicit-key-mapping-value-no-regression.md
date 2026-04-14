---
test-name: explicit-key-mapping-value-no-regression
category: explicit-key
---

# Test: Mapping Value Under Plain Key Does Not Trigger Explicit Key Form

A plain scalar key with a block mapping value must render as `key:\n  child: val`,
not as explicit key form. Guards against regression.

## Test-Document

```yaml
key:
  child: val
```

## Expected-Document

```yaml
key:
  child: val
```
