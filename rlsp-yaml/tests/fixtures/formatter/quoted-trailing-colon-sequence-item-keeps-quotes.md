---
test-name: quoted-trailing-colon-sequence-item-keeps-quotes
category: quoting
---

# Test: Quoted Sequence Item Ending in a Colon Keeps Its Quotes

Without quotes `- a:` is a block mapping with the key `a`, not the string `a:`.

## Test-Document

```yaml
- "a:"
```

## Expected-Document

```yaml
- "a:"
```
