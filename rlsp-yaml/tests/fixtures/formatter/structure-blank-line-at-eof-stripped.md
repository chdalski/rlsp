---
test-name: structure-blank-line-at-eof-stripped
category: structure
---

# Test: Trailing Blank Line at EOF Is Stripped

A blank line at the end of the input is removed by the formatter.

## Test-Document

```yaml
a: 1

```

## Expected-Document

```yaml
a: 1
```
