---
test-name: quoting-yes-stays-quoted-v1-1
category: quoting
settings:
  yaml_version: "1.1"
---

# Test: Quoted `yes` Value Stays Quoted Under YAML 1.1

In YAML 1.1, `yes` is a boolean keyword. When the value `"yes"` is explicitly
quoted in the source, the formatter must preserve the quotes to prevent the
value from being interpreted as a boolean.

Ref: YAML 1.1 §10.2.1.2 — Boolean tag resolution

## Test-Document

```yaml
value: "yes"
```

## Expected-Document

```yaml
value: "yes"
```
