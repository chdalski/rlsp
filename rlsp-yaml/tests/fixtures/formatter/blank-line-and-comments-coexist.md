---
test-name: blank-line-and-comments-coexist
category: blank-lines
---

# Test: Blank Lines and Section Comments Coexist

A blank line between two sections, where each section has a leading comment,
is preserved along with the comments.

## Test-Document

```yaml
# section one
a: 1

# section two
b: 2
```

## Expected-Document

```yaml
# section one
a: 1

# section two
b: 2
```
