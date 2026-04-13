---
test-name: quoting-yes-no-plain-keys-not-quoted
category: quoting
---

# Test: Plain `yes` and `no` as Mapping Key and Value Not Quoted

`yes` and `no` are YAML 1.1 boolean keywords when used as values, but the
formatter (with default YAML 1.2 settings) treats them as plain scalars.
When used as plain mapping keys or values in the source, they must not be
quoted in the output.

## Test-Document

```yaml
yes: no
```

## Expected-Document

```yaml
yes: no
```
