---
test-name: preserve-quotes-keys-kept-quoted
category: quoting
settings:
  preserve_quotes: true
---

# Test: Quoted Mapping Keys Are Preserved

`preserve_quotes` applies to keys as well as values — a quoted key stays quoted.
Unlike `singleQuote`, which has an `in_key` suppression, `preserve_quotes` reproduces
the source style without choosing a new style, so the suppression does not apply.

## Test-Document

```yaml
"key": value
```

## Expected-Document

```yaml
"key": value
```
