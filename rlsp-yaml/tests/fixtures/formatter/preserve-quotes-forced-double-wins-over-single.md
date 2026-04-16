---
test-name: preserve-quotes-forced-double-wins-over-single
category: quoting
settings:
  preserve_quotes: true
  single_quote: true
---

# Test: Spec-Forced Double Quoting Overrides preserve_quotes and singleQuote

A scalar whose decoded value requires double quoting (control characters, backslash
sequences) is always emitted as double-quoted, regardless of `preserve_quotes: true`
or `single_quote: true`. The spec overrides both user preferences.

The value `"value\nline2"` is a double-quoted YAML scalar containing a literal
newline — `requires_double_quoting` returns true for it. With `single_quote: true`
and `preserve_quotes: true`, the output must still be double-quoted.

## Test-Document

```yaml
key: "value\nline2"
```

## Expected-Document

```yaml
key: "value\nline2"
```
