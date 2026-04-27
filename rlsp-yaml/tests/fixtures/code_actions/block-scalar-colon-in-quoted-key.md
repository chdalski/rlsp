---
test-name: block-scalar-colon-in-quoted-key
category: block-scalar
cursor: 0:0
applies-action: block scalar
---

# Test: Mapping value is converted correctly when the key itself is a quoted string containing a colon

## Test-Document

```yaml
"foo:bar": "this is a long mapping value that exceeds forty characters"
```

## Expected-Document

```yaml
"foo:bar": |
  this is a long mapping value that exceeds forty characters
```
