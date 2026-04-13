---
test-name: quoting-colon-space-in-string
category: quoting
---

# Test: Quoted String with Colon-Space Retained

A double-quoted string `"key: value"` contains `: ` which the parser strips
(known parser limitation: the space after `:` in a double-quoted string is
lost). As a result, the stored string is `"key:value"` and `needs_quoting`
no longer triggers. The formatter emits the (modified) string without re-quoting.

This test documents the known parser limitation and verifies the formatter
does not crash on this input.

## Test-Document

```yaml
value: "key: value"
```

## Expected-Document

```yaml
value: "key: value"
```
