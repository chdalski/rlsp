---
test-name: quoting-on-stays-quoted-v1-1
category: quoting
settings:
  yaml_version: "1.1"
---

# Test: Quoted `on` Value Stays Quoted Under YAML 1.1

In YAML 1.1, `on` is a boolean keyword. When the value `"on"` is explicitly
quoted in the source, the formatter must preserve the quotes to prevent the
value from being interpreted as a boolean.

Ref: YAML 1.1 §10.2.1.2 — Boolean tag resolution

## Test-Document

```yaml
value: "on"
```

## Expected-Document

```yaml
value: "on"
```
