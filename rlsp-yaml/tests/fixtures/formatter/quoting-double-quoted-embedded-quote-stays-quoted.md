---
test-name: quoting-double-quoted-embedded-quote-stays-quoted
category: quoting
---

# Test: Double-Quoted Scalar With Leading Double-Quote Needs Quoting

A double-quoted scalar whose decoded value starts with a double-quote character
cannot be emitted as a plain scalar — the leading `"` would be parsed as the
start of a new double-quoted scalar. The formatter must re-emit it quoted.

## Test-Document

```yaml
key: "\"hello\""
```

## Expected-Document

```yaml
key: "\"hello\""
```
