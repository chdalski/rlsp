---
test-name: block-to-flow-empty-last-sequence-item
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A bare `-` as last item ends the edit right after the dash; the comment stays.

The formatter prints an empty flow sequence item as `!!null ""` on purpose (see `flow_seq_item_to_doc`), so the data stays the same in flow style. The fixture pins the range of the edit and the text around it.

## Test-Document

```yaml
a:
  - x
  -
# n
d: 1
```

## Expected-Document

```yaml
a: [x, !!null ""]
# n
d: 1
```
