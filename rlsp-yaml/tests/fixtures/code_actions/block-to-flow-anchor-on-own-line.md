---
test-name: block-to-flow-anchor-on-own-line
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: An anchor on a line of its own moves behind the colon and is printed once.

## Test-Document

```yaml
a:
  &x
  b: 1
d: 1
```

## Expected-Document

```yaml
a: &x { b: 1 }
d: 1
```
