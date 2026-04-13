---
test-name: quoting-true-stays-quoted-v1-2
category: quoting
settings:
  yaml_version: "1.2"
---

# Test: Quoted `true` Value Stays Quoted Under YAML 1.2

In YAML 1.2, `true` is a boolean keyword. When the value `"true"` is explicitly
quoted in the source, the formatter preserves the quotes.

Ref: YAML 1.2 §10.3.2 — Core Schema, bool resolution

## Test-Document

```yaml
value: "true"
```

## Expected-Document

```yaml
value: "true"
```
