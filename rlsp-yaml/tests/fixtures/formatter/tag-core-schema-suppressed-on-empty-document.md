---
test-name: tag-core-schema-suppressed-on-empty-document
category: tag
---

# Test: Resolver-Injected Core Schema Tag Suppressed on Empty Document

When a document contains only an implicit null value (no content after `---`),
the loader's schema resolver injects `tag:yaml.org,2002:null` automatically
(with no source position). The formatter must suppress this injected tag —
emitting `!!null` on the first format pass produces output that fails to
re-parse cleanly ("a node may not have more than one tag") because the second
format pass sees `!!null` as a user-authored tag and tries to add another one.

The parser normalizes the resolved tag into the AST node's tag field without a
source position (`tag_loc: None`), so the formatter only sees the resolved tag
value, not its original source form. This fixture verifies the formatter
preserves `---\n---` rather than emitting `---\n!!null\n---\n!!null`.

Regression fixture for conformance case 6XDY (Two document start markers).

## Test-Document

```yaml
---
---
```

## Expected-Document

```yaml
---

---
```
