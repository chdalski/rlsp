---
test-name: preserve-quotes-off-strips-single-as-before
category: quoting
---

# Test: Default Behavior Strips Single Quotes From Safe-Plain Scalars

When `preserve_quotes` is `false` (the default), safe-plain single-quoted scalars
are still stripped to plain. Backward-compat guard.

## Test-Document

```yaml
key: 'python'
```

## Expected-Document

```yaml
key: python
```
