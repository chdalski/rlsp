---
test-name: quoted-bool-flow-mapping-value-applies
category: quoted-bool
cursor: 0:19
applies-action: Convert quoted
---

# Test: Bool conversion offered for quoted bool as a flow mapping value

## Test-Document

```yaml
config: { enabled: "true" }
```

## Expected-Document

```yaml
config: { enabled: true }
```
