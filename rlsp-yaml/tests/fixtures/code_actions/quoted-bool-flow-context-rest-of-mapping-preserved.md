---
test-name: quoted-bool-flow-context-rest-of-mapping-preserved
category: quoted-bool
cursor: 0:14
applies-action: Convert quoted
---

# Test: Bool conversion in flow mapping replaces only the scalar span

## Test-Document

```yaml
config: { a: "true", b: 1 }
```

## Expected-Document

```yaml
config: { a: true, b: 1 }
```
