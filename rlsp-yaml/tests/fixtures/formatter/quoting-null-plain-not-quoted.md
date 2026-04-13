---
test-name: quoting-null-plain-not-quoted
category: quoting
---

# Test: Plain `null` Not Re-Quoted

A plain `null` value is the null scalar in YAML. The formatter must not wrap
it in quotes when it is already unquoted in the source.

## Test-Document

```yaml
value: null
```

## Expected-Document

```yaml
value: null
```
