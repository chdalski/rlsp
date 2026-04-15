---
test-name: quoting-value-starts-with-double-quote
category: quoting
---

# Test: Scalar Value Starting with Double Quote Stays Quoted

A scalar whose decoded value starts with a double quote character (`"`) must
remain in a quoted form with the `"` properly escaped. Emitting it as a plain
scalar would make it look like the start of an unterminated double-quoted string
to a re-parser, producing a parse error.

`needs_quoting` returns `true` for values starting with `"`, so the formatter
re-emits them in the appropriate quoted form rather than as plain scalars.

## Test-Document

```yaml
"\"value starting with double quote"
```

## Expected-Document

```yaml
"\"value starting with double quote"
```
