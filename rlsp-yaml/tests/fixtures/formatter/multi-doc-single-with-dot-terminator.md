---
test-name: multi-doc-single-with-dot-terminator
category: structure
---

# Test: Single Document Closed by `...` Terminator

A single document closed with `...` (no following document) is parsed as one
document. The formatter preserves the `...` end marker because `explicit_end`
is true on the loaded document.

## Test-Document

```yaml
key: value
...
```

## Expected-Document

```yaml
key: value
...
```
