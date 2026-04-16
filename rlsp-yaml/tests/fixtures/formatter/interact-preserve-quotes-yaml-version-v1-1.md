---
test-name: interact-preserve-quotes-yaml-version-v1-1
category: quoting
settings:
  preserve_quotes: true
  yaml_version: "1.1"
---

# Test: preserve_quotes With YAML 1.1 Reserved Keywords

Under YAML 1.1, `yes` and `no` are reserved boolean keywords that `needs_quoting`
flags — they already stay quoted via the `needs_quoting=true` branch. With
`preserve_quotes: true`, single-quoting is preserved as well (not flipped to
double). Guards against the new preserve branch interfering with the 1.1 path.

## Test-Document

```yaml
flag: 'yes'
other: 'no'
```

## Expected-Document

```yaml
flag: 'yes'
other: 'no'
```
