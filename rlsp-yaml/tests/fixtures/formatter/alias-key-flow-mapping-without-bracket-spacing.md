---
test-name: alias-key-flow-mapping-without-bracket-spacing
category: anchor
settings:
  bracket_spacing: false
---

# Test: Alias Key Space Before Colon With bracket_spacing False

The space before `:` for an alias key in a flow mapping is a syntactic
requirement of YAML, not a formatting preference. It must be present even
when `bracket_spacing: false` disables spaces inside braces.

## Test-Document

```yaml
{*ref : value, *other : 42}
```

## Expected-Document

```yaml
{*ref : value, *other : 42}
```
