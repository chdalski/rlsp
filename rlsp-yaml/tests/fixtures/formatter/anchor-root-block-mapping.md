---
test-name: anchor-root-block-mapping
category: anchor
---

# Test: Anchor on a Root-Level Block Mapping

An anchor on a root-level block mapping is emitted on its own line, with the
mapping entries on subsequent lines. Inline placement (`&anchor key: val`)
would produce `&anchor key:` which is parsed as a scalar not an anchor.

## Test-Document

```yaml
&mmap
key: val
other: 42
```

## Expected-Document

```yaml
&mmap
key: val
other: 42
```
