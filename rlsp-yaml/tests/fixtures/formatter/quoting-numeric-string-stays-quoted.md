---
test-name: quoting-numeric-string-stays-quoted
category: quoting
---

# Test: Numeric-Looking String Stays Quoted

A string value like `"123"` that looks like a number must stay quoted to
prevent it from being interpreted as an integer by YAML parsers.

Ref: YAML 1.2 §10.3.2 — Core Schema, integer resolution

## Test-Document

```yaml
version: "123"
```

## Expected-Document

```yaml
version: "123"
```
