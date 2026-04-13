---
test-name: quoting-on-key-plain-not-quoted
category: quoting
---

# Test: Plain `on` Mapping Key Not Quoted

A plain `on:` mapping key must not be quoted by the formatter. In YAML 1.2
`on` is not a reserved keyword; in YAML 1.1 it is a boolean, but only as a
*value* — as a mapping key, quoting it would be incorrect. The formatter must
preserve it as plain in both versions.

Ref: YAML 1.1 §10.2.1.2 — Boolean tag resolution (value context only)

## Test-Document

```yaml
on: push
```

## Expected-Document

```yaml
on: push
```
