---
test-name: comment-duplicate-key-removed-multi-document-no-crossing
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: Comments stay in their document

Comments stay in their document.

## Test-Document

```yaml
a: 1 # x
a: 2
---
b: 1 # y
b: 2
```

## Expected-Document

```yaml
# x
a: 2
---
# y
b: 2
```
