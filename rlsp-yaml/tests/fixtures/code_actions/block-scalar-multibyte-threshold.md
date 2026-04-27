---
test-name: block-scalar-multibyte-threshold
category: block-scalar
cursor: 0:0
applies-action: block scalar
---

# Test: Offer block-scalar when char count (not byte count) meets 40-char threshold with multibyte

## Test-Document

```yaml
key: "αααααααααααααααααααααααααααααααααααααααα"
```

## Expected-Document

```yaml
key: |
  αααααααααααααααααααααααααααααααααααααααα
```
