---
test-name: comment-block-scalar-header-stays-on-line
category: comments
---

# Test: A Comment On A Block Scalar's Header Line Stays There

The comment is printed after the indicator, never after the content, where it would become part of the value.

## Test-Document

```yaml
a: | # c
  t
```

## Expected-Document

```yaml
a: |  # c
  t
```
