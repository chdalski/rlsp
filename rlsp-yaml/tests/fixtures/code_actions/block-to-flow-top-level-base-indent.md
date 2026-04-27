---
test-name: block-to-flow-top-level-base-indent
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Use loc start column as base indent for top-level block sequence

## Test-Document

```yaml
items:
  - a
```

## Expected-Document

```yaml
items: [a]
```
