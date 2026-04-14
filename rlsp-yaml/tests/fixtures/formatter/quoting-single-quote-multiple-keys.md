---
test-name: quoting-single-quote-multiple-keys
category: quoting
settings:
  single_quote: true
---

# Test: Multiple Keys Stay Plain, Values Are Single-Quoted

When `single_quote: true` is set, all plain-safe mapping keys remain unquoted
across a multi-key document, while all plain-safe values are wrapped in single
quotes.

## Test-Document

```yaml
name: alice
role: admin
city: paris
```

## Expected-Document

```yaml
name: 'alice'
role: 'admin'
city: 'paris'
```
