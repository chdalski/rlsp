---
test-name: quoting-double-quoted-safe-stripped
category: quoting
---

# Test: Double-Quoted Safe String Is Stripped to Plain

A double-quoted string that does not need quoting (no reserved keywords, no
special characters) is emitted as a plain scalar. Unnecessary quotes are noise
in most YAML documents.

## Test-Document

```yaml
value: "python"
```

## Expected-Document

```yaml
value: python
```
