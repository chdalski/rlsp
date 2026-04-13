---
test-name: ecosystem-gha-matrix
category: ecosystem
idempotent: true
---

# Test: GHA Matrix Round-Trip

A GitHub Actions matrix strategy workflow with flow sequences for `os:` and
`rust:`. Verifies the formatter is idempotent and preserves flow sequences.

Ref: GitHub Actions workflow syntax — matrix strategy

## Test-Document

```yaml
name: Matrix

on:
  push:
    branches: [main]

jobs:
  build:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
        rust: [stable, beta]
    steps:
      - uses: actions/checkout@v6
      - name: Build
        run: cargo build
```
