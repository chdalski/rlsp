---
test-name: block-to-flow-explicit-key-value-on-next-line
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: The value indicator of an explicit key on its own line keeps its place.

## Test-Document

```yaml
? a
:
  b: 1
```

## Expected-Document

```yaml
? a
: { b: 1 }
```
