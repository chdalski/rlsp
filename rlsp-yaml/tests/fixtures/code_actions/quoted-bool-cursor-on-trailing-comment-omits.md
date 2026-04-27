---
test-name: quoted-bool-cursor-on-trailing-comment-omits
category: quoted-bool
cursor: 0:14
omits-action: Convert quoted
---

# Test: No bool conversion when cursor is on a trailing comment

## Test-Document

```yaml
key: "true"  # comment
```
