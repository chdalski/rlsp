---
test-name: quoting-true-plain-not-quoted
category: quoting
---

# Test: Plain Boolean `true` Not Re-Quoted

A plain `true` value is a boolean in YAML. The formatter must not wrap it in
quotes when it is already unquoted in the source.

## Test-Document

```yaml
enabled: true
```

## Expected-Document

```yaml
enabled: true
```
