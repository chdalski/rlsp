---
test-name: explicit-key-flow-sequence-as-key
category: explicit-key
---

# Test: Empty Flow Sequence as Key Uses Inline Implicit Key Form

When the key is an empty flow sequence `[]`, the formatter uses inline implicit
key form (no explicit `?` prefix). Corresponds to conformance case M2N8[1].

The parser produces: outer-mapping{key=inner-mapping{key=[], value=x}, value=""}.
The inner mapping key is `[]` (empty flow sequence), which is safe as an inline
implicit key — it renders as a single token and causes no re-parsing ambiguity.

## Test-Document

```yaml
? []: x
```

## Expected-Document

```yaml
? []: x
:
```
