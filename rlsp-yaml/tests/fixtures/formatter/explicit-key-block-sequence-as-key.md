---
test-name: explicit-key-block-sequence-as-key
category: explicit-key
---

# Test: Block Sequence as Key Uses Explicit Key Form

When the key is a block sequence, the entry must use `? key\n: value` form.
Corresponds to conformance case 6PBE (partially — the full case has loader bugs).

## Test-Document

```yaml
? - a
  - b
: - c
  - d
```

## Expected-Document

```yaml
? - a
  - b
:
  - c
  - d
```
