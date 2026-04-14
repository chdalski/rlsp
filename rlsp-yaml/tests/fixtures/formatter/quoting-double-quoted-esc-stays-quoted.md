---
test-name: quoting-double-quoted-esc-stays-quoted
category: quoting
---

# Test: Double-Quoted Scalar With Escape Character Stays Quoted

A double-quoted scalar whose decoded value contains an ESC character (U+001B)
must stay double-quoted with the `\e` escape preserved.

## Test-Document

```yaml
key: "\e[32mgreen\e[0m"
```

## Expected-Document

```yaml
key: "\e[32mgreen\e[0m"
```
