---
test-name: block-to-flow-keeps-comment-trailing-blanks-and-tab
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: The comment after the last entry keeps its blanks before it and its trailing blanks and tab. The fixture holds invisible characters: a tab before `#` and spaces plus a tab after `keep`; an editor that strips trailing whitespace breaks it.

## Test-Document

```yaml
a:
  b: 1	# keep   	
d: 1
```

## Expected-Document

```yaml
a: { b: 1 }	# keep   	
d: 1
```
