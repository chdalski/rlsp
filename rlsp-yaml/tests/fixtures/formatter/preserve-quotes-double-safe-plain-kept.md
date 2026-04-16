---
test-name: preserve-quotes-double-safe-plain-kept
category: quoting
settings:
  preserve_quotes: true
---

# Test: Double-Quoted Safe-Plain Scalar Is Kept Quoted

When `preserve_quotes: true`, a double-quoted scalar whose value is safe as plain
is kept double-quoted rather than stripped to a plain scalar.

## Test-Document

```yaml
key: "python"
```

## Expected-Document

```yaml
key: "python"
```
