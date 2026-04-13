---
test-name: blank-line-multiple-collapsed-to-one
category: blank-lines
---

# Test: Multiple Consecutive Blank Lines Collapsed to One

Two consecutive blank lines between keys are collapsed to a single blank line.

## Test-Document

```yaml
a: 1


b: 2
```

## Expected-Document

```yaml
a: 1

b: 2
```
