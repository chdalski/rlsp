---
test-name: block-to-flow-keeps-document-end-marker
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: The `...` marker after the collection stays.

## Test-Document

```yaml
a:
  b: 1
...
```

## Expected-Document

```yaml
a: { b: 1 }
...
```
