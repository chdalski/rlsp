---
test-name: quoting-single-quote-key-needs-quoting
category: quoting
settings:
  single_quote: true
---

# Test: Key That Genuinely Needs Quoting Is Still Quoted With `single_quote: true`

When `single_quote: true` is set and a mapping key requires quoting (e.g., it
looks like a number), the key is still quoted using its original style. The
`single_quote` option suppresses style-preference quoting on keys, but does not
override mandatory quoting — nor does it change the original quote style of the key.

## Test-Document

```yaml
"123": value
```

## Expected-Document

```yaml
"123": 'value'
```
