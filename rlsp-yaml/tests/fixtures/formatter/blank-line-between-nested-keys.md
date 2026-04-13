---
test-name: blank-line-between-nested-keys
category: blank-lines
---

# Test: Blank Line Between Nested Keys Preserved

A blank line between two sibling keys in a nested mapping is preserved.

## Test-Document

```yaml
parent:
  a: 1

  b: 2
```

## Expected-Document

```yaml
parent:
  a: 1

  b: 2
```
