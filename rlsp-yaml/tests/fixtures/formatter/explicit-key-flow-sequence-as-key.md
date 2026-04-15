---
test-name: explicit-key-flow-sequence-as-key
category: explicit-key
---

# Test: Flow Sequence as Key Uses Explicit Key Form

When the key is a flow sequence (including empty `[]`), the explicit `?` form
is used. Corresponds to conformance case M2N8[1].

The parser produces: outer-mapping{key=inner-mapping{key=[], value=x}, value=""}.
The formatter outputs the inner mapping as a nested explicit key.

Note: the formatter does not yet produce idempotent output for this case
(M2N8[1] is in formatter KNOWN_FAILURES). This fixture verifies the formatter
does not panic or corrupt the output on the first formatting pass.

## Test-Document

```yaml
? []: x
```

## Expected-Document

```yaml
? ? []
  : x
:
```
