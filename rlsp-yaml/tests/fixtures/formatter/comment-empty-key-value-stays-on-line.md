---
test-name: comment-empty-key-value-stays-on-line
category: comments
---

# Test: A Comment After A Value Of An Empty Key Stays On Its Line

The entry with no key keeps the comment on its line.

## Test-Document

```yaml
: v # c
```

## Expected-Document

```yaml
: v  # c
```
