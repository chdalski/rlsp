---
test-name: block-enforce-block-sequence-unchanged
category: enforce-block-style
settings:
  format_enforce_block_style: true
---

# Test: enforce_block_style Leaves Block Sequence Unchanged

When `format_enforce_block_style: true`, a sequence already in block style
stays block — no conversion needed.

## Test-Document

```yaml
items:
  - a
  - b
```

## Expected-Document

```yaml
items:
  - a
  - b
```
