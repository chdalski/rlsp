---
test-name: comment-key-line-block-mapping-stays-on-line
category: comments
---

# Test: A Comment On The Line Of A Key With A Block Mapping Stays On It

The comment on the key's line stays after the colon, above the nested entries.

## Test-Document

```yaml
k: # c
  sub: 1
```

## Expected-Document

```yaml
k:  # c
  sub: 1
```
