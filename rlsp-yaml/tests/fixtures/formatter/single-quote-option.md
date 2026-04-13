---
test-name: single-quote-option
category: quoting
settings:
  single_quote: true
---

# Test: Single Quote Option

When `single_quote: true`, safe string values are wrapped in single quotes instead
of being emitted as plain scalars.

## Test-Document

```yaml
key: hello
```

## Expected-Document

```yaml
'key': 'hello'
```
