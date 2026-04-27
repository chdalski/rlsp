---
test-name: block-to-flow-wraps-long-output
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: Wrap flow output when single-line form exceeds print_width

When the single-line flow form of the converted collection exceeds the
configured `formatPrintWidth` (default 80), the formatter breaks it across
multiple lines.

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
