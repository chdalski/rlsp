---
test-name: block-scalar-hash-line-is-content-not-comment
category: comments
idempotent: true
---

# Test: A Hash Line Inside A Block Scalar Is Content, Not A Comment

The `# d` line is part of the scalar's value and is printed once; the `# e` line below the scalar is a comment and is printed once.

## Test-Document

```yaml
a: |
  t
  # d
# e
```
