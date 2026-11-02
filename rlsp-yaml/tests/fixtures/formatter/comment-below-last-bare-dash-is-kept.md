---
test-name: comment-below-last-bare-dash-is-kept
category: comments
---

# Test: Comment Below The Last Bare Dash Is Kept

A comment line below the last bare `-` of a sequence is kept after the empty item.

## Test-Document

```yaml
- a
-
  # c
```

## Expected-Document

```yaml
- a
- 
# c
```
