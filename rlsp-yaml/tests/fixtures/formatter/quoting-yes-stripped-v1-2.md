---
test-name: quoting-yes-stripped-v1-2
category: quoting
settings:
  yaml_version: "1.2"
---

# Test: Quoted `yes` Stripped to Plain Under YAML 1.2

In YAML 1.2, `yes` is not a reserved keyword, so a quoted `"yes"` value can be
safely emitted as a plain scalar. The formatter strips the unnecessary quotes.

Ref: YAML 1.2 §10.3.2 — Core Schema (no bool resolution for on/off/yes/no)

## Test-Document

```yaml
value: "yes"
```

## Expected-Document

```yaml
value: yes
```
