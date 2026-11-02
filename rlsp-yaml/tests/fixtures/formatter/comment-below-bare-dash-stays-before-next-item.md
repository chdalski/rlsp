---
test-name: comment-below-bare-dash-stays-before-next-item
category: comments
---

# Test: Comment Below A Bare Dash Stays Before The Next Item

A comment line between a bare `-` and the next item belongs to the next item.
The empty item stays, the comment is kept once, and it stays above `x`.

## Test-Document

```yaml
-
  # c
- x
```

## Expected-Document

```yaml
- 
# c
- x
```
