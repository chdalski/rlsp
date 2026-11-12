---
test-name: block-to-flow-space-before-colon
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A space between the key and its colon stays; the colon is not lost.

## Test-Document

```yaml
a :
  b: 1
d: 1
```

## Expected-Document

```yaml
a : { b: 1 }
d: 1
```
