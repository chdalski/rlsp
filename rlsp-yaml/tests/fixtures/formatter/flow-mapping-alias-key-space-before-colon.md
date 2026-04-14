---
test-name: flow-mapping-alias-key-space-before-colon
category: anchor
idempotent: true
---

# Test: Flow Mapping Alias Key Has Space Before Colon

When an alias is used as a key in a flow mapping, the colon separator must have
a space before it: `*name : value` not `*name: value`. Without the space, the
colon is part of the alias name (`a:`) on re-parse, breaking idempotency.

## Test-Document

```yaml
{ *ref : value, *other : 42 }
```
