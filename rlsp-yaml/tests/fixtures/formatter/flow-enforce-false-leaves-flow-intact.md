---
test-name: flow-enforce-false-leaves-flow-intact
category: enforce-block-style
---

# Test: enforce_block_style False Leaves Flow Intact

When `format_enforce_block_style: false` (the default), flow sequences are
preserved as flow — no conversion to block style.

## Test-Document

```yaml
items: [a, b]
```

## Expected-Document

```yaml
items: [a, b]
```
