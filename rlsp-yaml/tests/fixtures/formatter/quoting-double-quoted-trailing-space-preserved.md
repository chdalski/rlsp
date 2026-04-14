---
test-name: quoting-double-quoted-trailing-space-preserved
category: quoting
---

# Test: Double-Quoted Scalar with Trailing Space Preserves Quoting

A double-quoted scalar whose decoded value ends with a space must remain quoted
after formatting. Plain scalars have trailing whitespace stripped on re-parse,
so emitting unquoted would drop the trailing space and break idempotency.

## Test-Document

```yaml
"value with trailing space "
```

## Expected-Document

```yaml
"value with trailing space "
```
