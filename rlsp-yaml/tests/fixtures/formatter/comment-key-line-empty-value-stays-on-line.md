---
test-name: comment-key-line-empty-value-stays-on-line
category: comments
---

# Test: A Comment On The Line Of A Key With No Value Stays On It

The output has the key's line, and no value follows it, so the comment keeps the line.

## Test-Document

```yaml
k: # c
b: 1
```

## Expected-Document

```yaml
k:  # c
b: 1
```
