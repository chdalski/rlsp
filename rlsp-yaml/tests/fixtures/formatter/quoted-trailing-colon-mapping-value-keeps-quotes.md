---
test-name: quoted-trailing-colon-mapping-value-keeps-quotes
category: quoting
---

# Test: Quoted Mapping Value Ending in a Colon Keeps Its Quotes

A plain scalar cannot end in `:` (`ns-plain-char(c)` accepts `:` only before a
safe character), so `"a:"` stays quoted. Without quotes `k: a:` does not load.

## Test-Document

```yaml
k: "a:"
```

## Expected-Document

```yaml
k: "a:"
```
