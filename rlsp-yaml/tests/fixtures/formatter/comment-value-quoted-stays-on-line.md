---
test-name: comment-value-quoted-stays-on-line
category: comments
---

# Test: A Comment After A Quoted Value Stays On Its Line

The quotes of the value are dropped; the comment keeps the line it was on.

## Test-Document

```yaml
a: "q" # c
b: 1
```

## Expected-Document

```yaml
a: q  # c
b: 1
```
