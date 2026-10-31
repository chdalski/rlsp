---
test-name: comment-multiline-plain-value-with-blank-line-quoted
category: comments
---

# Test: A Comment After A Plain Value With A Blank Line Follows The Quoted Value

The value holds a line break, so it is printed double-quoted on one line, and the comment follows it.

## Test-Document

```yaml
a: v

  w # c
```

## Expected-Document

```yaml
a: "v\nw"  # c
```
