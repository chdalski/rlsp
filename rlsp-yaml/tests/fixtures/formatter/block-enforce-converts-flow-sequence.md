---
test-name: block-enforce-converts-flow-sequence
category: enforce-block-style
settings:
  format_enforce_block_style: true
---

# Test: enforce_block_style Converts Flow Sequence to Block

When `format_enforce_block_style: true`, a flow sequence `[a, b, c]` is
converted to block `- ` items.

## Test-Document

```yaml
items: [a, b, c]
```

## Expected-Document

```yaml
items:
  - a
  - b
  - c
```
