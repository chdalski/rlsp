---
test-name: block-to-flow-deep-nesting
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Convert three levels of block nesting to flow style recursively

## Test-Document

```yaml
level1:
  level2:
    level3:
      key: val
```

## Expected-Document

```yaml
level1: { level2: { level3: { key: val } } }
```
