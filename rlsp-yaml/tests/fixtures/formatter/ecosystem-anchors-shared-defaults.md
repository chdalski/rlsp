---
test-name: ecosystem-anchors-shared-defaults
category: ecosystem
---

# Test: Shared Defaults with Anchor/Alias Merge

Real-world anchor/alias pattern: shared defaults with `<<: *defaults` merge.
The anchor definition (`&defaults`) on the mapping node must be preserved
in the output — without it the alias references (`*defaults`) would be
dangling.

Ref: YAML 1.2 §6.9.2 Node Anchors, §10.1.2 Merge Key Language-Independent Type

## Test-Document

```yaml
defaults: &defaults
  retries: 3
  timeout: 30
  log_level: info

services:
  api:
    <<: *defaults
    timeout: 60
    port: 8080
  worker:
    <<: *defaults
    port: 8081
```

## Expected-Document

```yaml
defaults: &defaults
  retries: 3
  timeout: 30
  log_level: info

services:
  api:
    <<: *defaults
    timeout: 60
    port: 8080
  worker:
    <<: *defaults
    port: 8081
```
