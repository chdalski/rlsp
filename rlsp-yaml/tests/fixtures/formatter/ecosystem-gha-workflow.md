---
test-name: ecosystem-gha-workflow
category: ecosystem
idempotent: true
---

# Test: GHA Workflow Round-Trip

A GitHub Actions workflow with `on:` key, flow sequences in `branches`, and
blank lines between sections. Verifies the formatter is idempotent.

Ref: GitHub Actions workflow syntax

## Test-Document

```yaml
name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    name: Test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - name: Run tests
        run: cargo test --workspace
      - name: Check format
        run: cargo fmt --check
```
