---
test-name: quoting-boolean-keyword-stays-quoted
category: quoting
---

# Test: Boolean Keyword String Stays Quoted

A string whose value is a boolean keyword (e.g., "true") must stay quoted to
prevent it from being interpreted as a boolean. The formatter preserves these
quotes regardless of YAML version.

Ref: YAML 1.2 §10.3.2 — Core Schema, bool resolution

## Test-Document

```yaml
value: "true"
```

## Expected-Document

```yaml
value: "true"
```
