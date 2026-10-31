---
test-name: block-scalar-literal-blank-lines-survive-duplicate-key-removal
category: blank-lines
settings:
  format_remove_duplicate_keys: true
---

# Test: Literal Blank Lines Survive Duplicate-Key Removal

Removing a duplicate key changes the number of block scalars in the output; the blank line inside the literal of the remaining key stays.

## Test-Document

```yaml
a: |
  x

  y
b: |
  p
b: 2
```

## Expected-Document

```yaml
a: |
  x

  y
b: 2
```
