---
test-name: block-to-flow-comment-before-a-sequence-item-key-stays-offered
category: block-to-flow
cursor: 1:0
applies-action: Convert block to flow style
---

# Test: A comment before the key of a sequence item is outside the converted range.

## Test-Document

```yaml
- # c
  a:
    b: 1
```

## Expected-Document

```yaml
- # c
  a: { b: 1 }
```
