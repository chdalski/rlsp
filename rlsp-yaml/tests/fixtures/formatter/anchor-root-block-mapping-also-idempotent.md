---
test-name: anchor-root-block-mapping-also-idempotent
category: anchor
idempotent: true
---

# Test: Anchor on Root Block Mapping Is Idempotent

Formatting a root-level anchored block mapping twice produces the same result.

## Test-Document

```yaml
&mymap
a: 1
b: 2
```
