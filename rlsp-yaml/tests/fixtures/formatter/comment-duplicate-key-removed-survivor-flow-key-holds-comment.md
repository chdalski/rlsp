---
test-name: comment-duplicate-key-removed-survivor-flow-key-holds-comment
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: The moved comment precedes the comment inside a flow collection key

The moved comment precedes the comment inside a flow collection key.

## Test-Document

```yaml
a: 1 # x
? [b, # in
  c]
: v
a: 2
```

## Expected-Document

```yaml
# x
# in
? [b, c]
: v
a: 2
```
