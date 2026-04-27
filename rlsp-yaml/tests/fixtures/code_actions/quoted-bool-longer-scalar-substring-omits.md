---
test-name: quoted-bool-longer-scalar-substring-omits
category: quoted-bool
cursor: 0:10
omits-action: Convert quoted
---

# Test: No bool conversion for longer scalar that contains true as substring

## Test-Document

```yaml
msg: 'status ''true'' reported'
```
