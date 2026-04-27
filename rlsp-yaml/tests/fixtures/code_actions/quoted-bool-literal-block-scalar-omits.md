---
test-name: quoted-bool-literal-block-scalar-omits
category: quoted-bool
cursor: 1:3
omits-action: Convert quoted
---

# Test: No bool conversion for literal block scalar containing true

## Test-Document

```yaml
enabled: |
  true
```
