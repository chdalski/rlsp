---
test-name: block-scalar-colon-in-url
category: block-scalar
cursor: 0:0
applies-action: block scalar
---

# Test: URL with colon in value is fully preserved in the block scalar output

## Test-Document

```yaml
homepage: "https://example.com/very-long-path-that-exceeds-forty-chars"
```

## Expected-Document

```yaml
homepage: |
  https://example.com/very-long-path-that-exceeds-forty-chars
```
