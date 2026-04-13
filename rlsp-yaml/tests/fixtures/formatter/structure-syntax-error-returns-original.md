---
test-name: structure-syntax-error-returns-original
category: structure
---

# Test: Syntax Error Returns Original Input

When the input is invalid YAML (unclosed bracket), `format_yaml` returns it
unchanged rather than producing malformed output.

## Test-Document

```yaml
key: [unclosed
```

## Expected-Document

```yaml
key: [unclosed
```
