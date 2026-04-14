---
test-name: alias-key-space-before-colon-in-flow-mapping
category: anchor
---

# Test: Alias Key in Flow Mapping Has Space Before Colon

When an alias is used as a key in a flow mapping, the colon separator must have
a leading space: `*a : val` not `*a: val`. Without the space, a re-parser reads
`*a:` as alias name `a:`, breaking idempotency.

## Test-Document

```yaml
{ *ref : value, *other : 42 }
```

## Expected-Document

```yaml
{ *ref : value, *other : 42 }
```
