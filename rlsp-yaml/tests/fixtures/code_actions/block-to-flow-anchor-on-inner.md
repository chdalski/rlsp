---
test-name: block-to-flow-anchor-on-inner
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Anchor on inner nested collection appears exactly once in flow output

## Test-Document

```yaml
outer:
  inner: &ref
    a: 1
    b: 2
```

## Expected-Document

```yaml
outer: { inner: &ref { a: 1, b: 2 } }
```
