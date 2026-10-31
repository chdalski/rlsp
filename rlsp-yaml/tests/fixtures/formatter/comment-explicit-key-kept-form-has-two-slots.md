---
test-name: comment-explicit-key-kept-form-has-two-slots
category: comments
---

# Test: A Comment On An Explicit Key's Line Stays When The Key Keeps The Explicit Form

A flow collection key keeps the `?` form, so the key line and the value line each have a slot.

## Test-Document

```yaml
? [a, b] # c
: v # d
```

## Expected-Document

```yaml
? [a, b]  # c
: v  # d
```
