---
test-name: anchor-idempotent
category: anchor
idempotent: true
---

# Test: Anchor and Alias Idempotency

A document with multiple anchor definitions and alias references produces the
same output when formatted twice. This guards against the formatter
double-emitting anchors or dropping them on the second pass.

## Test-Document

```yaml
defaults: &defaults
  timeout: 30
  retries: 3
service: &service
  <<: *defaults
  port: 8080
replica:
  <<: *service
  port: 9090
```
