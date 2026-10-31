---
test-name: comment-duplicate-key-removed-explicit-key-and-value-comments
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: Comments on the key and on the value of a removed explicit entry both move

Comments on the key and on the value of a removed explicit entry both move.

## Test-Document

```yaml
? a # kc
: 1 # vc
b: 2
a: 3
```

## Expected-Document

```yaml
# kc
# vc
b: 2
a: 3
```
