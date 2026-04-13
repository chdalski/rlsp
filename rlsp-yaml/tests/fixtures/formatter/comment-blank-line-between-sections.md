---
test-name: comment-blank-line-between-sections
category: comments
---

# Test: Blank Line Between Sections with Comments Preserved

A blank line separating two sections (each with a leading comment) is preserved.
Both comments appear before their respective keys.

## Test-Document

```yaml
# section 1
key1: v1

# section 2
key2: v2
```

## Expected-Document

```yaml
# section 1
key1: v1

# section 2
key2: v2
```
