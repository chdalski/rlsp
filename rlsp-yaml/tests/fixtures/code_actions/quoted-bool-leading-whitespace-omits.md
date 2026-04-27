---
test-name: quoted-bool-leading-whitespace-omits
category: quoted-bool
cursor: 0:7
omits-action: Convert quoted
---

# Test: No bool conversion when decoded value has leading whitespace

## Test-Document

```yaml
key: " true"
```
