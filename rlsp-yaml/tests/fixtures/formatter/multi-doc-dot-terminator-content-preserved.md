---
test-name: multi-doc-dot-terminator-content-preserved
category: structure
---

# Test: Document-End Terminator (`...`) Content Preserved

The `...` document-end terminator is parsed as a document boundary, same as
`---`. The formatter always emits `---` separators between documents, so `...`
terminators are not preserved in the output — but the content of both documents
is preserved.

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
---
key2: value2
```
