---
test-name: block-to-flow-blank-lines-between-colon-and-collection
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: The blank lines between the colon and the collection become one space.

## Test-Document

```yaml
a:

  b: 1
d: 1
```

## Expected-Document

```yaml
a: { b: 1 }
d: 1
```
