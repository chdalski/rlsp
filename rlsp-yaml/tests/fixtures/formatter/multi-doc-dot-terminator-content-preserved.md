---
test-name: multi-doc-dot-terminator-content-preserved
category: structure
---

# Test: Document-End Terminator (`...`) Content Preserved

The `...` document-end terminator is parsed as a document boundary. The
formatter preserves `...` when `explicit_end` is true on the document.
The `---` separator for the second document is also preserved because
`explicit_start` is true on that document.

## Test-Document

```yaml
key1: value1
...
---
key2: value2
```

## Expected-Document

```yaml
key1: value1
...
---
key2: value2
```
