---
test-name: anchor-root-block-sequence-idempotent
category: anchor
idempotent: true
---

# Test: Anchor on Root Block Sequence Is Idempotent

Formatting a root-level anchored block sequence twice produces the same result.

## Test-Document

```yaml
&sequence
- a
- b
- c
```
