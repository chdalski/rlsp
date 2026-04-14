---
test-name: quoting-double-quoted-safe-still-stripped
category: quoting
---

# Test: Double-Quoted Safe String Is Still Stripped to Plain

A double-quoted scalar whose decoded value contains no control characters and
no structural characters that need quoting is still stripped to a plain scalar.
This ensures the `requires_double_quoting` gate does not over-match.

## Test-Document

```yaml
greeting: "hello world"
```

## Expected-Document

```yaml
greeting: hello world
```
