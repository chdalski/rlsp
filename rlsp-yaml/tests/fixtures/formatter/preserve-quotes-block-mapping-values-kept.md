---
test-name: preserve-quotes-block-mapping-values-kept
category: quoting
settings:
  preserve_quotes: true
---

# Test: Block Mapping Double-Quoted Values Are Preserved

Multiple double-quoted safe-plain values in a block mapping all stay quoted when
`preserve_quotes: true`. Plain values stay plain.

## Test-Document

```yaml
name: "my-service"
version: "1.0"
plain: unquoted
```

## Expected-Document

```yaml
name: "my-service"
version: "1.0"
plain: unquoted
```
