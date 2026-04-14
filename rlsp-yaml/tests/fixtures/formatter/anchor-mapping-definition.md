---
test-name: anchor-mapping-definition
category: anchor
---

# Test: Anchor on a Block Mapping Value

An anchor definition on a block mapping value is emitted on the key line, before
the block content. The anchor name appears between the colon separator and the
newline that opens the indented block.

## Test-Document

```yaml
defaults: &defaults
  timeout: 30
```

## Expected-Document

```yaml
defaults: &defaults
  timeout: 30
```
