---
test-name: interact-preserve-quotes-single-quote
category: quoting
settings:
  preserve_quotes: true
  single_quote: true
---

# Test: preserve_quotes and singleQuote Interaction

`singleQuote` applies only where the formatter has to choose a style — plain scalars
with no source quoting. Already-styled scalars are reproduced verbatim by
`preserve_quotes`. So `"double"` stays double, `'single'` stays single, and `plain`
gets wrapped to `'plain'` by `singleQuote`.

## Test-Document

```yaml
a: "double"
b: 'single'
c: plain
```

## Expected-Document

```yaml
a: "double"
b: 'single'
c: 'plain'
```
