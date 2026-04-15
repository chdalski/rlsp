---
test-name: explicit-key-flow-sequence-as-key
category: explicit-key
---

# Test: Flow Sequence as Key Uses Explicit Key Form

When the key is a flow sequence (including empty `[]`), the entry uses `? key\n: value` form.
Corresponds to conformance case M2N8[1].

Note: the parser currently treats `? []: x` as outer-mapping{key=[], value={"": "x"}} rather
than the spec-correct outer-mapping{key={"[]": "x"}, value=""} (M2N8[1] is in KNOWN_FAILURES).
The expected output reflects what the formatter produces from the current (non-conformant) AST.

## Test-Document

```yaml
? []: x
```

## Expected-Document

```yaml
? []
:
  : x
```
