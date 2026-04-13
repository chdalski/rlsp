---
test-name: quoting-double-quoted-greeting-stripped
category: quoting
---

# Test: Double-Quoted Greeting Value Stripped to Plain

The word "hello" has no special meaning in YAML and does not need quoting.
The formatter strips the double quotes.

## Test-Document

```yaml
greeting: "hello"
```

## Expected-Document

```yaml
greeting: hello
```
