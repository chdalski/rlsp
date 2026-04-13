---
test-name: quoting-single-quoted-safe-stripped
category: quoting
---

# Test: Single-Quoted Safe String Is Stripped to Plain

A single-quoted string that does not need quoting is emitted as a plain scalar.
The formatter removes unnecessary quoting regardless of the original quote style.

## Test-Document

```yaml
value: 'hello'
```

## Expected-Document

```yaml
value: hello
```
