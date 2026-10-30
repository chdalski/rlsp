---
test-name: flow-quoted-item-with-comma-keeps-quotes
category: flow-style
settings:
  print_width: 80
---

# Test: Quoted Flow Items That Contain Flow Indicators Keep Their Quotes

Inside a flow collection `,`, `[`, `]`, `{` and `}` end a plain scalar
(`ns-plain-safe-in` in YAML 1.2). A quoted item whose text holds one of them
stays quoted, otherwise the output would re-parse as different data.

## Test-Document

```yaml
- [ a, "Up, up and away!", b ]
- {b: "x, y", "c]d": 1}
- [ 'p, q' ]
```

## Expected-Document

```yaml
- [a, "Up, up and away!", b]
- { b: "x, y", "c]d": 1 }
- ["p, q"]
```
