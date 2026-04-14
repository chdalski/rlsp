---
test-name: quoting-single-quote-key-not-quoted
category: quoting
settings:
  single_quote: true
---

# Test: `single_quote: true` Does Not Quote Plain-Safe Keys

When `single_quote: true` is set, mapping keys that are plain-safe remain
unquoted. The `single_quote` option applies to values only.

## Test-Document

```yaml
key: hello
```

## Expected-Document

```yaml
key: 'hello'
```
