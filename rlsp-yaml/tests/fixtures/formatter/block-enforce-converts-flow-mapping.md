---
test-name: block-enforce-converts-flow-mapping
category: enforce-block-style
settings:
  format_enforce_block_style: true
---

# Test: enforce_block_style Converts Flow Mapping to Block

When `format_enforce_block_style: true`, a non-empty flow mapping `{a: 1, b: 2}` is
converted to block entries.

## Test-Document

```yaml
meta: {a: 1, b: 2}
```

## Expected-Document

```yaml
meta:
  a: 1
  b: 2
```
