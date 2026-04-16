---
test-name: preserve-quotes-idempotent
category: quoting
idempotent: true
settings:
  preserve_quotes: true
---

# Test: Preserved Quote Output Is Idempotent

Once quotes are preserved on the first formatting pass, the second pass must not
strip or re-wrap them. This fixture verifies `format(format(input)) == format(input)`
for a mix of quoted and plain scalars.

## Test-Document

```yaml
command: ["python", "-m", "http.server", "5000"]
"quoted-key": 'single-val'
plain: value
```

## Expected-Document

```yaml
command: ["python", "-m", "http.server", "5000"]
"quoted-key": 'single-val'
plain: value
```
