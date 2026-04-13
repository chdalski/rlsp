---
test-name: quoting-explicit-quoted-on-key-stays-quoted-v1-1
category: quoting
settings:
  yaml_version: "1.1"
---

# Test: Explicitly Quoted `"on"` Key Stays Quoted Under YAML 1.1

When `"on"` appears as an explicitly double-quoted *key*, the formatter must
preserve the quotes in YAML 1.1 to maintain the original intent of the author
(distinguishing the string from the boolean).

Ref: YAML 1.1 §10.2.1.2 — Boolean tag resolution

## Test-Document

```yaml
"on": push
```

## Expected-Document

```yaml
"on": push
```
