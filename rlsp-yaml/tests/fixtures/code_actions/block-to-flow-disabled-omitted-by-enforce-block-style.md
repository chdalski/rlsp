---
test-name: block-to-flow-disabled-omitted-by-enforce-block-style
category: block-to-flow
cursor: 0:0
omits-action: block to flow
format-options:
  format_enforce_block_style: true
---

# Test: With the block style enforced, the action is not offered at all, not even disabled.

## Test-Document

```yaml
a: # note
  b: 1
```
