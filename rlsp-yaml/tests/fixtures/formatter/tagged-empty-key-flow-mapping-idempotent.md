---
test-name: tagged-empty-key-flow-mapping-idempotent
category: flow-style
idempotent: true
---

# Test: Tagged Empty Scalar Key in Flow Mapping Is Idempotent

Formatting a flow mapping with a tagged empty scalar key twice must produce
the same result. The space before the colon in `!!str : bar` ensures the
second pass does not misparse `!!str` as a tag annotation on `bar`.

## Test-Document

```yaml
{ foo: !!str, !!str : bar }
```
