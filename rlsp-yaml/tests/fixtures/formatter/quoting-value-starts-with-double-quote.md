---
test-name: quoting-value-starts-with-double-quote
category: quoting
---

# Test: Scalar Value Starting with Double Quote Stays Quoted

A scalar whose decoded value starts with a double quote character (`"`) must
remain in a quoted form with the `"` properly escaped. Emitting it as a plain
scalar would make it look like the start of an unterminated double-quoted string
to a re-parser, producing a parse error.

This can occur when the YAML parser encounters a multi-line double-quoted scalar
containing `...` or `---` at a line boundary and loads a truncated plain value
starting with `"`.

## Test-Document

```yaml
"\"value starting with double quote"
```

## Expected-Document

```yaml
"\"value starting with double quote"
```
