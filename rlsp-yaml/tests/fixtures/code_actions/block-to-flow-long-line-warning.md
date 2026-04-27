---
test-name: block-to-flow-long-line-warning
category: block-to-flow
cursor: 0:0
applies-action: (long line)
---

# Test: Offer block-to-flow with long-line warning when result exceeds 80 chars

## Test-Document

```yaml
items:
  - long_item_aaa
  - long_item_bbb
  - long_item_ccc
  - long_item_ddd
  - long_item_eee
  - long_item_fff
```

## Expected-Document

```yaml
items: [
    long_item_aaa,
    long_item_bbb,
    long_item_ccc,
    long_item_ddd,
    long_item_eee,
    long_item_fff
  ]
```
