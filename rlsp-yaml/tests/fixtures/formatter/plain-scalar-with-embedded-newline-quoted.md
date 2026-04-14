---
test-name: plain-scalar-with-embedded-newline-quoted
category: quoting
---

# Test: Scalar With Embedded Newline Stays Double-Quoted

A scalar whose decoded value contains an embedded newline (`\n`) cannot be
emitted as a plain scalar — splitting the text across two lines would cause the
second line to be parsed as a new mapping key or value at the wrong indentation.
The formatter preserves (or produces) double-quoted form with the `\n` escaped.

## Test-Document

```yaml
key: "value with\ntabs"
```

## Expected-Document

```yaml
key: "value with\ntabs"
```
