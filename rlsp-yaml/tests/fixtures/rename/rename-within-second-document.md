---
test-name: rename-within-second-document
category: rename
cursor: 2:6
new-name: other
applies-rename: true
---

# Test: Rename within the second document in a multi-document stream

Cursor is on `&name` in doc2. Only doc2's anchor and alias are renamed;
doc1's identically-named anchor and alias are untouched.

## Test-Document

```yaml
doc1: &name
---
doc2: &name
  ref: *name
```

## Expected-Document

```yaml
doc1: &name
---
doc2: &other
  ref: *other
```
