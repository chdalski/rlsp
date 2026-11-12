---
test-name: block-to-flow-offered-when-the-comment-is-outside-the-collection
category: block-to-flow
cursor: 1:0
applies-action: Convert block to flow style
---

# Test: A comment before the key and a comment after the collection are outside the converted range, so the action stays enabled and keeps both.

## Test-Document

```yaml
# lead
a:
  b: 1
# tail
d: 1
```

## Expected-Document

```yaml
# lead
a: { b: 1 }
# tail
d: 1
```
