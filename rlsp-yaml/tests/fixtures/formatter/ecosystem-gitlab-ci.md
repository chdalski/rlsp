---
test-name: ecosystem-gitlab-ci
category: ecosystem
idempotent: true
---

# Test: GitLab CI Round-Trip

A GitLab CI pipeline with `stages:`, `variables:`, `script:` arrays, and
`rules:` conditions. Verifies the formatter is idempotent across the common
GitLab CI patterns.

Ref: GitLab CI/CD pipeline syntax reference

## Test-Document

```yaml
stages:
  - build
  - test
  - deploy

variables:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: "1"

build:
  stage: build
  script:
    - cargo build --release
    - echo "Build complete"
  artifacts:
    paths:
      - target/release/
    expire_in: 1 hour

test:
  stage: test
  script:
    - cargo test --workspace
    - cargo clippy --all-targets
  rules:
    - if: $CI_MERGE_REQUEST_ID
    - if: $CI_COMMIT_BRANCH == "main"

deploy:
  stage: deploy
  script:
    - echo "Deploying to production"
  rules:
    - if: $CI_COMMIT_BRANCH == "main"
      when: manual
```
