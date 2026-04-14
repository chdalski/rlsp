---
test-name: alias-key-flow-mapping-idempotent
category: anchor
idempotent: true
---

# Test: Alias Key Flow Mapping Is Idempotent

Formatting a flow mapping with alias keys twice produces the same result.
The space before the colon must be present in the first output so the second
format pass does not misparse the alias name.

## Test-Document

```yaml
{ *ref : value, *other : 42 }
```
