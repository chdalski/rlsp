---
test-name: rename-does-not-cross-boundary-preserves-doc1
category: rename
cursor: 3:6
new-name: renamed
applies-rename: true
---

# Test: Rename from doc2 does not cross document boundary to doc1

Cursor is on `&name` in doc2. Only doc2's anchor and alias are renamed;
doc1's identically-named anchor and alias are preserved.

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
doc1: &name
  ref: *name
---
doc2: &renamed
  ref: *renamed
```
