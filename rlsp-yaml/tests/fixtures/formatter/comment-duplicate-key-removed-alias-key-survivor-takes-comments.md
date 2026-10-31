---
test-name: comment-duplicate-key-removed-alias-key-survivor-takes-comments
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: An alias key that stays takes the comments of the removed alias entry

An alias key that stays takes the comments of the removed alias entry.

## Test-Document

```yaml
? *r # kc
: 1 # vc
? *r
: 2
```

## Expected-Document

```yaml
# kc
# vc
*r : 2
```
