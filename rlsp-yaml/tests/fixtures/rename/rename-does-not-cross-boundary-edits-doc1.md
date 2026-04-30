---
test-name: rename-does-not-cross-boundary-edits-doc1
category: rename
cursor: 0:6
new-name: renamed
applies-rename: true
---

# Test: Rename from doc1 does not cross document boundary to doc2

Cursor is on `&name` in doc1. Only doc1's anchor and alias are renamed;
doc2's identically-named anchor and alias are untouched.

## Test-Document

```yaml
doc1: &name
  ref: *name
---
doc2: &name
  ref: *name
```

## Expected-Document

```yaml
doc1: &renamed
  ref: *renamed
---
doc2: &name
  ref: *name
```
