---
test-name: block-to-flow-wrapped-output-keeps-following-comment
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A flow collection that wraps over several lines is followed by the comment as before.

## Test-Document

```yaml
items:
  - long_item_aaa
  - long_item_bbb
  - long_item_ccc
  - long_item_ddd
  - long_item_eee
  - long_item_fff
# after
next: 1
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
# after
next: 1
```
