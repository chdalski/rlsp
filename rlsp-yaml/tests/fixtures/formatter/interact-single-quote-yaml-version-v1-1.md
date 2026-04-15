---
test-name: interact-single-quote-yaml-version-v1-1
category: interaction
settings:
  single_quote: true
  yaml_version: "1.1"
---

# Test: single_quote + yaml_version V1.1 — Must-Quote Values Ignore single_quote Preference

When `single_quote: true` and `yaml_version: "1.1"`, the `single_quote`
preference applies only to values that do not need quoting. Values that are
reserved YAML 1.1 boolean keywords (such as `yes`) must be quoted to prevent
mis-parsing, and the formatter preserves the original quote style rather than
switching to single quotes.

A double-quoted `"yes"` under V1.1 stays `"yes"` — the formatter does not
convert it to `'yes'` because the value must remain quoted for safety and the
original quoting style is preserved for must-quote values.

Contrast with `yaml_version: "1.2"` where `yes` is not a keyword: there,
`"yes"` with `single_quote: true` would be emitted as `'yes'`.

## Test-Document

```yaml
enabled: "yes"
```

## Expected-Document

```yaml
enabled: "yes"
```
