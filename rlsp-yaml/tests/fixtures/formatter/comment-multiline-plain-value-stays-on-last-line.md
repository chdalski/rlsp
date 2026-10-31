---
test-name: comment-multiline-plain-value-stays-on-last-line
category: comments
---

# Test: A Comment After A Multi-Line Plain Value Stays On The Line It Ends On

The value is joined onto one line and the comment follows it.

## Test-Document

```yaml
a:
  v
  w # c
```

## Expected-Document

```yaml
a: v w  # c
```
