---
test-name: quoting-value-starts-with-single-quote
category: quoting
---

# Test: Scalar Value Starting with Single Quote Stays Quoted

A scalar whose decoded value starts with a single quote character (`'`) must
remain in a quoted form. Emitting it as a plain scalar would make it look like
the start of an unterminated single-quoted string to a re-parser.

## Test-Document

```yaml
"'value starting with single quote"
```

## Expected-Document

```yaml
"'value starting with single quote"
```
