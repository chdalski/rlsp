---
test-name: quoting-single-quoted-greeting-stripped
category: quoting
---

# Test: Single-Quoted Greeting Value Stripped to Plain

The word "hello" has no special meaning in YAML and does not need quoting.
The formatter strips the single quotes.

## Test-Document

```yaml
greeting: 'hello'
```

## Expected-Document

```yaml
greeting: hello
```
