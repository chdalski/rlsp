---
test-name: comment-document-level-with-duplicate-keys-removed
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: Document-Level Comments Survive Duplicate-Key Removal

Removing a duplicate key leaves the comments around the document where they are.

## Test-Document

```yaml
# p
---
# m
a: 1
a: 2
# e
...
# z
```

## Expected-Document

```yaml
# p
---
# m
a: 2
# e
...
# z
```
