---
test-name: quoting-single-quote-flow-mapping-key-not-quoted
category: quoting
settings:
  single_quote: true
---

# Test: `single_quote: true` Does Not Quote Flow-Mapping Keys

When `single_quote: true` is set, plain-safe keys in flow-style mappings remain
unquoted. The `single_quote` option applies to values only, for both block and
flow mappings.

## Test-Document

```yaml
{key: value}
```

## Expected-Document

```yaml
{ key: 'value' }
```
