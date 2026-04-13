---
test-name: quoting-true-stays-quoted-v1-1
category: quoting
settings:
  yaml_version: "1.1"
---

# Test: Quoted `true` Value Stays Quoted Under YAML 1.1

In YAML 1.1, `true` is a boolean keyword. When the value `"true"` is explicitly
quoted in the source, the formatter preserves the quotes.

Ref: YAML 1.1 §10.2.1.2 — Boolean tag resolution

## Test-Document

```yaml
value: "true"
```

## Expected-Document

```yaml
value: "true"
```
