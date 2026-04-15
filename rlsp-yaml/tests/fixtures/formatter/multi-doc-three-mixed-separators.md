---
test-name: multi-doc-three-mixed-separators
category: structure
---

# Test: Three-Document File with Mixed `---` and `...` Separators

A three-document file using a mix of `...` and `---` separators is parsed as
three separate documents. The formatter preserves `...` when `explicit_end` is
true and `---` when `explicit_start` is true. Doc1 ends with `...`; doc2 starts
implicitly but is separated by `---` between docs; doc3 starts explicitly with
`---`.

## Test-Document

```yaml
key: value
...
key2: value2
---
key3: value3
```

## Expected-Document

```yaml
key: value
...
---
key2: value2
---
key3: value3
```
