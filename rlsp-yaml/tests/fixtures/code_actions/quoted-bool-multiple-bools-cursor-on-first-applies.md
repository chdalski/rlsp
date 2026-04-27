---
test-name: quoted-bool-multiple-bools-cursor-on-first-applies
category: quoted-bool
cursor: 0:9
applies-action: Convert quoted string to true
---

# Test: Cursor on first bool in same-line flow mapping converts true, not false

## Test-Document

```yaml
x: { a: "true", b: "false" }
```

## Expected-Document

```yaml
x: { a: true, b: "false" }
```
