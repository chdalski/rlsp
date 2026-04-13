---
test-name: quoting-plain-multi-word-not-quoted
category: quoting
---

# Test: Multi-Word Plain String Value Not Quoted

A multi-word plain string like `some value` does not require quoting and is
emitted as-is. The formatter must not add unnecessary quotes.

## Test-Document

```yaml
key: some value
```

## Expected-Document

```yaml
key: some value
```
