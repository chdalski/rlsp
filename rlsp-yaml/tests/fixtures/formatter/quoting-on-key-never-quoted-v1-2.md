---
test-name: quoting-on-key-never-quoted-v1-2
category: quoting
settings:
  yaml_version: "1.2"
---

# Test: Plain `on` Mapping Key Never Quoted Under YAML 1.2

In YAML 1.2, `on` is not a reserved keyword at all. A plain `on:` mapping key
must not be quoted by the formatter.

## Test-Document

```yaml
on: push
```

## Expected-Document

```yaml
on: push
```
