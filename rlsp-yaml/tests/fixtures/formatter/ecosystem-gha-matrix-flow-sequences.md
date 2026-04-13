---
test-name: ecosystem-gha-matrix-flow-sequences
category: ecosystem
---

# Test: GHA Matrix Flow Sequences Preserved

Flow sequences in a GHA matrix strategy (`os: [...]`, `rust: [...]`) must be
preserved after formatting — not converted to block sequences.

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

## Expected-Document

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
