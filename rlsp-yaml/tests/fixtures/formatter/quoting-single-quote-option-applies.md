---
test-name: quoting-single-quote-option-applies
category: quoting
settings:
  single_quote: true
---

# Test: `single_quote: true` Wraps Values in Single Quotes

When `single_quote: true` is set, string values that would otherwise be plain
are wrapped in single quotes instead. Mapping keys are not affected — the
`single_quote` option is a value-only preference.

## Test-Document

```yaml
value: "python"
```

## Expected-Document

```yaml
value: 'python'
```
