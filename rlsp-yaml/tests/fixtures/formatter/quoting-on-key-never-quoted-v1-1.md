---
test-name: quoting-on-key-never-quoted-v1-1
category: quoting
settings:
  yaml_version: "1.1"
---

# Test: Plain `on` Mapping Key Never Quoted Under YAML 1.1

Even in YAML 1.1 (where `on` is a boolean keyword as a value), a plain `on:`
mapping key must not be quoted. The formatter must preserve it as-is.

Ref: YAML 1.1 §10.2.1.2 — Boolean tag resolution (value context only)

## Test-Document

```yaml
on: push
```

## Expected-Document

```yaml
on: push
```
