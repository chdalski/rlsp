---
test-name: quoting-double-quoted-key-with-newline-stays-quoted
category: quoting
---

# Test: Double-Quoted Mapping Key With Newline Stays Quoted

A double-quoted mapping key whose decoded value contains a newline must stay
double-quoted. Without quoting, the newline would split the key across lines
producing invalid YAML structure.

Ref: yaml-test-suite 6SLA[0]

## Test-Document

```yaml
"foo\nbar:baz\tx \\$%^&*()x": 23
```

## Expected-Document

```yaml
"foo\nbar:baz\tx \\$%^&*()x": 23
```
