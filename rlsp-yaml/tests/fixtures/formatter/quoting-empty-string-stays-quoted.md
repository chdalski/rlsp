---
test-name: quoting-empty-string-stays-quoted
category: quoting
---

# Test: Empty String Value Stays Quoted

An empty string value `""` must stay quoted in the output. An unquoted empty
value would be interpreted as null by YAML parsers.

Ref: YAML 1.2 §10.3.2 — Null resolution (empty scalar → null)

## Test-Document

```yaml
key: ""
```

## Expected-Document

```yaml
key: ""
```
