---
test-name: anchor-root-block-sequence-multi-item
category: anchor
---

# Test: Anchor on Root Block Sequence with Multiple Items

An anchor on a root-level block sequence with multiple items is emitted on its
own line. The anchor line must not include a sequence indicator (`-`).

## Test-Document

```yaml
&list
- x
- y
- z
```

## Expected-Document

```yaml
&list
- x
- y
- z
```
