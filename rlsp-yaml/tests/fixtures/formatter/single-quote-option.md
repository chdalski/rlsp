---
test-name: single-quote-option
category: quoting
settings:
  single_quote: true
---

# Test: Single Quote Option

When `single_quote: true`, safe string values are wrapped in single quotes instead
of being emitted as plain scalars. Keys are not affected — the `single_quote`
option is a value-only preference.

## Test-Document

```yaml
key: hello
```

## Expected-Document

```yaml
key: 'hello'
```
