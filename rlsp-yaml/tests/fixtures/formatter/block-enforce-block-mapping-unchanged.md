---
test-name: block-enforce-block-mapping-unchanged
category: enforce-block-style
settings:
  format_enforce_block_style: true
---

# Test: enforce_block_style Leaves Block Mapping Unchanged

When `format_enforce_block_style: true`, a mapping already in block style
stays block — no conversion needed.

## Test-Document

```yaml
a: 1
b: 2
```

## Expected-Document

```yaml
a: 1
b: 2
```
