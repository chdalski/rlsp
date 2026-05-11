---
test-name: block-to-flow-suppressed-by-enforce-block-style
category: block-to-flow
cursor: 0:0
omits-action: flow style
format-options:
  format_enforce_block_style: true
---

# Test: block_to_flow action is suppressed when formatEnforceBlockStyle is true

## Test-Document

```yaml
config:
  a: 1
  b: 2
```
