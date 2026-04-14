---
test-name: anchor-flow-collection-stays-inline
category: anchor
---

# Test: Anchor on Flow Collection Stays Inline

An anchor on a flow collection (sequence or mapping) is placed inline before
the opening bracket. Block collections require a line break; flow collections
do not.

## Test-Document

```yaml
seq: &items [a, b, c]
map: &coords {x: 1, y: 2}
```

## Expected-Document

```yaml
seq: &items [a, b, c]
map: &coords { x: 1, y: 2 }
```
