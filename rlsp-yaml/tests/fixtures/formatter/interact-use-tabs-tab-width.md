---
test-name: interact-use-tabs-tab-width
category: interaction
settings:
  use_tabs: true
  tab_width: 4
---

# Test: use_tabs + tab_width — Tab Indentation Overrides tab_width for Regular Indentation

When `use_tabs: true`, the formatter uses tab characters for block indentation
and `tab_width` is ignored for structural indentation. Each level of nesting
produces one tab character, regardless of `tab_width`.

The tab_width: 4 setting is non-default but produces the same tab-indented
output as tab_width: 2 for regular block mappings — both emit one tab per
indent level. The `tab_width` field only affects the explicit indentation
indicator digit emitted for block scalars with leading-space content, not the
tab character count for structural indent.

## Test-Document

```yaml
outer:
  inner: value
```

## Expected-Document

```yaml
outer:
	inner: value
```
