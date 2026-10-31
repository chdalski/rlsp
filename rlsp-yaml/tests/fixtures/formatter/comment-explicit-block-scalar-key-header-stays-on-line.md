---
test-name: comment-explicit-block-scalar-key-header-stays-on-line
category: comments
---

# Test: A Comment On An Explicit Block Scalar Key's Header Stays There

The block scalar key keeps the `?` form, and its header line keeps the comment.

## Test-Document

```yaml
? | # c
  k
: v
```

## Expected-Document

```yaml
? |  # c
    k
: v
```
