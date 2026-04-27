---
test-name: block-to-flow-safe-items-unquoted
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Safe items not quoted when converting block sequence to flow

## Test-Document

```yaml
items:
  - one
  - two
```

## Expected-Document

```yaml
items: [one, two]
```
