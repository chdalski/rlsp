---
test-name: tag-core-schema-suppressed-on-anchored-empty-scalar
category: tag
---

# Test: Resolver-Injected Core Schema Tag Suppressed on Anchored Empty Scalar

When a mapping value is an empty scalar with an anchor and no explicit tag in
the source, the loader's schema resolver injects `tag:yaml.org,2002:null`
automatically (with no source position). The formatter must suppress this
injected tag — re-emitting it would produce `a: &anchor !!null` on the first
format pass and break idempotency on the second.

The parser normalizes the resolved tag into the AST node's tag field without a
source position (`tag_loc: None`), so the formatter only sees the resolved tag
value, not its original source form. This fixture verifies the formatter
preserves `a: &anchor` rather than emitting `a: &anchor !!null`.

Regression fixture for conformance case 6KGN (Anchor for empty node).

## Test-Document

```yaml
a: &anchor
b: *anchor
```

## Expected-Document

```yaml
a: &anchor
b: *anchor
```
