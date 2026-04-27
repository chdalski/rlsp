---
test-name: block-to-flow-mapping-to-flow
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Convert block mapping to flow style

## Test-Document

```yaml
config:
  a: 1
  b: 2
```

## Expected-Document

```yaml
config: { a: 1, b: 2 }
```
