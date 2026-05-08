---
test-name: block-to-flow-nested-mapping
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Convert nested block mapping to flow style recursively

## Test-Document

```yaml
outer:
  inner:
    a: 1
    b: 2
```

## Expected-Document

```yaml
outer: { inner: { a: 1, b: 2 } }
```
