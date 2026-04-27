---
test-name: quoted-bool-multiple-bools-cursor-on-second-applies
category: quoted-bool
cursor: 0:20
applies-action: Convert quoted string to false
---

# Test: Cursor on second bool in same-line flow mapping converts false, not true

## Test-Document

```yaml
x: { a: "true", b: "false" }
```

## Expected-Document

```yaml
x: { a: "true", b: false }
```
