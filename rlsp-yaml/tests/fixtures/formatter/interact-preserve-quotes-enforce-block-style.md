---
test-name: interact-preserve-quotes-enforce-block-style
category: quoting
settings:
  preserve_quotes: true
  format_enforce_block_style: true
---

# Test: preserve_quotes and format_enforce_block_style Interaction

`format_enforce_block_style` converts flow sequences to block style, but scalar
quote styles inside the converted block are still preserved by `preserve_quotes`.
`"python"` and `"http.server"` stay double-quoted (safe-plain, preserved); `"-m"`
stays double-quoted (`needs_quoting`).

## Test-Document

```yaml
command: ["python", "-m", "http.server"]
```

## Expected-Document

```yaml
command:
  - "python"
  - "-m"
  - "http.server"
```
