---
test-name: quoted-bool-title-uses-plain-value
category: quoted-bool
cursor: 0:8
applies-action: Convert quoted string to false
---

# Test: Bool conversion action title uses the decoded plain value

## Test-Document

```yaml
flag: 'false'
```

## Expected-Document

```yaml
flag: false
```
