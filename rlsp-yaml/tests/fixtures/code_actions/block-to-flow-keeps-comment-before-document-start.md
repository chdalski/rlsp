---
test-name: block-to-flow-keeps-comment-before-document-start
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A comment before the next `---` stays.

## Test-Document

```yaml
a:
  b: 1
# end of doc
---
c: 1
```

## Expected-Document

```yaml
a: { b: 1 }
# end of doc
---
c: 1
```
