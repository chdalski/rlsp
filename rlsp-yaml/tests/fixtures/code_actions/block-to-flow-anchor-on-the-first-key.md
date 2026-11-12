---
test-name: block-to-flow-anchor-on-the-first-key
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: An anchor on the first key stays on the key; the converted mapping does not get it.

## Test-Document

```yaml
a:
  &k b: 1
  c: 2
d: 1
```

## Expected-Document

```yaml
a: { &k b: 1, c: 2 }
d: 1
```
