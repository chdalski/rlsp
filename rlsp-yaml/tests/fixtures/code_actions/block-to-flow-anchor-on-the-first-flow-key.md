---
test-name: block-to-flow-anchor-on-the-first-flow-key
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: An anchor on a first key that is a flow sequence stays on that key.

## Test-Document

```yaml
a:
  &k [x]: 1
  c: 2
d: 1
```

## Expected-Document

```yaml
a: { &k [x]: 1, c: 2 }
d: 1
```
