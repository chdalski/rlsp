---
test-name: anchor-sequence-definition
category: anchor
---

# Test: Anchor on a Block Sequence Value

An anchor definition on a block sequence value is emitted on the key line,
before the indented sequence items begin on the next line.

## Test-Document

```yaml
items: &mylist
  - a
  - b
```

## Expected-Document

```yaml
items: &mylist
  - a
  - b
```
