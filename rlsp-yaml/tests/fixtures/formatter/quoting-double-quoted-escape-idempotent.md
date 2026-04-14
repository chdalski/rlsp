---
test-name: quoting-double-quoted-escape-idempotent
category: quoting
idempotent: true
---

# Test: Double-Quoted Escape Is Idempotent

Formatting a double-quoted scalar with escape sequences twice produces the same
output. This guards against the second pass re-escaping the backslash in `\t`,
producing `\\t` on the second format.

## Test-Document

```yaml
key: "\tvalue"
```
