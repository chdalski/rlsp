---
test-name: flow-sequence-idempotent-long-broken
category: flow-style
idempotent: true
settings:
  print_width: 40
---

# Test: Long Broken Flow Sequence Is Idempotent

A flow sequence that breaks across lines due to a narrow `print_width` remains
stable when formatted a second time.

## Test-Document

```yaml
items: [alpha, bravo, charlie, delta, echo, foxtrot]
```
