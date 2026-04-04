---
paths:
  - "**/*.ts"
  - "**/*.tsx"
  - "**/*.py"
  - "**/*.rs"
  - "**/*.go"
---

# Integration Test Requirement

When a task adds user-facing behavior, unit tests alone are
not sufficient to verify delivery. A function that is
correct in isolation but never called from production code
is dead infrastructure — it passes all quality checks while
delivering nothing to users. Integration tests that exercise
features through production entry points catch this failure
mode mechanically.

## The Dead-Infrastructure Pattern

This pattern has caused repeated incomplete deliveries:

1. A plan specifies both infrastructure (parsing, data
   structures, utility functions) and integration (wiring
   into the server, behavioral activation)
2. The developer builds the infrastructure with thorough
   unit tests
3. The reviewer verifies the code is correct and
   well-tested
4. The integration step — connecting the new code to the
   production code path — is not delivered
5. The task is marked complete

Every quality gate passes because the code *is* correct.
Unit tests prove it works when called directly. But no user
can reach the feature because the production entry point
never calls it. A `pub fn` with comprehensive tests and no
call site outside `#[cfg(test)]` is the hallmark of this
pattern.

## The Rule

Every task that adds or modifies user-facing behavior must
include at least one integration test that exercises the
feature through the production entry point.

**Production entry point** means whatever real users
interact with — the server handler, the CLI command, the
API endpoint, the library's public module interface. The
test must reach the new code through the same path a user
would.

## Unit Tests vs. Integration Tests

Both are necessary. They catch different failure modes:

**Unit tests** verify that a function produces correct
output for given input. They call the function directly and
prove the logic works in isolation.

**Integration tests** verify that the feature is reachable
and works end-to-end. They call the production entry point
with input that exercises the new code path and prove the
feature is wired in.

A task that delivers only unit tests has proved the
building blocks work but has not proved the building is
standing.

## The Heuristic

After completing a task, apply this check: if you deleted
every new public function and only test files would break,
you have dead infrastructure. An integration test through
the production entry point would also break — its absence
is the signal that the feature is not wired in.

## When This Applies

- **New feature** (new endpoint, new command, new
  capability) — always requires integration tests through
  the production entry point
- **Modified behavior** (changing how a feature works) —
  verify existing integration tests exercise the changed
  path; add new ones if they do not
- **Internal refactoring** with no behavior change — unit
  tests are sufficient; existing integration tests confirm
  behavior is preserved
- **Explicitly staged infrastructure** where the plan says
  "build the parser; integration comes in task N" — unit
  tests are sufficient for this task, but verify the
  integration task exists and is not marked complete until
  integration tests pass

## What This Does NOT Mean

This rule does not require end-to-end system tests for
every change. An integration test can be lightweight — a
test that sends a request to the server and checks the
response, or a test that calls the library's public API
and verifies the output. The bar is "exercises the feature
through the production code path," not "replicates a full
production environment."

Language-specific testing rules (`lang-*-testing.md`)
describe how to write tests in each language. This rule
describes which *kind* of test a task must include to
count as fully delivered.
