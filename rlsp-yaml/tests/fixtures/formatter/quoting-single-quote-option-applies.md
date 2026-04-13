---
test-name: quoting-single-quote-option-applies
category: quoting
settings:
  single_quote: true
---

# Test: `single_quote: true` Wraps Values in Single Quotes

When `single_quote: true` is set, string values that would otherwise be plain
are wrapped in single quotes instead.

Note: the formatter currently also single-quotes plain-safe mapping keys (e.g.,
`value` becomes `'value'`). This behavior reflects the formatter's actual
implementation. This test documents and pins that behavior.

## Test-Document

```yaml
value: "python"
```

## Expected-Document

```yaml
'value': 'python'
```
