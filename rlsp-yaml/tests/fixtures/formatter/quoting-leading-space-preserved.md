---
test-name: quoting-leading-space-preserved
category: quoting
---

# Test: Single-Quoted Scalar with Leading Space Preserves Quoting

A single-quoted scalar whose value starts with a space must remain quoted after
formatting. Plain scalars have leading whitespace stripped on re-parse (YAML
trims plain scalar content), so emitting the value unquoted would drop the
leading space and break idempotency.

## Test-Document

```yaml
---
' value with leading space'
```

## Expected-Document

```yaml
' value with leading space'
```
