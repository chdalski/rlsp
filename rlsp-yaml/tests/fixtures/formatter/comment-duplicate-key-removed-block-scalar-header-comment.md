---
test-name: comment-duplicate-key-removed-block-scalar-header-comment
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: The comment on the header of a removed block scalar moves

The comment on the header of a removed block scalar moves.

## Test-Document

```yaml
a: | # hdr
  txt
b: 1
a: 2
```

## Expected-Document

```yaml
# hdr
b: 1
a: 2
```
