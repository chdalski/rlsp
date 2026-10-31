---
test-name: explicit-key-empty-key
category: explicit-key
---

# Test: Empty (Null) Key Renders as `: value`

When the key is an explicit null/empty scalar (`? ` with no content), the entry
renders as `: value` — no `?` prefix. Exercises the `is_empty_key` path in
`entry_to_parts`.

## Test-Document

```yaml
? 
: bar
```

## Expected-Document

```yaml
: bar
```
