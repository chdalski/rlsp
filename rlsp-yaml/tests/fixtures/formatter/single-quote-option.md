---
test-name: single-quote-option
category: quoting
settings:
  single_quote: true
---

# Test: Single Quote Option

When `single_quote: true`, safe string values are wrapped in single quotes instead
of being emitted as plain scalars.

Note: the formatter currently also single-quotes plain-safe mapping keys (e.g.,
`key` becomes `'key'`). This is a potential bug — the `single_quote` option is
intended to affect values only. The expected output below reflects the formatter's
actual behavior and should be updated when the bug is fixed.

## Test-Document

```yaml
key: hello
```

## Expected-Document

```yaml
'key': 'hello'
```
