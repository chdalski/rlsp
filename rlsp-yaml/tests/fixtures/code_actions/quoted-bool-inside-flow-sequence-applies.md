---
test-name: quoted-bool-inside-flow-sequence-applies
category: quoted-bool
cursor: 0:9
applies-action: Convert quoted
---

# Test: Bool conversion offered for quoted bool inside flow sequence

## Test-Document

```yaml
items: ["true", "false"]
```

## Expected-Document

```yaml
items: [true, "false"]
```
