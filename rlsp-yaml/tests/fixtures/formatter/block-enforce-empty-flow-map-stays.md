---
test-name: block-enforce-empty-flow-map-stays
category: enforce-block-style
settings:
  format_enforce_block_style: true
---

# Test: enforce_block_style Leaves Empty Flow Mapping as Braces

When `format_enforce_block_style: true`, an empty flow mapping `{}` stays as
`{}` — there are no entries to convert to block style.

## Test-Document

```yaml
status: {}
```

## Expected-Document

```yaml
status: {}
```
