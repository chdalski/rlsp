---
test-name: quoting-double-quoted-leading-space-preserved
category: quoting
---

# Test: Double-Quoted Scalar with Leading Space Preserves Quoting

A double-quoted scalar whose decoded value starts with a space must remain
quoted after formatting. This preserves the leading space, which would be lost
if emitted as a plain scalar.

## Test-Document

```yaml
---
" value with leading space"
```

## Expected-Document

```yaml
" value with leading space"
```
