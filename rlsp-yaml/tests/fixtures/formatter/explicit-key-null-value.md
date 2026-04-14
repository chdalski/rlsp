---
test-name: explicit-key-null-value
category: explicit-key
---

# Test: Explicit Key with Null Value Renders as Bare `:`

When the key requires explicit form (e.g. a literal block scalar) and the value
is null/empty, the entry renders as `? key\n:` — a bare colon with no trailing
space. Exercises the `value_is_empty` branch in `explicit_key_to_doc`.

## Test-Document

```yaml
? |
  lit key
:
```

## Expected-Document

```yaml
? |
    lit key
:
```
