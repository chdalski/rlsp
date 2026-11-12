---
test-name: block-to-flow-tag-on-the-first-key
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A tag on the first key stays on the key; the converted mapping does not get it.

## Test-Document

```yaml
a:
  !t b: 1
d: 1
```

## Expected-Document

```yaml
a: { !t b: 1 }
d: 1
```
