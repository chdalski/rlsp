---
test-name: quoting-false-plain-not-quoted
category: quoting
---

# Test: Plain Boolean `false` Not Re-Quoted

A plain `false` value is a boolean in YAML. The formatter must not wrap it in
quotes when it is already unquoted in the source.

## Test-Document

```yaml
active: false
```

## Expected-Document

```yaml
active: false
```
