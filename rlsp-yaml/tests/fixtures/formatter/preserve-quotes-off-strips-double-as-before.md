---
test-name: preserve-quotes-off-strips-double-as-before
category: quoting
---

# Test: Default Behavior Strips Double Quotes From Safe-Plain Scalars

When `preserve_quotes` is `false` (the default), safe-plain double-quoted scalars
are still stripped to plain. Backward-compat guard.

## Test-Document

```yaml
key: "python"
```

## Expected-Document

```yaml
key: python
```
