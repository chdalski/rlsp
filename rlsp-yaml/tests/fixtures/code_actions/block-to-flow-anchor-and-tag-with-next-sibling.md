---
test-name: block-to-flow-anchor-and-tag-with-next-sibling
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: An anchor and a tag before the collection are printed once.

## Test-Document

```yaml
a: !mytag &x
  b: 1
d: 1
```

## Expected-Document

```yaml
a: &x !mytag { b: 1 }
d: 1
```
