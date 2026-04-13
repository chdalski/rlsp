---
test-name: comment-at-document-end
category: comments
---

# Test: Comment at Document End

A comment at the end of the document (after the last value) is preserved.

## Test-Document

```yaml
key: value
# bottom comment
```

## Expected-Document

```yaml
key: value
# bottom comment
```
