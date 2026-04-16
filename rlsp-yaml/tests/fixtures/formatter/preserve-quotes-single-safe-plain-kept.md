---
test-name: preserve-quotes-single-safe-plain-kept
category: quoting
settings:
  preserve_quotes: true
---

# Test: Single-Quoted Safe-Plain Scalar Is Kept Quoted

When `preserve_quotes: true`, a single-quoted scalar whose value is safe as plain
is kept single-quoted rather than stripped to a plain scalar.

## Test-Document

```yaml
key: 'python'
```

## Expected-Document

```yaml
key: 'python'
```
