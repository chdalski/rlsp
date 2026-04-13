---
test-name: quoting-hash-start-stays-quoted
category: quoting
---

# Test: String Starting with Hash Stays Quoted

A string that starts with `#` would be interpreted as a comment if unquoted.
The formatter preserves quotes on such values.

Ref: YAML 1.2 §6.8 — Comments

## Test-Document

```yaml
value: "#comment"
```

## Expected-Document

```yaml
value: "#comment"
```
