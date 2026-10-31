---
test-name: comment-block-scalar-header-idempotent
category: comments
idempotent: true
---

# Test: A Comment On A Block Scalar's Header Is Stable

The header comment is printed after the indicator and formatting it again changes nothing.

## Test-Document

```yaml
a: |  # c
  t
```
