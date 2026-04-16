---
test-name: preserve-quotes-single-embedded-quote-doubling
category: quoting
settings:
  preserve_quotes: true
---

# Test: Single-Quoted Scalar With Embedded Single-Quote Doubling Is Preserved

A single-quoted YAML scalar uses `''` to encode a literal `'`. When the formatter
re-emits in single-quoted style under `preserve_quotes: true`, it must apply the
same doubling (`value.replace('\'', "''")`) so the embedded quote is correctly
encoded. Decoded value is `it's`; correctly re-emitted as `'it''s'`.

## Test-Document

```yaml
key: 'it''s'
```

## Expected-Document

```yaml
key: 'it''s'
```
