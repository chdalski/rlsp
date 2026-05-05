---
test-name: tab-to-spaces-flow-context-converts
category: tab-to-spaces
cursor: 0:0
applies-action: tabs to spaces
---

# Test: Convert tab inside a flow-style line to spaces

A tab character that appears inside a flow sequence on the cursor line is
converted to spaces by the tab-to-spaces action. The action operates on the
raw text of the line regardless of whether the surrounding structure is block
or flow style.

## Test-Document

```yaml
items: [a,	b]
```

## Expected-Document

```yaml
items: [a,  b]
```
