---
test-name: preserve-quotes-plain-stays-plain
category: quoting
settings:
  preserve_quotes: true
---

# Test: Plain Scalar Stays Plain Under preserve_quotes

The `ScalarStyle::Plain` arm is untouched by the preserve branch. A plain scalar
remains plain when `preserve_quotes: true` — the option only affects scalars
that had an explicit quote style in source.

## Test-Document

```yaml
key: python
```

## Expected-Document

```yaml
key: python
```
