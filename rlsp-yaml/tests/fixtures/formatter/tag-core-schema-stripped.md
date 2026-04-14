---
test-name: tag-core-schema-stripped
category: tag
---

# Test: Core Schema Tags on Collections Are Stripped

Core schema tags (`!!map`, `!!seq`) on mapping and sequence nodes are not
preserved in the formatter output. These tags resolve to `tag:yaml.org,2002:*`
URIs, which the formatter treats as implicit type annotations and drops —
matching the existing behavior for `!!str`, `!!int`, etc. on scalars.

## Test-Document

```yaml
mapping: !!map
  a: 1
sequence: !!seq
  - a
```

## Expected-Document

```yaml
mapping:
  a: 1
sequence:
  - a
```
