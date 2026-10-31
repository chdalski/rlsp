---
test-name: comment-block-scalar-header-between-blank-lines
category: comments
idempotent: true
---

# Test: A Block Scalar's Header Comment Keeps The Blank Lines Around It

The blank lines before and after the scalar stay, and none lands inside its value.

## Test-Document

```yaml
a: 1

b: |  # c
  t

c: 1
```
