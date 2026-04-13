---
test-name: quoting-number-like-string-stays-quoted
category: quoting
---

# Test: Number-Like String Stays Quoted

A value that looks like a number (e.g., "5000") must stay quoted to prevent
it from being interpreted as an integer. The formatter preserves these quotes.

Ref: YAML 1.2 §10.3.2 — Core Schema, integer and float resolution

## Test-Document

```yaml
value: "5000"
```

## Expected-Document

```yaml
value: "5000"
```
