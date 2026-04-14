---
test-name: tagged-empty-key-space-before-colon-in-flow-mapping
category: flow-style
---

# Test: Tagged Empty Scalar Key in Flow Mapping Has Space Before Colon

When a tagged empty scalar is used as a key in a flow mapping, the colon
separator must have a leading space: `!!str : val` not `!!str: val`. Without
the space, a re-parser reads `!!str:` as tag `tag:yaml.org,2002:str:` (a tag
URI with `:` appended), breaking idempotency.

This exercises the `key_needs_space_before_colon` path in `flow_mapping_to_doc`.

## Test-Document

```yaml
{ foo : !!str, !!str : bar }
```

## Expected-Document

```yaml
{ foo: !!str, !!str : bar }
```
