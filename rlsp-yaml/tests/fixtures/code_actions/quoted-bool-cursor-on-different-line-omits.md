---
test-name: quoted-bool-cursor-on-different-line-omits
category: quoted-bool
cursor: 0:3
omits-action: Convert quoted
---

# Test: No bool conversion when cursor is on a different line from the quoted bool

## Test-Document

```yaml
a: true
b: "false"
```
