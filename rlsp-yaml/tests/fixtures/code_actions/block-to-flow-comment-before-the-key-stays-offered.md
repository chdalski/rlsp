---
test-name: block-to-flow-comment-before-the-key-stays-offered
category: block-to-flow
cursor: 1:0
applies-action: Convert block to flow style
---

# Test: A comment before the key is outside the converted range, so the action stays enabled and keeps it.

## Test-Document

```yaml
# top
a:
  b: 1
d: 1
```

## Expected-Document

```yaml
# top
a: { b: 1 }
d: 1
```
